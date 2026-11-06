//! `thatsit --convert <conversion-id> <file>...` — headless batch, for CI and debugging.

use std::path::PathBuf;
use std::sync::Arc;

use crate::catalog::Catalog;
use crate::jobs::{Batch, Event, FileResult, Jobs};
use crate::log;

pub fn convert(catalog: &Catalog, args: &[String]) {
    let Some((id, files)) = args.split_first() else {
        eprintln!("usage: thatsit --convert <conversion-id> <file>...");
        std::process::exit(2);
    };
    let Some(conversion) = catalog.registry.get(id).cloned() else {
        eprintln!("unknown conversion {id:?}");
        std::process::exit(2);
    };
    if !catalog.availability().can_run(&conversion) {
        eprintln!("{id} cannot run here (missing tools or engines)");
        std::process::exit(2);
    }
    if files.is_empty() {
        eprintln!("no input files");
        std::process::exit(2);
    }
    let jobs = Jobs::start(Arc::clone(&catalog.tools), on_event);
    jobs.submit(Batch {
        label: catalog.label(&conversion).to_owned(),
        conversion,
        files: files.iter().map(PathBuf::from).collect(),
    });
    // The worker keeps running; `on_event` exits the process when the batch is done.
    std::mem::forget(jobs);
}

fn on_event(event: Event) {
    if let Event::Finished { results, .. } = event {
        let mut ok = true;
        for r in &results {
            match r {
                FileResult::Done { output, .. } => println!("OK {}", output.display()),
                FileResult::Failed { input, error } => {
                    ok = false;
                    println!("FAILED {}: {error}", input.display());
                }
                FileResult::Cancelled => {
                    ok = false;
                    println!("CANCELLED");
                }
            }
        }
        log::info("batch finished");
        std::process::exit(if ok { 0 } else { 1 });
    }
}
