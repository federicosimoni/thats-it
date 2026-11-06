//! Validates every bundled conversion in `conversions/` (run by CI on every PR).

use std::path::Path;

use thatsit_core::conversion::{Origin, Registry};

#[test]
fn all_bundled_conversions_are_valid() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conversions");
    let count = std::fs::read_dir(&dir)
        .expect("conversions/ directory")
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
        .count();
    assert!(
        count > 0,
        "no bundled conversions found in {}",
        dir.display()
    );

    let mut registry = Registry::new();
    registry.load_dir(&dir, Origin::Bundled);
    let report: Vec<String> = registry
        .issues()
        .iter()
        .map(|i| format!("{}: {}", i.path.display(), i.message))
        .collect();
    assert!(
        report.is_empty(),
        "invalid bundled conversions:\n{}",
        report.join("\n")
    );
    assert_eq!(registry.conversions().len(), count);

    for c in registry.conversions() {
        let file = c
            .path
            .as_ref()
            .unwrap()
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(
            file, c.id,
            "bundled file names must equal the conversion id"
        );
        assert!(
            c.label.contains_key("it"),
            "{}: missing Italian label",
            c.id
        );
        assert!(
            c.icon.is_some(),
            "{}: bundled conversions need an icon",
            c.id
        );
    }
}
