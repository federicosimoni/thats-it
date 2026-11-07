//! Loading conversion files and matching them against dragged files.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::{Conversion, Origin};

/// A conversion file that could not be loaded (shown in the log and in Preferences).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadIssue {
    pub path: PathBuf,
    pub message: String,
}

/// What the matcher needs to know about one dragged file.
///
/// The core cannot ask macOS about type conformance, so the app supplies the file's
/// UTType **and all the types it conforms to** (e.g. `public.jpeg`, `public.image`,
/// `public.data`, `public.item`). A conversion accepting `public.image` therefore matches
/// every image.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileInfo {
    /// The file's UTType identifier followed by its supertypes, in any order.
    pub types: Vec<String>,
    /// Lowercase extension without the dot, if any.
    pub ext: Option<String>,
}

impl FileInfo {
    /// Builds a `FileInfo` from a path (for the extension) and its conformance list.
    pub fn new(path: &Path, types: Vec<String>) -> Self {
        Self {
            types,
            ext: path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase),
        }
    }
}

/// Tools and builtin engines available on this machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Availability {
    /// Bare names of tools found by tool discovery.
    pub tools: BTreeSet<String>,
    /// Full builtin engine names (`builtin:…`) implemented by this build.
    pub engines: BTreeSet<String>,
}

impl Availability {
    /// Tools required by `conversion` that are not installed.
    pub fn missing_tools<'c>(&self, conversion: &'c Conversion) -> Vec<&'c str> {
        conversion
            .requires
            .iter()
            .filter(|t| !self.tools.contains(t.as_str()))
            .map(String::as_str)
            .collect()
    }

    /// Engines used by `conversion` that this build does not implement.
    pub fn missing_engines<'c>(&self, conversion: &'c Conversion) -> Vec<&'c str> {
        conversion
            .engines()
            .filter(|e| !self.engines.contains(*e))
            .collect()
    }

    /// Whether `conversion` can run here.
    pub fn can_run(&self, conversion: &Conversion) -> bool {
        self.missing_tools(conversion).is_empty() && self.missing_engines(conversion).is_empty()
    }
}

/// All loaded conversions, in load order, with later definitions overriding earlier ones
/// by `id`.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    conversions: Vec<Conversion>,
    issues: Vec<LoadIssue>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads every `*.toml` file of `dir` (sorted by file name, not recursive).
    ///
    /// A missing directory is not an error (the user folder usually does not exist).
    /// Invalid files are skipped and recorded in [`Registry::issues`].
    pub fn load_dir(&mut self, dir: &Path, origin: Origin) {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => {
                self.issue(dir, format!("cannot read directory: {e}"));
                return;
            }
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
                    && !p
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with('.'))
            })
            .collect();
        paths.sort();
        for path in paths {
            self.load_file(&path, origin);
        }
    }

    /// Loads one conversion file.
    pub fn load_file(&mut self, path: &Path, origin: Origin) {
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                self.issue(path, format!("cannot read file: {e}"));
                return;
            }
        };
        match Conversion::from_toml(&text, origin, Some(path)) {
            Ok(c) => self.insert(c),
            Err(e) => self.issue(path, e.to_string()),
        }
    }

    /// Adds a conversion, replacing (in place) any earlier one with the same id.
    pub fn insert(&mut self, conversion: Conversion) {
        match self.conversions.iter_mut().find(|c| c.id == conversion.id) {
            Some(existing) => {
                if existing.origin == conversion.origin {
                    let path = conversion.path.clone().unwrap_or_default();
                    let other = existing
                        .path
                        .as_deref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default();
                    self.issues.push(LoadIssue {
                        path,
                        message: format!(
                            "duplicate id {:?} (overrides the definition in {other})",
                            conversion.id
                        ),
                    });
                }
                *existing = conversion;
            }
            None => self.conversions.push(conversion),
        }
    }

    fn issue(&mut self, path: &Path, message: String) {
        self.issues.push(LoadIssue {
            path: path.to_path_buf(),
            message,
        });
    }

    pub fn conversions(&self) -> &[Conversion] {
        &self.conversions
    }

    pub fn issues(&self) -> &[LoadIssue] {
        &self.issues
    }

    pub fn get(&self, id: &str) -> Option<&Conversion> {
        self.conversions.iter().find(|c| c.id == id)
    }

    /// Conversions that accept **every** file in `files` and can run on this machine.
    ///
    /// When two matches produce the same kind of file and one of them names the files'
    /// extension while the other only accepts them through a supertype (Markdown is also
    /// plain text), only the specific one is kept: "Convert to PDF" should render the
    /// Markdown, not print its source.
    pub fn matching(&self, files: &[FileInfo], availability: &Availability) -> Vec<&Conversion> {
        if files.is_empty() {
            return Vec::new();
        }
        let matches: Vec<&Conversion> = self
            .conversions
            .iter()
            .filter(|c| files.iter().all(|f| accepts(c, f)))
            .filter(|c| availability.can_run(c))
            .collect();
        let specific = |c: &Conversion| files.iter().all(|f| accepts_by_ext(c, f));
        matches
            .iter()
            .copied()
            .filter(|c| {
                specific(c)
                    || !matches
                        .iter()
                        .any(|o| o.id != c.id && o.output == c.output && specific(o))
            })
            .collect()
    }
}

