//! External binary discovery.
//!
//! GUI apps launched from the Finder do not inherit the shell `PATH`, so tools installed
//! with Homebrew (e.g. `ffmpeg`) would not be found. We search `PATH` plus the usual
//! system and Homebrew locations, and cache the results until [`ToolFinder::rescan`].

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Directories always searched after `PATH`, in this order.
pub const EXTRA_DIRS: [&str; 5] = [
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/usr/bin",
    "/usr/sbin",
    "/bin",
];

/// Finds executables by bare name in a fixed list of directories.
#[derive(Debug)]
pub struct ToolFinder {
    dirs: Vec<PathBuf>,
    cache: Mutex<HashMap<String, Option<PathBuf>>>,
}

impl ToolFinder {
    /// Search the process `PATH` followed by [`EXTRA_DIRS`].
    pub fn from_env() -> Self {
        Self::new(std::env::var_os("PATH").as_deref())
    }

    /// Search the given `PATH`-style value followed by [`EXTRA_DIRS`].
    pub fn new(path_var: Option<&OsStr>) -> Self {
        let mut dirs: Vec<PathBuf> = path_var
            .map(|p| std::env::split_paths(p).collect())
            .unwrap_or_default();
        dirs.extend(EXTRA_DIRS.iter().map(PathBuf::from));
        Self::with_dirs(dirs)
    }

    /// Search exactly these directories (duplicates and relative entries are dropped).
    pub fn with_dirs(dirs: impl IntoIterator<Item = PathBuf>) -> Self {
        let mut unique: Vec<PathBuf> = Vec::new();
        for d in dirs {
            // Relative PATH entries (e.g. ".") would make lookups depend on the cwd.
            if d.is_absolute() && !unique.contains(&d) {
                unique.push(d);
            }
        }
        Self {
            dirs: unique,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// The directories searched, in order.
    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    /// A `PATH` value for child processes, so tools that call other tools find them too.
    pub fn path_env(&self) -> OsString {
        std::env::join_paths(&self.dirs).unwrap_or_default()
    }

    /// Absolute path of the executable `name`, if found. `name` must be a bare file name.
    pub fn find(&self, name: &str) -> Option<PathBuf> {
        if name.is_empty() || name.contains('/') {
            return None;
        }
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(hit) = cache.get(name) {
            return hit.clone();
        }
        let found = self
            .dirs
            .iter()
            .map(|d| d.join(name))
            .find(|p| is_executable(p));
        cache.insert(name.to_owned(), found.clone());
        found
    }

    /// Resolves a step's program: absolute paths are checked as-is, bare names are searched.
    pub fn resolve(&self, program: &str) -> Option<PathBuf> {
        if program.starts_with('/') {
            let p = PathBuf::from(program);
            is_executable(&p).then_some(p)
        } else {
            self.find(program)
        }
    }

    /// Forgets cached results (call when Preferences opens).
    pub fn rescan(&self) {
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn make(dir: &Path, name: &str, mode: u32) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(mode)).unwrap();
        p
    }

    #[test]
    fn finds_first_executable_in_order() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        make(a.path(), "tool", 0o644); // not executable → skipped
        let expected = make(b.path(), "tool", 0o755);
        make(b.path(), "other", 0o755);
        fs::create_dir(a.path().join("dir-tool")).unwrap();

        let finder = ToolFinder::with_dirs([a.path().into(), b.path().into()]);
        assert_eq!(finder.find("tool"), Some(expected));
        assert_eq!(finder.find("missing"), None);
        assert_eq!(finder.find("dir-tool"), None);
        assert_eq!(finder.find("../tool"), None);
        assert_eq!(finder.find(""), None);
    }

    #[test]
    fn caches_until_rescan() {
        let a = tempfile::tempdir().unwrap();
        let finder = ToolFinder::with_dirs([a.path().into()]);
        assert_eq!(finder.find("late"), None);
        let p = make(a.path(), "late", 0o755);
        assert_eq!(finder.find("late"), None, "cached negative result");
        finder.rescan();
        assert_eq!(finder.find("late"), Some(p));
    }

    #[test]
    fn path_var_plus_extra_dirs_deduplicated() {
        let finder = ToolFinder::new(Some(OsStr::new("/usr/bin:relative:/custom/bin")));
        let dirs: Vec<_> = finder.dirs().iter().map(|d| d.to_str().unwrap()).collect();
        assert_eq!(
            dirs,
            vec![
                "/usr/bin",
                "/custom/bin",
                "/opt/homebrew/bin",
                "/usr/local/bin",
                "/usr/sbin",
                "/bin"
            ]
        );
        assert!(
            finder
                .path_env()
                .to_str()
                .unwrap()
                .starts_with("/usr/bin:/custom/bin:")
        );
    }

    #[test]
    fn resolves_absolute_programs() {
        let a = tempfile::tempdir().unwrap();
        let p = make(a.path(), "abs", 0o755);
        let finder = ToolFinder::with_dirs([]);
        assert_eq!(finder.resolve(p.to_str().unwrap()), Some(p));
        assert_eq!(finder.resolve("/definitely/not/here"), None);
    }
}
