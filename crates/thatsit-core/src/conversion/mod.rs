//! Conversion definitions: TOML schema, parsing and validation.
//!
//! One TOML file describes one conversion. The full reference for authors is
//! `docs/conversions.md`; keep it in sync with this module.

mod registry;

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::placeholder::{self, Placeholder};

pub use registry::{Availability, FileInfo, LoadIssue, Registry};

/// The only schema version understood by this build.
pub const SCHEMA_VERSION: i64 = 1;
/// Step timeout used when a step does not set `timeout_secs`.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
/// Largest accepted `timeout_secs` (2 hours, for long video encodes).
pub const MAX_TIMEOUT_SECS: u64 = 7200;
/// Prefix of builtin engine names.
pub const ENGINE_PREFIX: &str = "builtin:";
/// Language used when a label is missing in the requested languages.
pub const FALLBACK_LANGUAGE: &str = "en";

/// Ring category of a conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    Images,
    AudioVideo,
    Documents,
    Utilities,
}

impl Category {
    pub const ALL: [Category; 4] = [
        Category::Images,
        Category::AudioVideo,
        Category::Documents,
        Category::Utilities,
    ];

    /// The identifier used in TOML files.
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Images => "images",
            Category::AudioVideo => "audio-video",
            Category::Documents => "documents",
            Category::Utilities => "utilities",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == s)
    }
}

/// Where a conversion definition was loaded from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Shipped inside the app bundle (reviewed in PRs).
    Bundled,
    /// From the user's Application Support folder.
    User,
}

/// What a conversion produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// A single file with this extension (lowercase, no dot).
    File { ext: String },
    /// A directory (e.g. one PNG per PDF page), named after the input stem.
    Directory,
}

/// One step of a conversion pipeline.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// Run a program with an argv array (never through a shell).
    Command {
        /// `argv[0]` is a bare tool name listed in `requires`, or an absolute path.
        argv: Vec<String>,
        timeout: Duration,
    },
    /// Run a builtin engine implemented by the app.
    Engine {
        /// Full engine name, including the `builtin:` prefix.
        name: String,
        args: toml::Table,
        timeout: Duration,
    },
}

impl Step {
    pub fn timeout(&self) -> Duration {
        match self {
            Step::Command { timeout, .. } | Step::Engine { timeout, .. } => *timeout,
        }
    }
}

/// A validated conversion definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Conversion {
    pub id: String,
    pub category: Category,
    /// SF Symbol name.
    pub icon: Option<String>,
    /// Accepted UTType identifiers.
    pub accepts: Vec<String>,
    /// Accepted file extensions (lowercase, no dot), as a fallback to `accepts`.
    pub accepts_ext: Vec<String>,
    pub output: Output,
    /// Tools (bare binary names) that must be installed, else the conversion is hidden.
    pub requires: Vec<String>,
    /// Localized labels keyed by language tag; always contains `"en"`.
    pub label: BTreeMap<String, String>,
    pub steps: Vec<Step>,
    pub origin: Origin,
    /// The file this conversion was loaded from, if any.
    pub path: Option<PathBuf>,
}

impl Conversion {
    /// Parses and validates a conversion from TOML text.
    pub fn from_toml(
        text: &str,
        origin: Origin,
        path: Option<&Path>,
    ) -> Result<Self, ValidationError> {
        let raw: RawConversion =
            toml::from_str(text).map_err(|e| ValidationError::single(e.to_string()))?;
        raw.validate(origin, path)
    }

    /// The label for the first matching language in `languages` (most preferred first),
    /// trying each tag exactly and then its base language (`it-CH` → `it`), finally `en`.
    pub fn label_for<S: AsRef<str>>(&self, languages: &[S]) -> &str {
        for lang in languages {
            let lang = lang.as_ref();
            if let Some(l) = self.find_label(lang) {
                return l;
            }
            if let Some(base) = lang.split(['-', '_']).next()
                && let Some(l) = self.find_label(base)
            {
                return l;
            }
        }
        &self.label[FALLBACK_LANGUAGE]
    }