/// Whether `file`'s extension is listed in `conversion.accepts_ext`.
fn accepts_by_ext(conversion: &Conversion, file: &FileInfo) -> bool {
    file.ext
        .as_deref()
        .is_some_and(|e| conversion.accepts_ext.iter().any(|a| a == e))
}

/// Whether `conversion` accepts `file` (by UTType conformance, else by extension).
pub fn accepts(conversion: &Conversion, file: &FileInfo) -> bool {
    let by_type = conversion
        .accepts
        .iter()
        .any(|a| file.types.iter().any(|t| t.eq_ignore_ascii_case(a)));
    by_type || accepts_by_ext(conversion, file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conv(id: &str, accepts: &[&str], exts: &[&str], requires: &[&str]) -> String {
        let q = |v: &[&str]| {
            v.iter()
                .map(|s| format!("{s:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let program = requires.first().copied().unwrap_or("/usr/bin/true");
        // A distinct output per conversion, so the "specific wins" rule does not apply.
        let out: String = id.chars().filter(char::is_ascii_alphanumeric).collect();
        format!(
            "schema = 1\nid = {id:?}\ncategory = \"images\"\naccepts = [{}]\naccepts_ext = [{}]\n\
             output_ext = \"{out}\"\nrequires = [{}]\nlabel.en = \"{id}\"\n\
             [[steps]]\ncmd = [{program:?}, \"{{input}}\", \"{{output}}\"]\n",
            q(accepts),
            q(exts),
            q(requires)
        )
    }

    fn jpeg() -> FileInfo {
        FileInfo::new(
            Path::new("/x/photo.JPG"),
            vec![
                "public.jpeg".into(),
                "public.image".into(),
                "public.data".into(),
                "public.item".into(),
            ],
        )
    }

    fn pdf() -> FileInfo {
        FileInfo::new(
            Path::new("/x/doc.pdf"),
            vec!["com.adobe.pdf".into(), "public.data".into()],
        )
    }

    fn all_available() -> Availability {
        Availability {
            tools: ["sips", "ffmpeg"].iter().map(|s| s.to_string()).collect(),
            engines: BTreeSet::new(),
        }
    }

    fn registry(defs: &[String]) -> Registry {
        let mut r = Registry::new();
        for d in defs {
            r.insert(Conversion::from_toml(d, Origin::Bundled, None).unwrap());
        }
        r
    }

    #[test]
    fn file_info_extension_is_lowercased() {
        assert_eq!(jpeg().ext.as_deref(), Some("jpg"));
        assert_eq!(FileInfo::new(Path::new("/x/Makefile"), vec![]).ext, None);
    }

    #[test]
    fn matches_by_conformance_and_extension() {
        let r = registry(&[
            conv("a", &["public.image"], &[], &["sips"]),
            conv("b", &["PUBLIC.JPEG"], &[], &["sips"]),
            conv("c", &[], &["jpg"], &["sips"]),
            conv("d", &["com.adobe.pdf"], &[], &["sips"]),
        ]);
        let ids: Vec<_> = r
            .matching(&[jpeg()], &all_available())
            .iter()
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn specific_conversion_hides_generic_one_with_the_same_output() {
        let same_output = |c: String| {
            c.replace(
                &format!(
                    "output_ext = \"{}\"",
                    c.split('"').nth(1).unwrap().replace('-', "")
                ),
                "output_ext = \"pdf\"",
            )
        };
        let r = registry(&[
            same_output(conv(
                "md-to-pdf",
                &["net.daringfireball.markdown"],
                &["md"],
                &[],
            )),
            same_output(conv("text-to-pdf", &["public.plain-text"], &["txt"], &[])),
            conv("text-to-rtf", &["public.plain-text"], &["txt"], &[]),
        ]);
        let ids = |f: FileInfo| -> Vec<String> {
            r.matching(&[f], &all_available())
                .iter()
                .map(|c| c.id.clone())
                .collect()
        };
        let text = |name: &str, types: &[&str]| {
            FileInfo::new(
                Path::new(name),
                types.iter().map(|t| t.to_string()).collect(),
            )
        };
        // Markdown is plain text too, but only the Markdown PDF conversion is offered.
        assert_eq!(
            ids(text(
                "/x/notes.md",
                &["net.daringfireball.markdown", "public.plain-text"]
            )),
            vec!["md-to-pdf", "text-to-rtf"]
        );
        assert_eq!(
            ids(text("/x/notes.txt", &["public.plain-text"])),
            vec!["text-to-pdf", "text-to-rtf"]
        );
        // Without an extension nothing is "specific", so nothing is hidden.
        assert_eq!(
            ids(text(
                "/x/README",
                &["net.daringfireball.markdown", "public.plain-text"]
            )),
            vec!["md-to-pdf", "text-to-pdf", "text-to-rtf"]
        );
    }

    #[test]
    fn multiple_files_use_intersection() {
        let r = registry(&[
            conv("images", &["public.image"], &[], &["sips"]),
            conv("anything", &["public.data"], &[], &["sips"]),
        ]);
        let ids: Vec<_> = r
            .matching(&[jpeg(), pdf()], &all_available())
            .iter()
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(ids, vec!["anything"]);
        assert!(r.matching(&[], &all_available()).is_empty());
    }

    #[test]
    fn hides_conversions_with_missing_tools_or_engines() {
        let r = registry(&[
            conv("needs-magick", &["public.image"], &[], &["magick"]),
            conv("ok", &["public.image"], &[], &["sips"]),
            "schema = 1\nid = \"engine\"\ncategory = \"images\"\naccepts = [\"public.image\"]\n\
             output_ext = \"png\"\nlabel.en = \"E\"\n[[steps]]\nengine = \"builtin:image\"\n"
                .to_owned(),
        ]);
        let mut avail = all_available();
        let ids = |r: &Registry, a: &Availability| -> Vec<String> {
            r.matching(&[jpeg()], a)
                .iter()
                .map(|c| c.id.clone())
                .collect()
        };
        assert_eq!(ids(&r, &avail), vec!["ok"]);
        assert_eq!(
            avail.missing_tools(r.get("needs-magick").unwrap()),
            vec!["magick"]
        );
        assert_eq!(
            avail.missing_engines(r.get("engine").unwrap()),
            vec!["builtin:image"]
        );
        avail.engines.insert("builtin:image".into());
        assert_eq!(ids(&r, &avail), vec!["ok", "engine"]);
    }

    #[test]
    fn user_files_override_bundled_by_id() {
        let bundled = tempfile::tempdir().unwrap();
        let user = tempfile::tempdir().unwrap();
        fs::write(
            bundled.path().join("a.toml"),
            conv("x", &["public.image"], &[], &["sips"]),
        )
        .unwrap();
        fs::write(
            bundled.path().join("b.toml"),
            conv("y", &["public.image"], &[], &["sips"]),
        )
        .unwrap();
        fs::write(
            user.path().join("mine.toml"),
            conv("x", &["com.adobe.pdf"], &[], &["sips"]),
        )
        .unwrap();
        fs::write(user.path().join("notes.txt"), "ignored").unwrap();
        fs::write(user.path().join(".hidden.toml"), "garbage").unwrap();

        let mut r = Registry::new();
        r.load_dir(bundled.path(), Origin::Bundled);
        r.load_dir(user.path(), Origin::User);
        r.load_dir(&user.path().join("does-not-exist"), Origin::User);

        assert!(r.issues().is_empty(), "{:?}", r.issues());
        let ids: Vec<_> = r.conversions().iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["x", "y"]); // override keeps the original position
        let x = r.get("x").unwrap();
        assert_eq!(x.origin, Origin::User);
        assert_eq!(x.accepts, vec!["com.adobe.pdf"]);
    }

    #[test]
    fn invalid_files_are_reported_and_skipped() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("bad.toml"), "schema = 1\nid = \"x\"").unwrap();
        fs::write(
            dir.path().join("good.toml"),
            conv("g", &["public.image"], &[], &["sips"]),
        )
        .unwrap();
        fs::write(
            dir.path().join("good-dup.toml"),
            conv("g", &["public.image"], &[], &["sips"]),
        )
        .unwrap();

        let mut r = Registry::new();
        r.load_dir(dir.path(), Origin::Bundled);
        assert_eq!(r.conversions().len(), 1);
        assert_eq!(r.issues().len(), 2);
        assert!(r.issues()[0].path.ends_with("bad.toml"));
        assert!(r.issues()[1].message.contains("duplicate id"));
    }
}
