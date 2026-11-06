//! Placeholder parsing and substitution for conversion step arguments.
//!
//! Syntax: `{name}` is replaced by the value of placeholder `name`; `{{` and `}}` produce a
//! literal `{` / `}`. Substitution happens **inside a single argument only**: the result is
//! always exactly one argument, with no word splitting, globbing or shell interpretation.

use std::fmt;

/// The placeholders a conversion step may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Placeholder {
    /// Absolute path of the input file.
    Input,
    /// Absolute path the step must write its result to.
    Output,
    /// File name of the input without its extension (`photo` for `photo.heic`).
    Stem,
    /// Directory containing the input file.
    Dir,
    /// A private, empty temporary directory for this job (removed afterwards).
    TmpDir,
}

impl Placeholder {
    /// All placeholders, in documentation order.
    pub const ALL: [Placeholder; 5] = [
        Placeholder::Input,
        Placeholder::Output,
        Placeholder::Stem,
        Placeholder::Dir,
        Placeholder::TmpDir,
    ];

    /// The name used between braces.
    pub fn name(self) -> &'static str {
        match self {
            Placeholder::Input => "input",
            Placeholder::Output => "output",
            Placeholder::Stem => "stem",
            Placeholder::Dir => "dir",
            Placeholder::TmpDir => "tmpdir",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }
}

/// A syntax error in a templated argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// `{name}` where `name` is not a known placeholder.
    Unknown(String),
    /// A `{` without a matching `}`.
    Unclosed,
    /// A single `}` that does not close a placeholder (use `}}` for a literal brace).
    StrayClose,
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TemplateError::Unknown(name) => {
                let known: Vec<_> = Placeholder::ALL
                    .iter()
                    .map(|p| format!("{{{}}}", p.name()))
                    .collect();
                write!(
                    f,
                    "unknown placeholder {{{name}}} (known: {})",
                    known.join(", ")
                )
            }
            TemplateError::Unclosed => write!(f, "unclosed '{{' (use '{{{{' for a literal brace)"),
            TemplateError::StrayClose => {
                write!(f, "unmatched '}}' (use '}}}}' for a literal brace)")
            }
        }
    }
}

impl std::error::Error for TemplateError {}

/// One piece of a parsed template.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece<'a> {
    Literal(&'a str),
    Brace(char),
    Var(Placeholder),
}

fn parse(template: &str) -> Result<Vec<Piece<'_>>, TemplateError> {
    let mut pieces = Vec::new();
    let mut rest = template;
    while let Some(pos) = rest.find(['{', '}']) {
        if pos > 0 {
            pieces.push(Piece::Literal(&rest[..pos]));
        }
        let after = &rest[pos + 1..];
        if rest.as_bytes()[pos] == b'}' {
            if let Some(stripped) = after.strip_prefix('}') {
                pieces.push(Piece::Brace('}'));
                rest = stripped;
                continue;
            }
            return Err(TemplateError::StrayClose);
        }
        if let Some(stripped) = after.strip_prefix('{') {
            pieces.push(Piece::Brace('{'));
            rest = stripped;
            continue;
        }
        let end = after.find('}').ok_or(TemplateError::Unclosed)?;
        let name = &after[..end];
        let var =
            Placeholder::from_name(name).ok_or_else(|| TemplateError::Unknown(name.to_owned()))?;
        pieces.push(Piece::Var(var));
        rest = &after[end + 1..];
    }
    if !rest.is_empty() {
        pieces.push(Piece::Literal(rest));
    }
    Ok(pieces)
}

/// Checks the syntax of `template` and returns the placeholders it uses.
pub fn placeholders_in(template: &str) -> Result<Vec<Placeholder>, TemplateError> {
    Ok(parse(template)?
        .into_iter()
        .filter_map(|p| match p {
            Piece::Var(v) => Some(v),
            _ => None,
        })
        .collect())
}

/// Substitutes placeholders in `template` using `value`, producing exactly one argument.
pub fn substitute(
    template: &str,
    value: impl Fn(Placeholder) -> String,
) -> Result<String, TemplateError> {
    let mut out = String::with_capacity(template.len());
    for piece in parse(template)? {
        match piece {
            Piece::Literal(s) => out.push_str(s),
            Piece::Brace(c) => out.push(c),
            Piece::Var(v) => out.push_str(&value(v)),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(p: Placeholder) -> String {
        match p {
            Placeholder::Input => "/a b/in put.heic".into(),
            Placeholder::Output => "/a b/out.jpg".into(),
            Placeholder::Stem => "in put".into(),
            Placeholder::Dir => "/a b".into(),
            Placeholder::TmpDir => "/tmp/x".into(),
        }
    }

    #[test]
    fn substitutes_whole_and_partial_arguments() {
        assert_eq!(substitute("{input}", values).unwrap(), "/a b/in put.heic");
        assert_eq!(
            substitute("{tmpdir}/icon.iconset", values).unwrap(),
            "/tmp/x/icon.iconset"
        );
        assert_eq!(
            substitute("--name={stem}-{stem}", values).unwrap(),
            "--name=in put-in put"
        );
        assert_eq!(substitute("plain", values).unwrap(), "plain");
        assert_eq!(substitute("", values).unwrap(), "");
    }

    #[test]
    fn values_are_not_reinterpreted() {
        // A value containing braces or shell metacharacters is inserted verbatim.
        let v = |_| "{output} $(rm -rf ~) *".to_string();
        assert_eq!(substitute("{input}", v).unwrap(), "{output} $(rm -rf ~) *");
    }

    #[test]
    fn escaped_braces() {
        assert_eq!(substitute("{{}}", values).unwrap(), "{}");
        assert_eq!(substitute("a{{{stem}}}b", values).unwrap(), "a{in put}b");
        assert_eq!(
            substitute("scale=iw*min(1\\,{{w}}/iw)", values).unwrap(),
            "scale=iw*min(1\\,{w}/iw)"
        );
    }

    #[test]
    fn errors() {
        assert_eq!(
            substitute("{nope}", values),
            Err(TemplateError::Unknown("nope".into()))
        );
        assert_eq!(substitute("{input", values), Err(TemplateError::Unclosed));
        assert_eq!(substitute("x}", values), Err(TemplateError::StrayClose));
        assert_eq!(
            substitute("{}", values),
            Err(TemplateError::Unknown(String::new()))
        );
        assert!(
            TemplateError::Unknown("x".into())
                .to_string()
                .contains("{input}")
        );
    }

    #[test]
    fn lists_placeholders() {
        assert_eq!(
            placeholders_in("{input}:{output}{{x}}").unwrap(),
            vec![Placeholder::Input, Placeholder::Output]
        );
        assert!(placeholders_in("none").unwrap().is_empty());
    }
}