    fn find_label(&self, lang: &str) -> Option<&str> {
        let lang = lang.replace('_', "-");
        self.label
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(&lang))
            .map(|(_, v)| v.as_str())
    }

    /// Builtin engine names used by the steps.
    pub fn engines(&self) -> impl Iterator<Item = &str> {
        self.steps.iter().filter_map(|s| match s {
            Step::Engine { name, .. } => Some(name.as_str()),
            Step::Command { .. } => None,
        })
    }
}

/// All problems found while validating one conversion file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub problems: Vec<String>,
}

impl ValidationError {
    fn single(message: impl Into<String>) -> Self {
        Self {
            problems: vec![message.into()],
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.problems.join("; "))
    }
}

impl std::error::Error for ValidationError {}

// ---------------------------------------------------------------------------------------
// Raw (unvalidated) TOML shape

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConversion {
    schema: Option<i64>,
    id: String,
    category: String,
    icon: Option<String>,
    #[serde(default)]
    accepts: Vec<String>,
    #[serde(default)]
    accepts_ext: Vec<String>,
    output_ext: Option<String>,
    output_kind: Option<String>,
    #[serde(default)]
    requires: Vec<String>,
    #[serde(default)]
    label: BTreeMap<String, String>,
    #[serde(default)]
    steps: Vec<RawStep>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStep {
    cmd: Option<Vec<String>>,
    engine: Option<String>,
    args: Option<toml::Table>,
    timeout_secs: Option<u64>,
}

fn is_valid_id(id: &str) -> bool {
    // Lowercase ASCII words separated by single '.', '-' or '_'.
    !id.is_empty()
        && id.split(['.', '-', '_']).all(|w| {
            !w.is_empty()
                && w.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

fn is_valid_ext(ext: &str) -> bool {
    !ext.is_empty()
        && ext
            .split('.')
            .all(|w| !w.is_empty() && w.bytes().all(|b| b.is_ascii_alphanumeric()))
}

fn is_valid_uti(uti: &str) -> bool {
    !uti.is_empty()
        && !uti.starts_with('.')
        && !uti.ends_with('.')
        && uti
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
}

fn is_valid_tool_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'+'))
}

fn is_valid_lang_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag
            .split('-')
            .all(|w| !w.is_empty() && w.len() <= 8 && w.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// Recursively checks placeholder syntax in every string inside an engine `args` value.
fn check_value_templates(value: &toml::Value, at: &str, problems: &mut Vec<String>) {
    match value {
        toml::Value::String(s) => {
            if let Err(e) = placeholder::placeholders_in(s) {
                problems.push(format!("{at}: {e}"));
            }
        }
        toml::Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                check_value_templates(v, &format!("{at}[{i}]"), problems);
            }
        }
        toml::Value::Table(t) => {
            for (k, v) in t {
                check_value_templates(v, &format!("{at}.{k}"), problems);
            }
        }
        _ => {}
    }
}

