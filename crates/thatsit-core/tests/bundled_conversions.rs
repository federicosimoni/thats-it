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

/// No conversion may be dropped from the ring: for common file types, every category ring
/// (and the flat ring) must fit in `MAX_SECTORS`, with every tool and engine installed.
#[test]
fn every_match_fits_in_the_ring() {
    use thatsit_core::conversion::{Availability, FileInfo};
    use thatsit_core::menu::Menu;

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conversions");
    let mut registry = Registry::new();
    registry.load_dir(&dir, Origin::Bundled);
    let mut all = Availability::default();
    for c in registry.conversions() {
        all.tools.extend(c.requires.iter().cloned());
        all.engines.extend(c.engines().map(str::to_owned));
    }

    // (file name, UTType followed by the supertypes macOS reports for it)
    let image = [
        "public.image",
        "public.data",
        "public.item",
        "public.content",
    ];
    let samples: &[(&str, &str, &[&str])] = &[
        ("a.png", "public.png", &image),
        ("a.jpg", "public.jpeg", &image),
        ("a.tiff", "public.tiff", &image),
        ("a.gif", "com.compuserve.gif", &image),
        ("a.bmp", "com.microsoft.bmp", &image),
        ("a.webp", "org.webmproject.webp", &image),
        (
            "a.heic",
            "public.heic",
            &[
                "public.heif-standard",
                "public.image",
                "public.data",
                "public.item",
            ],
        ),
        (
            "a.svg",
            "public.svg-image",
            &[
                "public.image",
                "public.xml",
                "public.text",
                "public.data",
                "public.item",
            ],
        ),
        (
            "a.pdf",
            "com.adobe.pdf",
            &[
                "public.composite-content",
                "public.data",
                "public.item",
                "public.content",
            ],
        ),
        (
            "a.docx",
            "org.openxmlformats.wordprocessingml.document",
            &[
                "org.openxmlformats.openxml",
                "public.zip-archive",
                "public.composite-content",
                "public.data",
                "public.item",
            ],
        ),
        (
            "a.rtf",
            "public.rtf",
            &[
                "public.text",
                "public.data",
                "public.item",
                "public.content",
            ],
        ),
        (
            "a.html",
            "public.html",
            &[
                "public.text",
                "public.data",
                "public.item",
                "public.content",
            ],
        ),
        (
            "a.txt",
            "public.plain-text",
            &[
                "public.text",
                "public.data",
                "public.item",
                "public.content",
            ],
        ),
        (
            "a.md",
            "net.daringfireball.markdown",
            &[
                "public.plain-text",
                "public.text",
                "public.data",
                "public.item",
            ],
        ),
        (
            "a.wav",
            "com.microsoft.waveform-audio",
            &[
                "public.audio",
                "public.audiovisual-content",
                "public.data",
                "public.item",
            ],
        ),
        (
            "a.m4a",
            "com.apple.m4a-audio",
            &[
                "public.mpeg-4-audio",
                "public.audio",
                "public.audiovisual-content",
                "public.data",
                "public.item",
            ],
        ),
        (
            "a.mov",
            "com.apple.quicktime-movie",
            &[
                "public.movie",
                "public.audiovisual-content",
                "public.data",
                "public.item",
            ],
        ),
        (
            "a.mp4",
            "public.mpeg-4",
            &[
                "public.movie",
                "public.audiovisual-content",
                "public.data",
                "public.item",
            ],
        ),
        (
            "a.zip",
            "public.zip-archive",
            &["public.archive", "public.data", "public.item"],
        ),
    ];

    let mut problems = Vec::new();
    for (name, uti, supertypes) in samples {
        let mut types = vec![uti.to_string()];
        types.extend(supertypes.iter().map(|s| s.to_string()));
        let info = FileInfo::new(Path::new(name), types);
        let matches = registry.matching(&[info], &all);
        let categories: Vec<_> = matches.iter().map(|c| c.category).collect();
        let menu = Menu::new(&categories);
        if menu.hidden_count() > 0 {
            let ids: Vec<_> = matches
                .iter()
                .map(|c| format!("{} ({:?})", c.id, c.category))
                .collect();
            problems.push(format!(
                "{name}: {} conversion(s) would not fit in the ring: {}",
                menu.hidden_count(),
                ids.join(", ")
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
