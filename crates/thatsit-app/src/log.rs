//! Logging to stderr and to `~/Library/Logs/io.github.federicosimoni.thatsit/thatsit.log`.
//!
//! Safe to call from any thread.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Log files larger than this are rotated to `thatsit.log.1` at startup.
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

fn start() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

/// The log folder.
pub fn dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| {
        PathBuf::from(h)
            .join("Library/Logs")
            .join(crate::catalog::BUNDLE_ID)
    })
}

fn file() -> &'static Mutex<Option<File>> {
    static FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();
    FILE.get_or_init(|| {
        let file = dir().and_then(|d| {
            fs::create_dir_all(&d).ok()?;
            let path = d.join("thatsit.log");
            if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
                let _ = fs::rename(&path, d.join("thatsit.log.1"));
            }
            OpenOptions::new().create(true).append(true).open(path).ok()
        });
        Mutex::new(file)
    })
}

/// Logs a line prefixed with the seconds elapsed since the first log call.
pub fn info(message: impl AsRef<str>) {
    let t = start().elapsed().as_secs_f64();
    let line = format!("[{t:9.3}] {}", message.as_ref());
    eprintln!("{line}");
    if let Ok(mut guard) = file().lock()
        && let Some(f) = guard.as_mut()
    {
        let unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "{unix} {line}");
    }
}
