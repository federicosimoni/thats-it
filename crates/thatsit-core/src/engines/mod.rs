//! Builtin engines implemented in pure Rust.
//!
//! Engines that need AppKit or PDFKit (`builtin:doc-to-pdf`, `builtin:pdf-to-images`) live
//! in the app crate. The full list with their `args` is documented in `docs/conversions.md`.

mod image;
mod markdown;
mod metadata;
mod png;
mod svg;

use crate::pipeline::{CancelToken, EngineRunner, StepContext};

/// Names of the engines implemented here.
pub const CORE_ENGINES: [&str; 5] = [
    "builtin:image",
    "builtin:strip-metadata",
    "builtin:optimize-png",
    "builtin:svg-to-png",
    "builtin:markdown-to-html",
];

/// Runs the pure-Rust builtin engines.
#[derive(Debug, Default, Clone, Copy)]
pub struct CoreEngines;

impl EngineRunner for CoreEngines {
    fn run(
        &self,
        name: &str,
        args: &toml::Table,
        ctx: &StepContext,
        _cancel: &CancelToken,
    ) -> Result<(), String> {
        match name {
            "builtin:image" => image::run(args, ctx),
            "builtin:strip-metadata" => metadata::run(ctx),
            "builtin:optimize-png" => png::run(args, ctx),
            "builtin:svg-to-png" => svg::run(args, ctx),
            "builtin:markdown-to-html" => markdown::run(ctx),
            other => Err(format!("unknown engine {other}")),
        }
    }
}

/// Tries each runner in order; the first that knows the engine runs it.
pub struct Chain<'a> {
    runners: Vec<(&'a [&'a str], &'a dyn EngineRunner)>,
}

impl<'a> Chain<'a> {
    pub fn new() -> Self {
        Self {
            runners: Vec::new(),
        }
    }

    /// Adds a runner that implements the engines named in `names`.
    pub fn with(mut self, names: &'a [&'a str], runner: &'a dyn EngineRunner) -> Self {
        self.runners.push((names, runner));
        self
    }
}

impl Default for Chain<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineRunner for Chain<'_> {
    fn run(
        &self,
        name: &str,
        args: &toml::Table,
        ctx: &StepContext,
        cancel: &CancelToken,
    ) -> Result<(), String> {
        match self.runners.iter().find(|(names, _)| names.contains(&name)) {
            Some((_, runner)) => runner.run(name, args, ctx, cancel),
            None => Err(format!("engine {name} is not available in this build")),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Argument helpers shared by the engines

pub(crate) fn arg_str<'t>(args: &'t toml::Table, key: &str) -> Result<Option<&'t str>, String> {
    match args.get(key) {
        None => Ok(None),
        Some(toml::Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(format!("argument `{key}` must be a string")),
    }
}

pub(crate) fn arg_int(
    args: &toml::Table,
    key: &str,
    range: std::ops::RangeInclusive<i64>,
) -> Result<Option<i64>, String> {
    match args.get(key) {
        None => Ok(None),
        Some(toml::Value::Integer(n)) if range.contains(n) => Ok(Some(*n)),
        Some(_) => Err(format!(
            "argument `{key}` must be an integer in {}..={}",
            range.start(),
            range.end()
        )),
    }
}

pub(crate) fn arg_float(
    args: &toml::Table,
    key: &str,
    range: std::ops::RangeInclusive<f64>,
) -> Result<Option<f64>, String> {
    let v = match args.get(key) {
        None => return Ok(None),
        Some(toml::Value::Float(f)) => *f,
        Some(toml::Value::Integer(n)) => *n as f64,
        Some(_) => return Err(format!("argument `{key}` must be a number")),
    };
    if range.contains(&v) {
        Ok(Some(v))
    } else {
        Err(format!(
            "argument `{key}` must be between {} and {}",
            range.start(),
            range.end()
        ))
    }
}

pub(crate) fn io_err(what: &str, e: impl std::fmt::Display) -> String {
    format!("{what}: {e}")
}

#[cfg(test)]
pub(crate) mod test_util {
    use std::path::{Path, PathBuf};

    use crate::pipeline::StepContext;

    pub fn ctx(input: &Path, output: &Path) -> StepContext {
        StepContext {
            input: input.to_path_buf(),
            output: output.to_path_buf(),
            stem: crate::output::stem_of(input),
            dir: input.parent().unwrap().to_path_buf(),
            tmpdir: PathBuf::from("/tmp"),
        }
    }

    pub fn table(text: &str) -> toml::Table {
        text.parse().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn chain_dispatches_by_name() {
        struct Fake;
        impl EngineRunner for Fake {
            fn run(
                &self,
                n: &str,
                _: &toml::Table,
                _: &StepContext,
                _: &CancelToken,
            ) -> Result<(), String> {
                Err(format!("fake ran {n}"))
            }
        }
        let fake = Fake;
        let chain = Chain::new()
            .with(&["builtin:fake"], &fake)
            .with(&CORE_ENGINES, &CoreEngines);
        let ctx = test_util::ctx(Path::new("/x/a.md"), Path::new("/x/out"));
        let cancel = CancelToken::new();
        assert_eq!(
            chain.run("builtin:fake", &toml::Table::new(), &ctx, &cancel),
            Err("fake ran builtin:fake".into())
        );
        assert!(
            chain
                .run("builtin:nope", &toml::Table::new(), &ctx, &cancel)
                .unwrap_err()
                .contains("not available")
        );
    }

    #[test]
    fn argument_helpers() {
        let t = test_util::table("s = \"x\"\nn = 5\nf = 0.5\nbad = true");
        assert_eq!(arg_str(&t, "s"), Ok(Some("x")));
        assert_eq!(arg_str(&t, "missing"), Ok(None));
        assert!(arg_str(&t, "n").is_err());
        assert_eq!(arg_int(&t, "n", 1..=10), Ok(Some(5)));
        assert!(arg_int(&t, "n", 6..=10).is_err());
        assert_eq!(arg_float(&t, "f", 0.0..=1.0), Ok(Some(0.5)));
        assert_eq!(arg_float(&t, "n", 0.0..=10.0), Ok(Some(5.0)));
        assert!(arg_float(&t, "bad", 0.0..=1.0).is_err());
    }
}
