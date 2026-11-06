//! UI strings with Project Fluent.
//!
//! The `.ftl` files in `locales/<lang>/main.ftl` are embedded at build time. English is the
//! reference and the final fallback: a message missing in the chosen language is taken
//! from English, and a message missing everywhere shows its id.
//!
//! Language negotiation: exact match (`it-CH`) → base language (`it`) → `en`.

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use fluent_langneg::{NegotiationStrategy, negotiate_languages};
use unic_langid::LanguageIdentifier;

/// The reference / fallback language.
pub const FALLBACK: &str = "en";

/// Every shipped locale: (language tag, FTL source).
pub const LOCALES: &[(&str, &str)] = &[
    ("en", include_str!("../../../locales/en/main.ftl")),
    ("it", include_str!("../../../locales/it/main.ftl")),
];

/// An argument value for [`I18n::tr_args`].
#[derive(Debug, Clone, Copy)]
pub enum Arg<'a> {
    Str(&'a str),
    Num(f64),
}

impl<'a> From<&'a str> for Arg<'a> {
    fn from(s: &'a str) -> Self {
        Arg::Str(s)
    }
}

impl<'a> From<&'a String> for Arg<'a> {
    fn from(s: &'a String) -> Self {
        Arg::Str(s)
    }
}

impl From<usize> for Arg<'_> {
    fn from(n: usize) -> Self {
        Arg::Num(n as f64)
    }
}

/// Localized strings for one negotiated language.
pub struct I18n {
    language: String,
    bundle: FluentBundle<FluentResource>,
    fallback: FluentBundle<FluentResource>,
}

impl std::fmt::Debug for I18n {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("I18n")
            .field("language", &self.language)
            .finish()
    }
}

impl I18n {
    /// Negotiates `requested` (most preferred first, e.g. `["it-CH", "en-US"]`) against the
    /// shipped locales.
    pub fn new<S: AsRef<str>>(requested: &[S]) -> Self {
        let language = negotiate(requested);
        Self {
            bundle: bundle_for(&language),
            fallback: bundle_for(FALLBACK),
            language,
        }
    }

    /// The negotiated language tag (`"en"`, `"it"`).
    pub fn language(&self) -> &str {
        &self.language
    }

    /// Languages to use for conversion labels: the negotiated one, then English.
    pub fn label_languages(&self) -> Vec<String> {
        let mut v = vec![self.language.clone()];
        if self.language != FALLBACK {
            v.push(FALLBACK.to_owned());
        }
        v
    }

    /// The message `id` without arguments.
    pub fn tr(&self, id: &str) -> String {
        self.format(id, None)
    }

    /// The message `id` with named arguments.
    pub fn tr_args(&self, id: &str, args: &[(&str, Arg<'_>)]) -> String {
        let mut fargs = FluentArgs::new();
        for (k, v) in args {
            match v {
                Arg::Str(s) => fargs.set(*k, FluentValue::from(*s)),
                Arg::Num(n) => fargs.set(*k, FluentValue::from(*n)),
            }
        }
        self.format(id, Some(&fargs))
    }

    fn format(&self, id: &str, args: Option<&FluentArgs>) -> String {
        for bundle in [&self.bundle, &self.fallback] {
            if let Some(pattern) = bundle.get_message(id).and_then(|m| m.value()) {
                let mut errors = Vec::new();
                return bundle
                    .format_pattern(pattern, args, &mut errors)
                    .into_owned();
            }
        }
        id.to_owned()
    }
}

/// Picks the best shipped locale for `requested`.
pub fn negotiate<S: AsRef<str>>(requested: &[S]) -> String {
    let requested: Vec<LanguageIdentifier> = requested
        .iter()
        .filter_map(|s| s.as_ref().replace('_', "-").parse().ok())
        .collect();
    let available: Vec<LanguageIdentifier> = LOCALES
        .iter()
        .map(|(tag, _)| tag.parse().expect("valid shipped tag"))
        .collect();
    let default: LanguageIdentifier = FALLBACK.parse().expect("valid tag");
    negotiate_languages(
        &requested,
        &available,
        Some(&default),
        NegotiationStrategy::Lookup,
    )
    .first()
    .map(|l| l.to_string())
    .unwrap_or_else(|| FALLBACK.to_owned())
}

fn bundle_for(language: &str) -> FluentBundle<FluentResource> {
    let source = LOCALES
        .iter()
        .find(|(tag, _)| *tag == language)
        .map(|(_, s)| *s)
        .unwrap_or(LOCALES[0].1);
    let langid: LanguageIdentifier = language.parse().unwrap_or_default();
    let mut bundle = FluentBundle::new(vec![langid]);
    // No Unicode isolation marks: strings go to menus, notifications and logs.
    bundle.set_use_isolating(false);
    let resource = match FluentResource::try_new(source.to_owned()) {
        Ok(r) => r,
        // A broken locale must not break the app; the locale test catches it in CI.
        Err((r, _)) => r,
    };
    let _ = bundle.add_resource(resource);
    bundle
}

/// Message ids defined in an FTL source (for the locale consistency checks).
pub fn message_ids(source: &str) -> Result<Vec<String>, String> {
    let resource =
        FluentResource::try_new(source.to_owned()).map_err(|(_, errors)| format!("{errors:?}"))?;
    Ok(resource
        .entries()
        .filter_map(|e| match e {
            fluent_syntax::ast::Entry::Message(m) => Some(m.id.name.to_owned()),
            _ => None,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiation() {
        assert_eq!(negotiate(&["it-CH", "en-US"]), "it");
        assert_eq!(negotiate(&["it_IT"]), "it");
        assert_eq!(negotiate(&["de-DE", "it"]), "it");
        assert_eq!(negotiate(&["de-DE"]), "en");
        assert_eq!(negotiate(&["en-GB"]), "en");
        assert_eq!(negotiate::<&str>(&[]), "en");
        assert_eq!(negotiate(&["not a tag"]), "en");
    }

    #[test]
    fn translates_with_plurals_and_fallback() {
        let it = I18n::new(&["it"]);
        assert_eq!(it.language(), "it");
        assert_eq!(it.tr("menu-quit"), "Esci da That's it");
        assert_eq!(
            it.tr_args(
                "summary-converted",
                &[
                    ("conversion", "Converti in JPG".into()),
                    ("count", 1usize.into())
                ]
            ),
            "Converti in JPG: 1 file convertito"
        );
        assert_eq!(
            it.tr_args(
                "summary-converted",
                &[("conversion", "X".into()), ("count", 3usize.into())]
            ),
            "X: 3 file convertiti"
        );
        let en = I18n::new(&["en-US"]);
        assert_eq!(
            en.tr_args(
                "summary-converted",
                &[("conversion", "X".into()), ("count", 1usize.into())]
            ),
            "X: 1 file converted"
        );
        assert_eq!(en.tr("no-such-message"), "no-such-message");
        assert_eq!(it.label_languages(), vec!["it", "en"]);
    }

    #[test]
    fn every_locale_parses() {
        for (tag, source) in LOCALES {
            let ids = message_ids(source).unwrap_or_else(|e| panic!("{tag}: {e}"));
            assert!(!ids.is_empty(), "{tag}");
        }
    }
}
