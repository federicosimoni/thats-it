//! Locale consistency (run by CI on every PR):
//! - **fails** when a locale defines a message that English (the reference) does not;
//! - **warns** (GitHub annotation) when a locale is missing a message English has.

use std::collections::BTreeSet;

use thatsit_core::i18n::{FALLBACK, LOCALES, message_ids};

#[test]
fn locales_match_the_english_reference() {
    let reference: BTreeSet<String> = LOCALES
        .iter()
        .find(|(tag, _)| *tag == FALLBACK)
        .map(|(_, src)| message_ids(src).unwrap().into_iter().collect())
        .unwrap();

    let mut extra = Vec::new();
    for (tag, source) in LOCALES.iter().filter(|(t, _)| *t != FALLBACK) {
        let ids: BTreeSet<String> = message_ids(source)
            .unwrap_or_else(|e| panic!("locales/{tag}/main.ftl does not parse: {e}"))
            .into_iter()
            .collect();
        for missing in reference.difference(&ids) {
            println!("::warning file=locales/{tag}/main.ftl::missing translation for `{missing}`");
        }
        for id in ids.difference(&reference) {
            extra.push(format!(
                "locales/{tag}/main.ftl: `{id}` is not defined in English"
            ));
        }
    }
    assert!(extra.is_empty(), "{}", extra.join("\n"));
}
