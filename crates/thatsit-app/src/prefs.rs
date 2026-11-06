//! User preferences stored in `NSUserDefaults`.
//!
//! The Preferences window arrives in Phase 4; until then values can be set from Terminal,
//! e.g. `defaults write thatsit useSystemAccent -bool true` (`thatsit` is the defaults
//! domain of the unbundled dev binary; the bundled app uses its bundle identifier).

use objc2::rc::Retained;
use objc2_foundation::{NSString, NSUserDefaults};

const USE_SYSTEM_ACCENT: &str = "useSystemAccent";

/// Highlight the selected sector with the system accent color instead of glass.
/// Off by default (decided with the owner).
pub fn use_system_accent() -> bool {
    NSUserDefaults::standardUserDefaults().boolForKey(&NSString::from_str(USE_SYSTEM_ACCENT))
}

fn position_keys(screen: &str) -> (Retained<NSString>, Retained<NSString>, Retained<NSString>) {
    (
        NSString::from_str(&format!("position.{screen}.x")),
        NSString::from_str(&format!("position.{screen}.y")),
        NSString::from_str(&format!("position.{screen}.set")),
    )
}

/// The saved widget centre for `screen`, as fractions (0…1) of its visible frame.
pub fn position(screen: &str) -> Option<(f64, f64)> {
    let d = NSUserDefaults::standardUserDefaults();
    let (x, y, set) = position_keys(screen);
    if !d.boolForKey(&set) {
        return None;
    }
    let (fx, fy) = (d.doubleForKey(&x), d.doubleForKey(&y));
    ((0.0..=1.0).contains(&fx) && (0.0..=1.0).contains(&fy)).then_some((fx, fy))
}

/// Saves the widget centre for `screen`, as fractions of its visible frame.
pub fn set_position(screen: &str, fx: f64, fy: f64) {
    let d = NSUserDefaults::standardUserDefaults();
    let (x, y, set) = position_keys(screen);
    d.setDouble_forKey(fx.clamp(0.0, 1.0), &x);
    d.setDouble_forKey(fy.clamp(0.0, 1.0), &y);
    d.setBool_forKey(true, &set);
}
