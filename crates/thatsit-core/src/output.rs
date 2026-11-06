//! Output naming, collision handling and atomic, never-overwriting commits.
//!
//! Rules (project brief §5):
//! - the result is saved next to the original: `photo.heic` → `photo.jpg`;
//! - on collision: `photo (1).jpg`, `photo (2).jpg`, …;
//! - the pipeline writes to a hidden temporary path in the same directory, which is then
//!   moved into place atomically; existing files are never overwritten and the source is
//!   never touched;
//! - if the source directory is not writable, the output goes to a fallback directory
//!   (`~/Downloads`, chosen by the app).

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::conversion::Output;

/// Upper bound on `name (n)` attempts before giving up.
const MAX_COLLISION_INDEX: u32 = 9999;

/// The `n`-th candidate name: `stem.ext` for 0, `stem (n).ext` afterwards.
/// `ext = None` produces directory names (`stem`, `stem (1)`…).
pub fn candidate_name(stem: &str, ext: Option<&str>, n: u32) -> String {
    let base = if n == 0 {
        stem.to_owned()
    } else {
        format!("{stem} ({n})")
    };
    match ext {
        Some(ext) => format!("{base}.{ext}"),
        None => base,
    }
}

/// Stem used for naming outputs of `input` (`photo` for `photo.heic`, `Folder` for a folder).
pub fn stem_of(input: &Path) -> String {
    input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "output".to_owned())
}

/// A reserved temporary output location, inside the directory the result will end up in.
#[derive(Debug)]
pub struct PendingOutput {
    /// Path the pipeline must write to (`{output}`).
    pub temp_path: PathBuf,
    /// Directory the final result is moved into.
    pub dir: PathBuf,
    /// `true` when the source directory was not writable and `fallback_dir` was used.
    pub fell_back: bool,
    stem: String,
    kind: Output,
    committed: bool,
}

impl PendingOutput {
    /// Reserves a temporary output for `input` in its directory, or in `fallback_dir` if
    /// that directory is not writable.
    ///
    /// For file outputs the temporary path does **not** exist yet (some tools refuse to
    /// write over an existing file); for directory outputs an empty directory is created.
    pub fn prepare(input: &Path, kind: &Output, fallback_dir: &Path) -> io::Result<Self> {
        let stem = stem_of(input);
        let source_dir = input.parent().unwrap_or(Path::new("/"));
        match Self::prepare_in(source_dir, &stem, kind) {
            Ok(p) => Ok(p),
            Err(e) if is_not_writable(&e) => {
                let mut p = Self::prepare_in(fallback_dir, &stem, kind)?;
                p.fell_back = true;
                Ok(p)
            }
            Err(e) => Err(e),
        }
    }

    fn prepare_in(dir: &Path, stem: &str, kind: &Output) -> io::Result<Self> {
        let temp_path = dir.join(temp_name(stem, kind));
        match kind {
            Output::File { .. } => {
                // Probe that we can create a file here, then free the name for the tool.
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&temp_path)?;
                fs::remove_file(&temp_path)?;
            }
            Output::Directory => fs::create_dir(&temp_path)?,
        }
        Ok(Self {
            temp_path,
            dir: dir.to_path_buf(),
            fell_back: false,
            stem: stem.to_owned(),
            kind: kind.clone(),
            committed: false,
        })
    }

    /// Moves the finished result to its final, non-colliding name and returns that path.
    pub fn commit(mut self) -> io::Result<PathBuf> {
        let ext = match &self.kind {
            Output::File { ext } => Some(ext.as_str()),
            Output::Directory => None,
        };
        let meta = fs::symlink_metadata(&self.temp_path)
            .map_err(|e| io::Error::new(e.kind(), "the conversion did not produce an output"))?;
        if ext.is_some() && !meta.is_file() {
            return Err(io::Error::other(
                "the conversion output is not a regular file",
            ));
        }
        if ext.is_none() && !meta.is_dir() {
            return Err(io::Error::other("the conversion output is not a directory"));
        }

        for n in 0..=MAX_COLLISION_INDEX {
            let target = self.dir.join(candidate_name(&self.stem, ext, n));
            match place_no_clobber(&self.temp_path, &target, ext.is_some()) {
                Ok(()) => {
                    self.committed = true;
                    return Ok(target);
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "too many files with the same name",
        ))
    }
}

impl Drop for PendingOutput {
    fn drop(&mut self) {
        if !self.committed {
            // Best effort: remove whatever a failed or cancelled pipeline left behind.
            let _ = fs::remove_file(&self.temp_path);
            let _ = fs::remove_dir_all(&self.temp_path);
        }
    }
}

/// Moves `from` to `to` without ever replacing an existing `to`.
fn place_no_clobber(from: &Path, to: &Path, is_file: bool) -> io::Result<()> {
    if is_file {
        // A hard link fails atomically with AlreadyExists if `to` exists.
        match fs::hard_link(from, to) {
            Ok(()) => {
                fs::remove_file(from)?;
                return Ok(());
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Err(e),
            // Filesystems without hard links (e.g. some network/FAT volumes): fall through.
            Err(_) => {}
        }
    }
    // Check-then-rename: a small race window remains, acceptable for directories and
    // filesystems without hard-link support.
    if fs::symlink_metadata(to).is_ok() {
        return Err(io::Error::from(io::ErrorKind::AlreadyExists));
    }
    fs::rename(from, to)
}

fn is_not_writable(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem
    ) || e.raw_os_error() == Some(30) // EROFS on macOS/Linux
}

