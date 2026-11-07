//! User preferences stored in `NSUserDefaults`.
//!
//! Edited in the Preferences window. When running unbundled (`cargo run`), the defaults
//! domain is the binary name, so values can also be set with
//! `defaults write thatsit <key> …`; the bundled app uses its bundle identifier.

use std::path::PathBuf;

use objc2::rc::Retained;
use objc2_foundation::{NSArray, NSString, NSUserDefaults};
use thatsit_core::geometry::Anchor;

const USE_SYSTEM_ACCENT: &str = "useSystemAccent";
const WIDGET_HIDDEN: &str = "widgetHidden";
const WIDGET_SIZE: &str = "widgetSize";
const SCREEN: &str = "screen";
const DISABLED: &str = "disabledConversions";
const FALLBACK_DIR: &str = "fallbackFolder";

fn defaults() -> Retained<NSUserDefaults> {
    NSUserDefaults::standardUserDefaults()
}

fn key(k: &str) -> Retained<NSString> {
    NSString::from_str(k)
}

fn string(k: &str) -> Option<String> {
    defaults().stringForKey(&key(k)).map(|s| s.to_string())
}

fn set_string(k: &str, value: Option<&str>) {
    let d = defaults();
    match value {
        // SAFETY: an NSString is a valid property-list value for NSUserDefaults.
        Some(v) => unsafe { d.setObject_forKey(Some(&NSString::from_str(v)), &key(k)) },
        None => d.removeObjectForKey(&key(k)),
    }
}

/// Highlight the selected sector with the system accent color instead of glass.
/// Off by default (decided with the owner).
pub fn use_system_accent() -> bool {
    defaults().boolForKey(&key(USE_SYSTEM_ACCENT))
}

pub fn set_use_system_accent(on: bool) {
    defaults().setBool_forKey(on, &key(USE_SYSTEM_ACCENT));
}

/// The widget is hidden (from the menu bar's Hide Widget).
pub fn widget_hidden() -> bool {
    defaults().boolForKey(&key(WIDGET_HIDDEN))
}

pub fn set_widget_hidden(hidden: bool) {
    defaults().setBool_forKey(hidden, &key(WIDGET_HIDDEN));
}

/// Size of the collapsed widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetSize {
    Small,
    Medium,
    Large,
}

impl WidgetSize {
    pub const ALL: [WidgetSize; 3] = [WidgetSize::Small, WidgetSize::Medium, WidgetSize::Large];

    /// Diameter of the collapsed glass disc, in points.
    pub fn disc(self) -> f64 {
        match self {
            WidgetSize::Small => 60.0,
            WidgetSize::Medium => 76.0,
            WidgetSize::Large => 92.0,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            WidgetSize::Small => "small",
            WidgetSize::Medium => "medium",
            WidgetSize::Large => "large",
        }
    }

    /// Fluent message id of its name.
    pub fn message_id(self) -> &'static str {
        match self {
            WidgetSize::Small => "prefs-size-small",
            WidgetSize::Medium => "prefs-size-medium",
            WidgetSize::Large => "prefs-size-large",
        }
    }
}

pub fn widget_size() -> WidgetSize {
    match string(WIDGET_SIZE).as_deref() {
        Some("small") => WidgetSize::Small,
        Some("large") => WidgetSize::Large,
        _ => WidgetSize::Medium,
    }
}

pub fn set_widget_size(size: WidgetSize) {
    set_string(WIDGET_SIZE, Some(size.as_str()));
}

/// The screen chosen for the widget (its localized name), if any.
pub fn screen() -> Option<String> {
    string(SCREEN)
}

pub fn set_screen(name: Option<&str>) {
    set_string(SCREEN, name);
}

/// Ids of conversions the user switched off.
pub fn disabled_conversions() -> Vec<String> {
    defaults()
        .stringArrayForKey(&key(DISABLED))
        .map(|a| a.iter().map(|s| s.to_string()).collect())
        .unwrap_or_default()
}

pub fn set_conversion_enabled(id: &str, enabled: bool) {
    let mut ids = disabled_conversions();
    ids.retain(|x| x != id);
    if !enabled {
        ids.push(id.to_owned());
    }
    ids.sort();
    let strings: Vec<Retained<NSString>> = ids.iter().map(|s| NSString::from_str(s)).collect();
    let array = NSArray::from_retained_slice(&strings);
    // SAFETY: an NSArray of NSStrings is a valid property-list value.
    unsafe { defaults().setObject_forKey(Some(&array), &key(DISABLED)) };
}

/// Where results go when the original folder is not writable (default `~/Downloads`).
pub fn fallback_dir() -> PathBuf {
    string(FALLBACK_DIR)
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(default_fallback_dir)
}

pub fn set_fallback_dir(path: &str) {
    set_string(FALLBACK_DIR, Some(path));
}

pub fn default_fallback_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join("Downloads"))
        .unwrap_or_else(std::env::temp_dir)
}

/// The anchor the widget is snapped to on `screen` (Option-drag / Option-click).
pub fn anchor(screen: &str) -> Option<Anchor> {
    string(&format!("anchor.{screen}")).and_then(|s| Anchor::parse(&s))
}

pub fn set_anchor(screen: &str, anchor: Anchor) {
    set_string(&format!("anchor.{screen}"), Some(anchor.as_str()));
}

/// A free widget centre saved by earlier versions, as fractions (0…1) of the screen's
/// visible frame. Only read once, to migrate it to the nearest anchor.
pub fn legacy_position(screen: &str) -> Option<(f64, f64)> {
    let d = defaults();
    if !d.boolForKey(&key(&format!("position.{screen}.set"))) {
        return None;
    }
    let fx = d.doubleForKey(&key(&format!("position.{screen}.x")));
    let fy = d.doubleForKey(&key(&format!("position.{screen}.y")));
    ((0.0..=1.0).contains(&fx) && (0.0..=1.0).contains(&fy)).then_some((fx, fy))
}
