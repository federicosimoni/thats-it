//! Step execution: placeholder substitution, argv-only process spawning, timeouts, cancel.
//!
//! Security rules (project brief §5): commands run as argv arrays and **never through a
//! shell**; placeholders are substituted inside individual arguments only.

use std::fmt;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use crate::conversion::{Conversion, Step};
use crate::output::stem_of;
use crate::placeholder::{self, Placeholder, TemplateError};
use crate::tools::ToolFinder;

/// How often a running step is checked for completion, timeout and cancellation.
const POLL_INTERVAL: Duration = Duration::from_millis(15);
/// How much of a failing tool's stderr is kept for the error message.
const STDERR_TAIL_BYTES: usize = 4096;
/// How long to wait for a failed tool's stderr after it exited.
const STDERR_GRACE: Duration = Duration::from_millis(500);

/// A cheap, clonable cancellation flag shared between the UI and a running job.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Values of the placeholders for one job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepContext {
    pub input: PathBuf,
    pub output: PathBuf,
    pub stem: String,
    pub dir: PathBuf,
    pub tmpdir: PathBuf,
}

impl StepContext {
    pub fn value(&self, p: Placeholder) -> String {
        match p {
            Placeholder::Input => self.input.to_string_lossy().into_owned(),
            Placeholder::Output => self.output.to_string_lossy().into_owned(),
            Placeholder::Stem => self.stem.clone(),
            Placeholder::Dir => self.dir.to_string_lossy().into_owned(),
            Placeholder::TmpDir => self.tmpdir.to_string_lossy().into_owned(),
        }
    }

    /// Substitutes placeholders in one argument.
    pub fn substitute(&self, template: &str) -> Result<String, TemplateError> {
        placeholder::substitute(template, |p| self.value(p))
    }

    /// Substitutes placeholders in every string inside an engine `args` table.
    pub fn substitute_table(&self, table: &toml::Table) -> Result<toml::Table, TemplateError> {
        table
            .iter()
            .map(|(k, v)| Ok((k.clone(), self.substitute_value(v)?)))
            .collect()
    }

    fn substitute_value(&self, value: &toml::Value) -> Result<toml::Value, TemplateError> {
        Ok(match value {
            toml::Value::String(s) => toml::Value::String(self.substitute(s)?),
            toml::Value::Array(items) => toml::Value::Array(
                items
                    .iter()
                    .map(|v| self.substitute_value(v))
                    .collect::<Result<_, _>>()?,
            ),
            toml::Value::Table(t) => toml::Value::Table(self.substitute_table(t)?),
            other => other.clone(),
        })
    }
}

/// Runs builtin engines. Implemented by the app (AppKit/PDFKit engines) and by the core
/// (pure-Rust engines, added in Phase 3).
pub trait EngineRunner: Sync {
    /// Runs engine `name` (including the `builtin:` prefix) with already-substituted
    /// `args`. Long-running engines should poll `cancel` and return early when set.
    fn run(
        &self,
        name: &str,
        args: &toml::Table,
        ctx: &StepContext,
        cancel: &CancelToken,
    ) -> Result<(), String>;
}

/// An [`EngineRunner`] with no engines at all.
pub struct NoEngines;

impl EngineRunner for NoEngines {
    fn run(
        &self,
        name: &str,
        _: &toml::Table,
        _: &StepContext,
        _: &CancelToken,
    ) -> Result<(), String> {
        Err(format!("engine {name} is not available in this build"))
    }
}

/// Why a pipeline did not complete.
#[derive(Debug)]
pub enum PipelineError {
    Cancelled,
    ToolNotFound {
        step: usize,
        program: String,
    },
    Template {
        step: usize,
        error: TemplateError,
    },
    Spawn {
        step: usize,
        program: String,
        error: io::Error,
    },
    Failed {
        step: usize,
        program: String,
        status: ExitStatus,
        stderr: String,
    },
    TimedOut {
        step: usize,
        program: String,
        after: Duration,
    },
    Engine {
        step: usize,
        name: String,
        message: String,
    },
    TempDir(io::Error),
}

