//! Loads the conversion registry and checks which tools are installed.

use std::path::PathBuf;
use std::sync::Arc;

use objc2_app_kit::NSImage;
use objc2_foundation::{NSBundle, NSLocale, NSString};
use thatsit_core::conversion::{Availability, Conversion, Origin, Registry};
use thatsit_core::engines::CORE_ENGINES;
use thatsit_core::tools::ToolFinder;

use crate::app_engines::APP_ENGINES;

use crate::log;

/// Bundle identifier, also the name of the per-user folders.
pub const BUNDLE_ID: &str = "io.github.federicosimoni.thatsit";

/// Everything the drop view needs to match dragged files.
pub struct Catalog {
    pub registry: Registry,
    pub availability: Availability,
    /// Shared with the job thread to resolve tool paths.
    pub tools: Arc<ToolFinder>,
    /// Preferred UI languages, most preferred first (for conversion labels).
    pub languages: Vec<String>,
}

impl Catalog {
    pub fn load() -> Self {
        let mut registry = Registry::new();
        if let Some(dir) = bundled_dir() {
            log::info(format!("bundled conversions: {}", dir.display()));
            registry.load_dir(&dir, Origin::Bundled);
        } else {
            log::info("bundled conversions folder not found");
        }
        if let Some(dir) = user_dir() {
            registry.load_dir(&dir, Origin::User);
        }
        for issue in registry.issues() {
            log::info(format!(
                "invalid conversion {}: {}",
                issue.path.display(),
                issue.message
            ));
        }

        let tools = Arc::new(ToolFinder::from_env());
        let availability = availability_for(&registry, &tools);
        for c in registry.conversions() {
            let missing = availability.missing_tools(c);
            let engines = availability.missing_engines(c);
            if !missing.is_empty() || !engines.is_empty() {
                log::info(format!(
                    "hidden {}: missing {}",
                    c.id,
                    missing
                        .iter()
                        .chain(engines.iter())
                        .copied()
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        for c in registry.conversions() {
            if let Some(icon) = &c.icon
                && NSImage::imageWithSystemSymbolName_accessibilityDescription(
                    &NSString::from_str(icon),
                    None,
                )
                .is_none()
            {
                log::info(format!("{}: unknown SF Symbol {icon:?}", c.id));
            }
        }
        log::info(format!(
            "{} conversion(s) loaded",
            registry.conversions().len()
        ));

        let languages = NSLocale::preferredLanguages()
            .iter()
            .map(|l| l.to_string())
            .collect();
        Self {
            registry,
            availability,
            tools,
            languages,
        }
    }

    pub fn label<'c>(&self, conversion: &'c Conversion) -> &'c str {
        conversion.label_for(&self.languages)
    }

    /// Whether Italian is preferred over English (UI strings until Fluent lands in Phase 4).
    pub fn prefers_italian(&self) -> bool {
        self.languages
            .first()
            .is_some_and(|l| l.to_ascii_lowercase().starts_with("it"))
    }
}

fn availability_for(registry: &Registry, tools: &ToolFinder) -> Availability {
    let mut availability = Availability::default();
    for c in registry.conversions() {
        for t in &c.requires {
            if tools.find(t).is_some() {
                availability.tools.insert(t.clone());
            }
        }
    }
    availability.engines = CORE_ENGINES
        .iter()
        .chain(APP_ENGINES)
        .map(|e| e.to_string())
        .collect();
    availability
}

/// `That's it.app/Contents/Resources/conversions`, or the repository's `conversions/`
/// folder when running unbundled with `cargo run`.
fn bundled_dir() -> Option<PathBuf> {
    let resources = NSBundle::mainBundle()
        .resourcePath()
        .map(|p| PathBuf::from(p.to_string()).join("conversions"));
    if let Some(dir) = resources.filter(|d| d.is_dir()) {
        return Some(dir);
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conversions");
    dev.is_dir().then_some(dev)
}

/// `~/Library/Application Support/<bundle id>/conversions`.
fn user_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join("Library/Application Support")
            .join(BUNDLE_ID)
            .join("conversions")
    })
}
