//! Command line, for Shortcuts ("Run Shell Script"), scripts, CI and debugging:
//!
//! ```text
//! thatsit --convert <conversion-id> <file>...   run one batch, print OK/FAILED per file
//! thatsit --list [<file>...]                    conversions available (for these files)
//! thatsit --help | --version
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use thatsit_core::conversion::Category;

use crate::catalog::Catalog;
use crate::files;
use crate::jobs::{Batch, Event, FileResult, Jobs};
use crate::log;

const USAGE: &str = "\
usage:
  thatsit --convert <conversion-id> <file>...   convert files, print OK/FAILED per file
  thatsit --list [<file>...]                    list conversions (only those for <file>s)
  thatsit --help | --version
Without arguments, starts the desktop widget.";

/// Handles the commands other than `--convert`. Returns the exit code, or `None` to start
/// the app normally (no arguments, or arguments macOS adds when launching an app).
pub fn run_other(catalog: &Catalog, args: &[String]) -> Option<i32> {
    match args.first().map(String::as_str) {
        Some("--help" | "-h") => {
            println!("That's it {}\n{USAGE}", env!("CARGO_PKG_VERSION"));
            Some(0)
        }
        Some("--version" | "-V") => {
            println!("That's it {}", env!("CARGO_PKG_VERSION"));
            Some(0)
        }
        Some("--list") => {
            list(catalog, &args[1..]);
            Some(0)
        }
        Some(other) if other.starts_with("--") => {
            eprintln!("unknown option {other}\n{USAGE}");
            Some(2)
        }
        _ => None,
    }
}

/// Prints `id<TAB>label` per conversion, grouped by category. With files, only the
/// conversions offered for all of them (as in the ring).
fn list(catalog: &Catalog, paths: &[String]) {
    let conversions: Vec<_> = if paths.is_empty() {
        let availability = catalog.availability();
        catalog
            .registry
            .conversions()
            .iter()
            .filter(|c| availability.can_run(c))
            .collect()
    } else {
        let dragged: Vec<_> = paths.iter().map(|p| files::file_at(p)).collect();
        catalog.compatible(&dragged)
    };
    for category in Category::ALL {
        let in_category: Vec<_> = conversions
            .iter()
            .filter(|c| c.category == category)
            .collect();
        if in_category.is_empty() {
            continue;
        }
        println!("# {}", catalog.category_name(category));
        for c in in_category {
            println!("{}\t{}", c.id, catalog.label(c));
        }
    }
}

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