impl fmt::Display for PipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PipelineError::Cancelled => write!(f, "cancelled"),
            PipelineError::ToolNotFound { step, program } => {
                write!(f, "step {step}: tool {program:?} not found")
            }
            PipelineError::Template { step, error } => write!(f, "step {step}: {error}"),
            PipelineError::Spawn {
                step,
                program,
                error,
            } => {
                write!(f, "step {step}: cannot start {program:?}: {error}")
            }
            PipelineError::Failed {
                step,
                program,
                status,
                stderr,
            } => {
                write!(f, "step {step}: {program:?} failed ({status})")?;
                if !stderr.trim().is_empty() {
                    write!(f, ": {}", stderr.trim())?;
                }
                Ok(())
            }
            PipelineError::TimedOut {
                step,
                program,
                after,
            } => {
                write!(
                    f,
                    "step {step}: {program:?} timed out after {}s",
                    after.as_secs()
                )
            }
            PipelineError::Engine {
                step,
                name,
                message,
            } => {
                write!(f, "step {step}: {name}: {message}")
            }
            PipelineError::TempDir(e) => write!(f, "cannot create a temporary directory: {e}"),
        }
    }
}

impl std::error::Error for PipelineError {}

/// A private temporary directory removed on drop.
#[derive(Debug)]
pub struct JobTempDir(PathBuf);

impl JobTempDir {
    pub fn new() -> io::Result<Self> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir();
        loop {
            let name = format!(
                "thatsit-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            );
            let path = base.join(name);
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for JobTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Runs every step of `conversion` for one input, writing the result to `output`
/// (normally [`crate::output::PendingOutput::temp_path`]).
pub fn run(
    conversion: &Conversion,
    input: &Path,
    output: &Path,
    tools: &ToolFinder,
    engines: &dyn EngineRunner,
    cancel: &CancelToken,
) -> Result<(), PipelineError> {
    let tmp = JobTempDir::new().map_err(PipelineError::TempDir)?;
    let ctx = StepContext {
        input: input.to_path_buf(),
        output: output.to_path_buf(),
        stem: stem_of(input),
        dir: input.parent().unwrap_or(Path::new("/")).to_path_buf(),
        tmpdir: tmp.path().to_path_buf(),
    };

    for (i, step) in conversion.steps.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(PipelineError::Cancelled);
        }
        match step {
            Step::Command { argv, timeout } => {
                run_command(i, argv, *timeout, &ctx, tools, cancel)?;
            }
            Step::Engine { name, args, .. } => {
                let args = ctx
                    .substitute_table(args)
                    .map_err(|error| PipelineError::Template { step: i, error })?;
                // `input` / `output` args redirect the engine (multi-step pipelines that
                // pass intermediate files through {tmpdir}).
                let mut step_ctx = ctx.clone();
                if let Some(toml::Value::String(p)) = args.get("input") {
                    step_ctx.input = PathBuf::from(p);
                }
                if let Some(toml::Value::String(p)) = args.get("output") {
                    step_ctx.output = PathBuf::from(p);
                }
                engines
                    .run(name, &args, &step_ctx, cancel)
                    .map_err(|message| PipelineError::Engine {
                        step: i,
                        name: name.clone(),
                        message,
                    })?;
                if cancel.is_cancelled() {
                    return Err(PipelineError::Cancelled);
                }
            }
        }
    }
    Ok(())
}

fn run_command(
    step: usize,
    argv: &[String],
    timeout: Duration,
    ctx: &StepContext,
    tools: &ToolFinder,
    cancel: &CancelToken,
) -> Result<(), PipelineError> {
    let program = argv.first().cloned().unwrap_or_default();
    let resolved = tools
        .resolve(&program)
        .ok_or_else(|| PipelineError::ToolNotFound {
            step,
            program: program.clone(),
        })?;
    let args = argv[1..]
        .iter()
        .map(|a| ctx.substitute(a))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| PipelineError::Template { step, error })?;