/// A hidden, unique temporary name that keeps the real extension (tools such as ffmpeg
/// pick the format from it): `.photo.thatsit-<unique>.jpg`.
fn temp_name(stem: &str, kind: &Output) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let unique = format!(
        "{}-{}-{nanos}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    match kind {
        Output::File { ext } => format!(".{stem}.thatsit-{unique}.{ext}"),
        Output::Directory => format!(".{stem}.thatsit-{unique}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpg() -> Output {
        Output::File { ext: "jpg".into() }
    }

    #[test]
    fn candidate_names() {
        assert_eq!(candidate_name("photo", Some("jpg"), 0), "photo.jpg");
        assert_eq!(candidate_name("photo", Some("jpg"), 2), "photo (2).jpg");
        assert_eq!(candidate_name("Pages", None, 0), "Pages");
        assert_eq!(candidate_name("Pages", None, 1), "Pages (1)");
    }

    #[test]
    fn stems() {
        assert_eq!(stem_of(Path::new("/a/photo.heic")), "photo");
        assert_eq!(stem_of(Path::new("/a/archive.tar.gz")), "archive.tar");
        assert_eq!(stem_of(Path::new("/a/Folder")), "Folder");
        assert_eq!(stem_of(Path::new("/a/.bashrc")), ".bashrc");
        assert_eq!(stem_of(Path::new("/")), "output");
    }

    #[test]
    fn commits_next_to_source_and_avoids_collisions() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("photo.heic");
        fs::write(&input, "source").unwrap();
        fs::write(dir.path().join("photo.jpg"), "existing").unwrap();

        let pending = PendingOutput::prepare(&input, &jpg(), Path::new("/nonexistent")).unwrap();
        assert!(!pending.fell_back);
        assert!(
            !pending.temp_path.exists(),
            "temp path is free for the tool"
        );
        let temp_name = pending.temp_path.file_name().unwrap().to_str().unwrap();
        assert!(temp_name.starts_with(".photo.thatsit-") && temp_name.ends_with(".jpg"));

        fs::write(&pending.temp_path, "new").unwrap();
        let out = pending.commit().unwrap();
        assert_eq!(out, dir.path().join("photo (1).jpg"));
        assert_eq!(fs::read_to_string(&out).unwrap(), "new");
        assert_eq!(
            fs::read_to_string(dir.path().join("photo.jpg")).unwrap(),
            "existing"
        );
        assert_eq!(fs::read_to_string(&input).unwrap(), "source");

        // Nothing else (no leftovers) in the directory.
        let mut names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, vec!["photo (1).jpg", "photo.heic", "photo.jpg"]);
    }

    #[test]
    fn same_extension_output_never_replaces_the_source() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("image.png");
        fs::write(&input, "source").unwrap();
        let pending = PendingOutput::prepare(
            &input,
            &Output::File { ext: "png".into() },
            Path::new("/nonexistent"),
        )
        .unwrap();
        fs::write(&pending.temp_path, "optimized").unwrap();
        assert_eq!(pending.commit().unwrap(), dir.path().join("image (1).png"));
        assert_eq!(fs::read_to_string(&input).unwrap(), "source");
    }

    #[test]
    fn missing_output_is_an_error_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("a.txt");
        fs::write(&input, "x").unwrap();
        let pending = PendingOutput::prepare(&input, &jpg(), Path::new("/nonexistent")).unwrap();
        assert!(pending.commit().is_err());

        // Dropping an uncommitted output removes the partial result.
        let pending = PendingOutput::prepare(&input, &jpg(), Path::new("/nonexistent")).unwrap();
        let temp = pending.temp_path.clone();
        fs::write(&temp, "partial").unwrap();
        drop(pending);
        assert!(!temp.exists());
    }

    #[test]
    fn directory_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("doc.pdf");
        fs::write(&input, "x").unwrap();
        fs::create_dir(dir.path().join("doc")).unwrap();

        let pending =
            PendingOutput::prepare(&input, &Output::Directory, Path::new("/nonexistent")).unwrap();
        assert!(pending.temp_path.is_dir());
        fs::write(pending.temp_path.join("doc-1.png"), "page").unwrap();
        let out = pending.commit().unwrap();
        assert_eq!(out, dir.path().join("doc (1)"));
        assert!(out.join("doc-1.png").is_file());

        // A file where a directory is expected is rejected.
        let pending =
            PendingOutput::prepare(&input, &Output::Directory, Path::new("/nonexistent")).unwrap();
        fs::remove_dir(&pending.temp_path).unwrap();
        fs::write(&pending.temp_path, "oops").unwrap();
        assert!(pending.commit().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn falls_back_when_source_dir_is_read_only() {
        use std::os::unix::fs::PermissionsExt;
        let src = tempfile::tempdir().unwrap();
        let fallback = tempfile::tempdir().unwrap();
        let input = src.path().join("song.wav");
        fs::write(&input, "x").unwrap();
        fs::set_permissions(src.path(), fs::Permissions::from_mode(0o555)).unwrap();

        // Root ignores directory permissions; skip the assertion in that case.
        let writable_anyway = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(src.path().join("probe"))
            .is_ok();
        if !writable_anyway {
            let pending = PendingOutput::prepare(
                &input,
                &Output::File { ext: "m4a".into() },
                fallback.path(),
            )
            .unwrap();
            assert!(pending.fell_back);
            assert_eq!(pending.dir, fallback.path());
            fs::write(&pending.temp_path, "y").unwrap();
            assert_eq!(pending.commit().unwrap(), fallback.path().join("song.m4a"));
        }
        fs::set_permissions(src.path(), fs::Permissions::from_mode(0o755)).unwrap();
    }
}
