//! Minimal stderr logging for development builds.
//!
//! Proper file logging to `~/Library/Logs/io.github.federicosimoni.thatsit/` arrives with
//! the job system (Phase 3).

use std::sync::OnceLock;
use std::time::Instant;

fn start() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

/// Logs a line prefixed with the seconds elapsed since the first log call.
pub fn info(message: impl AsRef<str>) {
    let t = start().elapsed().as_secs_f64();
    eprintln!("[{t:9.3}] {}", message.as_ref());
}