    let mut child = own_process_group(&mut Command::new(&resolved))
        .args(&args)
        .current_dir(&ctx.tmpdir)
        .env("PATH", tools.path_env())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| PipelineError::Spawn {
            step,
            program: program.clone(),
            error,
        })?;

    // Drain stderr on a thread so a chatty tool can never block on a full pipe. The
    // result comes back through a channel so we never wait forever for it: a grandchild
    // process may keep the pipe open after the tool itself has exited.
    let (stderr_tx, stderr_rx) = mpsc::channel();
    if let Some(mut pipe) = child.stderr.take() {
        thread::spawn(move || {
            let mut tail = Vec::new();
            let mut buf = [0u8; 4096];
            while let Ok(n) = pipe.read(&mut buf) {
                if n == 0 {
                    break;
                }
                tail.extend_from_slice(&buf[..n]);
                if tail.len() > STDERR_TAIL_BYTES {
                    tail.drain(..tail.len() - STDERR_TAIL_BYTES);
                }
            }
            let _ = stderr_tx.send(String::from_utf8_lossy(&tail).into_owned());
        });
    }

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok(());
                }
                let stderr = stderr_rx.recv_timeout(STDERR_GRACE).unwrap_or_default();
                return Err(PipelineError::Failed {
                    step,
                    program,
                    status,
                    stderr,
                });
            }
            Ok(None) => {}
            Err(error) => {
                kill_tree(&mut child);
                return Err(PipelineError::Spawn {
                    step,
                    program,
                    error,
                });
            }
        }
        let cancelled = cancel.is_cancelled();
        if cancelled || started.elapsed() >= timeout {
            kill_tree(&mut child);
            return Err(if cancelled {
                PipelineError::Cancelled
            } else {
                PipelineError::TimedOut {
                    step,
                    program,
                    after: timeout,
                }
            });
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// Puts the child in its own process group so that [`kill_tree`] can stop it together
/// with any helper processes it started.
fn own_process_group(cmd: &mut Command) -> &mut Command {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd
}

/// Kills the child's whole process group (falling back to the child alone) and reaps it.
fn kill_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        // `/bin/kill -KILL -- -<pgid>` signals the whole group; using the system tool keeps
        // this crate free of `unsafe` FFI. The group id equals the child's pid.
        let _ = Command::new("/bin/kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::conversion::Origin;
    use std::sync::Mutex;

    /// Tools for tests: the real system dirs (for `cp`, `sh`, `sleep`…).
    fn tools() -> ToolFinder {
        ToolFinder::with_dirs(["/usr/bin".into(), "/bin".into()])
    }

    fn conversion(requires: &[&str], steps: &str) -> Conversion {
        let req = requires
            .iter()
            .map(|s| format!("{s:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        let text = format!(
            "schema = 1\nid = \"test\"\ncategory = \"utilities\"\naccepts = [\"public.item\"]\n\
             output_ext = \"out\"\nrequires = [{req}]\nlabel.en = \"Test\"\n{steps}"
        );
        Conversion::from_toml(&text, Origin::Bundled, None).unwrap()
    }

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("my file.txt");
        fs::write(&input, "hello").unwrap();
        let output = dir.path().join("result.out");
        (dir, input, output)
    }

    #[test]
    fn runs_argv_with_placeholders_containing_spaces() {
        let (_d, input, output) = setup();
        let c = conversion(
            &["cp"],
            "[[steps]]\ncmd = [\"cp\", \"{input}\", \"{output}\"]",
        );
        run(
            &c,
            &input,
            &output,
            &tools(),
            &NoEngines,
            &CancelToken::new(),
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&output).unwrap(), "hello");
    }

    #[test]
    fn arguments_are_never_shell_interpreted() {
        let (dir, input, output) = setup();
        // If this went through a shell, `$(touch pwned)` would create a file.
        let c = conversion(
            &["printf"],
            "[[steps]]\ncmd = [\"/bin/sh\", \"-c\", \"printf %s \\\"$0\\\" > \\\"$1\\\"\", \
             \"$(touch pwned) {stem}\", \"{output}\"]",
        );
        run(
            &c,
            &input,
            &output,
            &tools(),
            &NoEngines,
            &CancelToken::new(),
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(&output).unwrap(),
            "$(touch pwned) my file"
        );
        assert!(!dir.path().join("pwned").exists());
    }

    #[test]
    fn multi_step_pipeline_uses_tmpdir() {
        let (_d, input, output) = setup();
        let c = conversion(
            &["cp", "mkdir"],
            r#"
            [[steps]]
            cmd = ["mkdir", "{tmpdir}/work"]
            [[steps]]
            cmd = ["cp", "{input}", "{tmpdir}/work/{stem}.tmp"]
            [[steps]]
            cmd = ["cp", "{tmpdir}/work/{stem}.tmp", "{output}"]
            "#,
        );
        run(
            &c,
            &input,
            &output,
            &tools(),
            &NoEngines,
            &CancelToken::new(),
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&output).unwrap(), "hello");
    }

    #[test]
    fn reports_failures_with_stderr() {
        let (_d, input, output) = setup();
        let c = conversion(
            &["cp"],
            "[[steps]]\ncmd = [\"cp\", \"/no/such/file\", \"{output}\"]",
        );
        let err = run(
            &c,
            &input,
            &output,
            &tools(),
            &NoEngines,
            &CancelToken::new(),
        )
        .unwrap_err();
        match &err {
            PipelineError::Failed {
                step: 0,
                program,
                stderr,
                ..
            } => {
                assert_eq!(program, "cp");
                assert!(stderr.contains("/no/such/file"), "{stderr}");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(err.to_string().contains("failed"));
    }

    #[test]
    fn missing_tool() {
        let (_d, input, output) = setup();
        let c = conversion(
            &["thatsit-no-such-tool"],
            "[[steps]]\ncmd = [\"thatsit-no-such-tool\", \"{output}\"]",
        );
        let err = run(
            &c,
            &input,
            &output,
            &tools(),
            &NoEngines,
            &CancelToken::new(),
        )
        .unwrap_err();
        assert!(matches!(err, PipelineError::ToolNotFound { step: 0, .. }));
    }

    #[test]
    fn times_out() {
        let (_d, input, output) = setup();
        let mut c = conversion(
            &["sleep"],
            "[[steps]]\ncmd = [\"/bin/sh\", \"-c\", \"sleep 10\", \"{output}\"]",
        );
        if let Step::Command { timeout, .. } = &mut c.steps[0] {
            *timeout = Duration::from_millis(200);
        }
        let started = Instant::now();
        let err = run(
            &c,
            &input,
            &output,
            &tools(),
            &NoEngines,
            &CancelToken::new(),
        )
        .unwrap_err();
        assert!(matches!(err, PipelineError::TimedOut { .. }), "{err:?}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn cancels_running_and_pending_steps() {
        let (_d, input, output) = setup();
        let c = conversion(
            &["sleep"],
            "[[steps]]\ncmd = [\"/bin/sh\", \"-c\", \"sleep 10\", \"{output}\"]",
        );
        let cancel = CancelToken::new();
        let c2 = cancel.clone();
        let t = thread::spawn(move || {
            thread::sleep(Duration::from_millis(150));
            c2.cancel();
        });
        let started = Instant::now();
        let err = run(&c, &input, &output, &tools(), &NoEngines, &cancel).unwrap_err();
        t.join().unwrap();
        assert!(matches!(err, PipelineError::Cancelled));
        assert!(started.elapsed() < Duration::from_secs(5));

        // Already cancelled: nothing runs.
        let err = run(&c, &input, &output, &tools(), &NoEngines, &cancel).unwrap_err();
        assert!(matches!(err, PipelineError::Cancelled));
    }

    #[test]
    fn timeout_kills_helper_processes_too() {
        let (_d, input, output) = setup();
        // The tool starts a background helper and records its pid next to the output.
        let mut c = conversion(
            &[],
            "[[steps]]\ncmd = [\"/bin/sh\", \"-c\", \"sleep 30 & echo $! > \\\"$0.pid\\\"; wait\", \"{output}\"]",
        );
        if let Step::Command { timeout, .. } = &mut c.steps[0] {
            *timeout = Duration::from_millis(300);
        }
        let err = run(
            &c,
            &input,
            &output,
            &tools(),
            &NoEngines,
            &CancelToken::new(),
        )
        .unwrap_err();
        assert!(matches!(err, PipelineError::TimedOut { .. }), "{err:?}");
        let pid = fs::read_to_string(output.with_extension("out.pid")).unwrap();
        let pid = pid.trim();
        thread::sleep(Duration::from_millis(100));
        // Dead means gone or a zombie (a non-reaping init may leave zombies around).
        let state = Command::new("ps")
            .args(["-o", "stat=", "-p", pid])
            .output()
            .unwrap();
        let state = String::from_utf8_lossy(&state.stdout).trim().to_owned();
        let alive = !state.is_empty() && !state.starts_with('Z');
        assert!(!alive, "helper process {pid} survived the timeout");
    }

    #[test]
    fn engine_steps_get_substituted_args() {
        struct Recorder(Mutex<Vec<(String, toml::Table)>>);
        impl EngineRunner for Recorder {
            fn run(
                &self,
                name: &str,
                args: &toml::Table,
                ctx: &StepContext,
                _: &CancelToken,
            ) -> Result<(), String> {
                self.0.lock().unwrap().push((name.into(), args.clone()));
                fs::write(&ctx.output, "engine").map_err(|e| e.to_string())
            }
        }
        let (_d, input, output) = setup();
        let c = conversion(
            &[],
            "[[steps]]\nengine = \"builtin:copy\"\nargs = { name = \"{stem}!\", list = [\"{{x}}\"], n = 3 }",
        );
        let rec = Recorder(Mutex::new(Vec::new()));
        run(&c, &input, &output, &tools(), &rec, &CancelToken::new()).unwrap();
        let calls = rec.0.lock().unwrap();
        assert_eq!(calls[0].0, "builtin:copy");
        assert_eq!(calls[0].1["name"].as_str(), Some("my file!"));
        assert_eq!(calls[0].1["list"][0].as_str(), Some("{x}"));
        assert_eq!(calls[0].1["n"].as_integer(), Some(3));
        assert_eq!(fs::read_to_string(&output).unwrap(), "engine");

        let err = run(
            &c,
            &input,
            &output,
            &tools(),
            &NoEngines,
            &CancelToken::new(),
        )
        .unwrap_err();
        assert!(matches!(err, PipelineError::Engine { .. }));
    }

    #[test]
    fn engine_input_output_can_be_redirected() {
        struct Copy;
        impl EngineRunner for Copy {
            fn run(
                &self,
                _: &str,
                _: &toml::Table,
                ctx: &StepContext,
                _: &CancelToken,
            ) -> Result<(), String> {
                fs::copy(&ctx.input, &ctx.output)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
        }
        let (_d, input, output) = setup();
        let c = conversion(
            &[],
            r#"
            [[steps]]
            engine = "builtin:copy"
            args = { output = "{tmpdir}/mid.txt" }
            [[steps]]
            engine = "builtin:copy"
            args = { input = "{tmpdir}/mid.txt" }
            "#,
        );
        run(&c, &input, &output, &tools(), &Copy, &CancelToken::new()).unwrap();
        assert_eq!(fs::read_to_string(&output).unwrap(), "hello");
    }

    #[test]
    fn temp_dir_is_removed() {
        let t = JobTempDir::new().unwrap();
        let p = t.path().to_path_buf();
        fs::write(p.join("x"), "y").unwrap();
        drop(t);
        assert!(!p.exists());
    }
}