impl RawConversion {
    fn validate(self, origin: Origin, path: Option<&Path>) -> Result<Conversion, ValidationError> {
        let mut p = Vec::new();

        match self.schema {
            Some(SCHEMA_VERSION) => {}
            Some(v) => p.push(format!(
                "unsupported schema = {v} (this version understands schema = {SCHEMA_VERSION})"
            )),
            None => p.push(format!("missing `schema = {SCHEMA_VERSION}`")),
        }

        if !is_valid_id(&self.id) {
            p.push(format!(
                "invalid id {:?}: use lowercase letters and digits separated by '.', '-' or '_' \
                 (e.g. \"image.heic-to-jpg\")",
                self.id
            ));
        }

        let category = Category::parse(&self.category);
        if category.is_none() {
            let all: Vec<_> = Category::ALL.iter().map(|c| c.as_str()).collect();
            p.push(format!(
                "invalid category {:?} (expected one of: {})",
                self.category,
                all.join(", ")
            ));
        }

        if let Some(icon) = &self.icon
            && icon.trim().is_empty()
        {
            p.push("icon must not be empty (omit it instead)".to_owned());
        }

        // Accepted types.
        if self.accepts.is_empty() && self.accepts_ext.is_empty() {
            p.push("at least one of `accepts` or `accepts_ext` must be non-empty".to_owned());
        }
        for uti in &self.accepts {
            if !is_valid_uti(uti) {
                p.push(format!("invalid UTType identifier in accepts: {uti:?}"));
            }
        }
        let accepts_ext: Vec<String> = self
            .accepts_ext
            .iter()
            .map(|e| e.trim_start_matches('.').to_ascii_lowercase())
            .collect();
        for (orig, ext) in self.accepts_ext.iter().zip(&accepts_ext) {
            if !is_valid_ext(ext) {
                p.push(format!("invalid extension in accepts_ext: {orig:?}"));
            }
        }

        // Output.
        let output = match self.output_kind.as_deref().unwrap_or("file") {
            "file" => match &self.output_ext {
                Some(ext) => {
                    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
                    if !is_valid_ext(&ext) {
                        p.push(format!("invalid output_ext {ext:?}"));
                    }
                    Some(Output::File { ext })
                }
                None => {
                    p.push(
                        "missing output_ext (required unless output_kind = \"directory\")".into(),
                    );
                    None
                }
            },
            "directory" => {
                if self.output_ext.is_some() {
                    p.push("output_ext must not be set when output_kind = \"directory\"".into());
                }
                Some(Output::Directory)
            }
            other => {
                p.push(format!(
                    "invalid output_kind {other:?} (expected \"file\" or \"directory\")"
                ));
                None
            }
        };

        // Required tools.
        for tool in &self.requires {
            if !is_valid_tool_name(tool) {
                p.push(format!(
                    "invalid tool name in requires: {tool:?} (use a bare binary name such as \"ffmpeg\")"
                ));
            }
        }

        // Labels.
        if !self.label.contains_key(FALLBACK_LANGUAGE) {
            p.push(format!("missing [label] {FALLBACK_LANGUAGE} = \"…\""));
        }
        for (lang, text) in &self.label {
            if !is_valid_lang_tag(lang) {
                p.push(format!("invalid language tag in [label]: {lang:?}"));
            }
            if text.trim().is_empty() {
                p.push(format!("empty label for language {lang:?}"));
            }
        }

        // Steps.
        if self.steps.is_empty() {
            p.push("at least one [[steps]] entry is required".to_owned());
        }
        let mut steps = Vec::with_capacity(self.steps.len());
        let mut writes_output = false;
        for (i, raw) in self.steps.into_iter().enumerate() {
            let at = format!("steps[{i}]");
            let timeout = match raw.timeout_secs {
                None => DEFAULT_TIMEOUT,
                Some(0) => {
                    p.push(format!("{at}: timeout_secs must be at least 1"));
                    DEFAULT_TIMEOUT
                }
                Some(s) if s > MAX_TIMEOUT_SECS => {
                    p.push(format!(
                        "{at}: timeout_secs must be at most {MAX_TIMEOUT_SECS}"
                    ));
                    DEFAULT_TIMEOUT
                }
                Some(s) => Duration::from_secs(s),
            };
            match (raw.cmd, raw.engine) {
                (Some(_), Some(_)) => {
                    p.push(format!("{at}: set either `cmd` or `engine`, not both"));
                }
                (None, None) => p.push(format!("{at}: set `cmd` or `engine`")),
                (Some(argv), None) => {
                    if raw.args.is_some() {
                        p.push(format!("{at}: `args` is only valid with `engine`"));
                    }
                    match argv.first() {
                        None => p.push(format!("{at}: cmd must not be empty")),
                        Some(program) => {
                            if program.contains(['{', '}']) {
                                p.push(format!(
                                    "{at}: the program (cmd[0]) must not contain placeholders"
                                ));
                            } else if program.contains('/') {
                                if !program.starts_with('/') {
                                    p.push(format!(
                                        "{at}: cmd[0] {program:?} must be a bare tool name or an absolute path"
                                    ));
                                }
                            } else if !self.requires.iter().any(|t| t == program) {
                                p.push(format!(
                                    "{at}: tool {program:?} is used in cmd but not listed in `requires`"
                                ));
                            }
                        }
                    }
                    for (j, arg) in argv.iter().enumerate().skip(1) {
                        match placeholder::placeholders_in(arg) {
                            Ok(vars) => {
                                if vars.contains(&Placeholder::Output) {
                                    writes_output = true;
                                }
                            }
                            Err(e) => p.push(format!("{at}.cmd[{j}]: {e}")),
                        }
                    }
                    steps.push(Step::Command { argv, timeout });
                }
                (None, Some(name)) => {
                    let ok_name = name.strip_prefix(ENGINE_PREFIX).is_some_and(is_valid_id);
                    if !ok_name {
                        p.push(format!(
                            "{at}: invalid engine {name:?} (expected \"{ENGINE_PREFIX}<name>\")"
                        ));
                    }
                    let args = raw.args.unwrap_or_default();
                    for (k, v) in &args {
                        check_value_templates(v, &format!("{at}.args.{k}"), &mut p);
                    }
                    // Engines always receive the output path.
                    writes_output = true;
                    steps.push(Step::Engine {
                        name,
                        args,
                        timeout,
                    });
                }
            }
        }
        if !steps.is_empty() && !writes_output {
            p.push("no step writes the result: use {output} in a cmd argument".to_owned());
        }

        if !p.is_empty() {
            return Err(ValidationError { problems: p });
        }
        Ok(Conversion {
            id: self.id,
            // Both are Some when there are no problems.
            category: category.expect("validated"),
            icon: self.icon,
            accepts: self.accepts,
            accepts_ext,
            output: output.expect("validated"),
            requires: self.requires,
            label: self.label,
            steps,
            origin,
            path: path.map(Path::to_path_buf),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEIC: &str = r#"
        schema   = 1
        id       = "image.heic-to-jpg"
        category = "images"
        icon     = "photo"
        accepts  = ["public.heic"]
        accepts_ext = [".HEIC"]
        output_ext = "jpg"
        requires = ["sips"]

        [label]
        en = "Convert to JPG"
        it = "Converti in JPG"
        pt-BR = "Converter para JPG"

        [[steps]]
        cmd = ["sips", "-s", "format", "jpeg", "{input}", "--out", "{output}"]
    "#;

    fn parse(text: &str) -> Result<Conversion, ValidationError> {
        Conversion::from_toml(text, Origin::Bundled, None)
    }

    fn problems(text: &str) -> String {
        parse(text).unwrap_err().to_string()
    }

    #[test]
    fn parses_valid_command_conversion() {
        let c = parse(HEIC).unwrap();
        assert_eq!(c.id, "image.heic-to-jpg");
        assert_eq!(c.category, Category::Images);
        assert_eq!(c.accepts_ext, vec!["heic"]);
        assert_eq!(c.output, Output::File { ext: "jpg".into() });
        assert_eq!(c.steps.len(), 1);
        assert_eq!(c.steps[0].timeout(), DEFAULT_TIMEOUT);
        assert_eq!(c.engines().count(), 0);
    }

    #[test]
    fn parses_engine_and_directory_conversion() {
        let c = parse(
            r#"
            schema = 1
            id = "document.pdf-to-png"
            category = "documents"
            accepts = ["com.adobe.pdf"]
            output_kind = "directory"
            label.en = "Pages to PNG"
            [[steps]]
            engine = "builtin:pdf-to-images"
            args = { format = "png", dpi = 144, name = "{stem}-page" }
            timeout_secs = 600
        "#,
        )
        .unwrap();
        assert_eq!(c.output, Output::Directory);
        assert_eq!(
            c.engines().collect::<Vec<_>>(),
            vec!["builtin:pdf-to-images"]
        );
        assert_eq!(c.steps[0].timeout(), Duration::from_secs(600));
    }

    #[test]
    fn label_negotiation() {
        let c = parse(HEIC).unwrap();
        assert_eq!(c.label_for(&["it-CH"]), "Converti in JPG");
        assert_eq!(c.label_for(&["it"]), "Converti in JPG");
        assert_eq!(c.label_for(&["pt_BR"]), "Converter para JPG");
        assert_eq!(c.label_for(&["pt-br"]), "Converter para JPG");
        assert_eq!(c.label_for(&["de", "it"]), "Converti in JPG");
        assert_eq!(c.label_for(&["de"]), "Convert to JPG");
        assert_eq!(c.label_for::<&str>(&[]), "Convert to JPG");
    }

    #[test]
    fn rejects_syntax_errors_and_unknown_fields() {
        assert!(problems("id = ").contains("TOML") || !problems("id = ").is_empty());
        let text = HEIC.replace("icon     = \"photo\"", "icon = \"photo\"\ncolour = \"red\"");
        assert!(problems(&text).contains("colour"));
    }

    #[test]
    fn rejects_bad_schema_versions() {
        assert!(problems(&HEIC.replace("schema   = 1", "")).contains("missing `schema"));
        assert!(problems(&HEIC.replace("schema   = 1", "schema = 2")).contains("unsupported"));
    }

    #[test]
    fn collects_many_problems_at_once() {
        let text = r#"
            schema = 1
            id = "Bad ID"
            category = "videos"
            output_ext = "j p g"
            requires = ["/usr/bin/sips"]
            label.it = "Solo italiano"
            [[steps]]
            cmd = ["sips", "{inptu}", "{output}"]
        "#;
        let e = parse(text).unwrap_err();
        let all = e.to_string();
        for needle in [
            "invalid id",
            "invalid category",
            "accepts",
            "invalid output_ext",
            "invalid tool name",
            "missing [label] en",
            "not listed in `requires`",
            "unknown placeholder {inptu}",
        ] {
            assert!(all.contains(needle), "missing {needle:?} in: {all}");
        }
    }

    #[test]
    fn step_rules() {
        let base = |steps: &str| {
            format!(
                "schema = 1\nid = \"x\"\ncategory = \"utilities\"\naccepts = [\"public.item\"]\n\
                 output_ext = \"zip\"\nrequires = [\"ditto\"]\nlabel.en = \"X\"\n{steps}"
            )
        };
        assert!(problems(&base("")).contains("at least one [[steps]]"));
        assert!(problems(&base("[[steps]]\ncmd = []")).contains("must not be empty"));
        assert!(problems(&base("[[steps]]\ncmd = [\"ditto\", \"{input}\"]")).contains("{output}"));
        assert!(
            problems(&base("[[steps]]\ncmd = [\"{input}\", \"{output}\"]"))
                .contains("must not contain placeholders")
        );
        assert!(
            problems(&base("[[steps]]\ncmd = [\"bin/x\", \"{output}\"]")).contains("absolute path")
        );
        assert!(
            problems(&base(
                "[[steps]]\ncmd = [\"ditto\", \"{output}\"]\nengine = \"builtin:x\""
            ))
            .contains("not both")
        );
        assert!(problems(&base("[[steps]]\nengine = \"image\"")).contains("invalid engine"));
        assert!(
            problems(&base(
                "[[steps]]\nengine = \"builtin:x\"\nargs = { a = [\"{nope}\"] }"
            ))
            .contains("steps[0].args.a[0]")
        );
        assert!(
            problems(&base(
                "[[steps]]\ncmd = [\"ditto\", \"{output}\"]\ntimeout_secs = 0"
            ))
            .contains("at least 1")
        );
        // Absolute program paths need no `requires` entry.
        parse(&base("[[steps]]\ncmd = [\"/usr/bin/ditto\", \"{output}\"]")).unwrap();
    }

    #[test]
    fn output_kind_rules() {
        let text = HEIC.replace(
            "output_ext = \"jpg\"",
            "output_kind = \"directory\"\noutput_ext = \"jpg\"",
        );
        assert!(problems(&text).contains("must not be set"));
        let text = HEIC.replace("output_ext = \"jpg\"", "");
        assert!(problems(&text).contains("missing output_ext"));
        let text = HEIC.replace(
            "output_ext = \"jpg\"",
            "output_kind = \"stream\"\noutput_ext = \"jpg\"",
        );
        assert!(problems(&text).contains("invalid output_kind"));
    }

    #[test]
    fn id_and_ext_rules() {
        assert!(is_valid_id("image.heic-to-jpg"));
        assert!(is_valid_id("a_b.c-1"));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("a..b"));
        assert!(!is_valid_id(".a"));
        assert!(!is_valid_id("A"));
        assert!(is_valid_ext("tar.gz"));
        assert!(!is_valid_ext("tar..gz"));
        assert!(!is_valid_ext("../x"));
        assert!(is_valid_uti("com.adobe.pdf"));
        assert!(is_valid_uti("dyn.ah62d4rv4ge80"));
        assert!(!is_valid_uti("public heic"));
    }
}
