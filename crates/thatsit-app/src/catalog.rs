//! Loads the conversion registry, checks which tools are installed and holds the UI
//! language.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;

use objc2_app_kit::NSImage;
use objc2_foundation::{NSBundle, NSLocale, NSString};
use thatsit_core::conversion::{Availability, Category, Conversion, Origin, Registry, Variant};
use thatsit_core::engines::CORE_ENGINES;
use thatsit_core::i18n::I18n;
use thatsit_core::tools::ToolFinder;

use crate::app_engines::APP_ENGINES;
use crate::files::DraggedFile;
use crate::{log, prefs};

/// Bundle identifier, also the name of the per-user folders.
pub const BUNDLE_ID: &str = "io.github.federicosimoni.thatsit";

/// Everything the UI needs to match dragged files and show strings.
pub struct Catalog {
    pub registry: Registry,
    /// Recomputed when tools are rescanned (Preferences).
    availability: RefCell<Availability>,
    pub tools: Arc<ToolFinder>,
    pub i18n: I18n,
    languages: Vec<String>,
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

        let tools = Arc::new(ToolFinder::from_env());
        let availability = availability_for(&registry, &tools);
        log_hidden(&registry, &availability);
        log::info(format!(
            "{} conversion(s) loaded",
            registry.conversions().len()
        ));

        let i18n = I18n::new(&ui_languages());
        log::info(format!("UI language: {}", i18n.language()));
        let languages = i18n.label_languages();
        Self {
            registry,
            availability: RefCell::new(availability),
            tools,
            i18n,
            languages,
        }
    }

    pub fn availability(&self) -> Availability {
        self.availability.borrow().clone()
    }

    /// Forgets cached tool lookups and recomputes which conversions can run.
    pub fn rescan_tools(&self) {
        self.tools.rescan();
        let availability = availability_for(&self.registry, &self.tools);
        log_hidden(&self.registry, &availability);
        *self.availability.borrow_mut() = availability;
    }

    /// Conversions offered for `files`: valid for all of them, runnable on this Mac and
    /// not switched off in Preferences (the ring, the Finder service and `--list`).
    pub fn compatible(&self, files: &[DraggedFile]) -> Vec<&Conversion> {
        let infos: Vec<_> = files.iter().map(DraggedFile::file_info).collect();
        let disabled = prefs::disabled_conversions();
        self.registry
            .matching(&infos, &self.availability())
            .into_iter()
            .filter(|c| !disabled.contains(&c.id))
            .collect()
    }

    pub fn label<'c>(&self, conversion: &'c Conversion) -> &'c str {
        conversion.label_for(&self.languages)
    }

    pub fn variant_label<'v>(&self, variant: &'v Variant) -> &'v str {
        variant.label_for(&self.languages)
    }

    /// Conversion `id` ready to run with `variant` (the default when `None`), and the
    /// label for progress and notifications ("Convert to JPG — High" for an explicit
    /// variant). `None` for an unknown id or variant.
    pub fn resolve(&self, id: &str, variant: Option<&str>) -> Option<(Conversion, String)> {
        let conversion = self.registry.get(id)?;
        let resolved = conversion.resolve(variant)?;
        let mut label = self.label(conversion).to_owned();
        if let Some(v) = variant.and_then(|v| conversion.variants.iter().find(|x| x.id == v)) {
            label = format!("{label} — {}", self.variant_label(v));
        }
        Some((resolved, label))
    }

    pub fn category_name(&self, c: Category) -> String {
        self.i18n.tr(match c {
            Category::Images => "category-images",
            Category::AudioVideo => "category-audio-video",
            Category::Documents => "category-documents",
            Category::Utilities => "category-utilities",
        })
    }

    /// Every distinct tool required by some conversion, sorted.
    pub fn required_tools(&self) -> Vec<String> {
        let mut tools: Vec<String> = self
            .registry
            .conversions()
            .iter()
            .flat_map(|c| c.requires.iter().cloned())
            .collect();
        tools.sort();
        tools.dedup();
        tools
    }
}

/// The UI languages as macOS resolves them for this app: inside a bundle, its preferred
/// localizations (which honour the per-app language in System Settings); unbundled, the
/// user's preferred languages.
fn ui_languages() -> Vec<String> {
    let bundle = NSBundle::mainBundle();
    if bundle.bundleIdentifier().is_some() {
        let localized: Vec<String> = bundle
            .preferredLocalizations()
            .iter()
            .map(|l| l.to_string())
            .collect();
        if !localized.is_empty() {
            return localized;
        }
    }
    NSLocale::preferredLanguages()
        .iter()
        .map(|l| l.to_string())
        .collect()
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

fn log_hidden(registry: &Registry, availability: &Availability) {
    for c in registry.conversions() {
        let mut missing = availability.missing_tools(c);
        missing.extend(availability.missing_engines(c));
        if !missing.is_empty() {
            log::info(format!("hidden {}: missing {}", c.id, missing.join(", ")));
        }
    }
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
pub fn user_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join("Library/Application Support")
            .join(BUNDLE_ID)
            .join("conversions")
    })
}
