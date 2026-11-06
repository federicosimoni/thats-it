//! Runs conversions off the main thread, one batch at a time, and reports back to the UI
//! on the main queue.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use dispatch2::DispatchQueue;
use thatsit_core::conversion::Conversion;
use thatsit_core::engines::{CORE_ENGINES, Chain, CoreEngines};
use thatsit_core::output::PendingOutput;
use thatsit_core::pipeline::{self, CancelToken};
use thatsit_core::tools::ToolFinder;

use crate::app_engines::{APP_ENGINES, AppEngines};
use crate::log;

/// A conversion to run on some files.
pub struct Batch {
    pub conversion: Conversion,
    pub label: String,
    pub files: Vec<PathBuf>,
}

/// The outcome of one file.
#[derive(Debug, Clone)]
pub enum FileResult {
    Done { output: PathBuf, fell_back: bool },
    Failed { input: PathBuf, error: String },
    Cancelled,
}

/// Progress and results, delivered on the main thread.
#[derive(Debug, Clone)]
pub enum Event {
    Progress {
        done: usize,
        total: usize,
    },
    Finished {
        label: String,
        results: Vec<FileResult>,
    },
}

/// Handle to the background worker.
pub struct Jobs {
    tx: Sender<Batch>,
    current: Arc<Mutex<CancelToken>>,
}

impl Jobs {
    /// Starts the worker; `on_event` runs on the main thread.
    pub fn start(tools: Arc<ToolFinder>, on_event: fn(Event)) -> Self {
        let (tx, rx) = mpsc::channel::<Batch>();
        let current = Arc::new(Mutex::new(CancelToken::new()));
        let cancel_slot = Arc::clone(&current);
        thread::Builder::new()
            .name("thatsit-jobs".into())
            .spawn(move || {
                for batch in rx {
                    let cancel = CancelToken::new();
                    if let Ok(mut slot) = cancel_slot.lock() {
                        *slot = cancel.clone();
                    }
                    let results = run_batch(&batch, &tools, &cancel, on_event);
                    let label = batch.label.clone();
                    DispatchQueue::main()
                        .exec_async(move || on_event(Event::Finished { label, results }));
                }
            })
            .expect("spawn the job thread");
        Self { tx, current }
    }

    pub fn submit(&self, batch: Batch) {
        if self.tx.send(batch).is_err() {
            log::info("job thread is gone; conversion not started");
        }
    }

    /// Cancels the batch that is running now (menu bar → Cancel Conversion).
    pub fn cancel_current(&self) {
        if let Ok(slot) = self.current.lock() {
            slot.cancel();
        }
    }
}

fn run_batch(
    batch: &Batch,
    tools: &ToolFinder,
    cancel: &CancelToken,
    on_event: fn(Event),
) -> Vec<FileResult> {
    let total = batch.files.len();
    let mut results = Vec::with_capacity(total);
    let app_engines = AppEngines;
    let engines = Chain::new()
        .with(APP_ENGINES, &app_engines)
        .with(CORE_ENGINES, &CoreEngines);
    let fallback = crate::prefs::fallback_dir();

    for (i, input) in batch.files.iter().enumerate() {
        DispatchQueue::main().exec_async(move || on_event(Event::Progress { done: i, total }));
        if cancel.is_cancelled() {
            results.push(FileResult::Cancelled);
            continue;
        }
        log::info(format!(
            "run {} on {}",
            batch.conversion.id,
            input.display()
        ));
        let result = run_one(&batch.conversion, input, tools, &engines, cancel, &fallback);
        match &result {
            FileResult::Done { output, fell_back } => log::info(format!(
                "  → {}{}",
                output.display(),
                if *fell_back {
                    " (source folder not writable)"
                } else {
                    ""
                }
            )),
            FileResult::Failed { error, .. } => log::info(format!("  ✗ {error}")),
            FileResult::Cancelled => log::info("  cancelled"),
        }
        results.push(result);
    }
    DispatchQueue::main().exec_async(move || on_event(Event::Progress { done: total, total }));
    results
}

fn run_one(
    conversion: &Conversion,
    input: &Path,
    tools: &ToolFinder,
    engines: &Chain<'_>,
    cancel: &CancelToken,
    fallback: &Path,
) -> FileResult {
    let failed = |error: String| FileResult::Failed {
        input: input.to_path_buf(),
        error,
    };
    let Some(kind) = conversion.output_for(input) else {
        return failed("the file has no extension".into());
    };
    let pending = match PendingOutput::prepare(input, &kind, &conversion.output_suffix, fallback) {
        Ok(p) => p,
        Err(e) => return failed(format!("cannot create the output: {e}")),
    };
    let fell_back = pending.fell_back;
    match pipeline::run(
        conversion,
        input,
        &pending.temp_path,
        tools,
        engines,
        cancel,
    ) {
        Ok(()) => match pending.commit() {
            Ok(output) => FileResult::Done { output, fell_back },
            Err(e) => failed(e.to_string()),
        },
        Err(pipeline::PipelineError::Cancelled) => FileResult::Cancelled,
        Err(e) => failed(e.to_string()),
    }
}
