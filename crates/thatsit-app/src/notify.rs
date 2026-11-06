//! User notifications (`UNUserNotificationCenter`).
//!
//! Notifications only work from a bundled `.app` (the center needs a bundle identifier);
//! in `cargo run` dev mode they are only logged.

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_foundation::{NSBundle, NSError, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationRequest,
    UNUserNotificationCenter,
};

use crate::log;

fn bundled() -> bool {
    NSBundle::mainBundle().bundleIdentifier().is_some()
}

/// Asks for permission to show notifications (once; macOS remembers the answer).
pub fn init() {
    if !bundled() {
        log::info("dev mode (not bundled): notifications are logged only");
        return;
    }
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let handler = RcBlock::new(|granted: Bool, _error: *mut NSError| {
        if !granted.as_bool() {
            log::info("notifications not allowed by the user");
        }
    });
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
        &handler,
    );
}

/// Shows a notification (or logs it in dev mode).
pub fn post(title: &str, body: &str) {
    log::info(format!("notification: {title} — {body}"));
    if !bundled() {
        return;
    }
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(title));
    content.setBody(&NSString::from_str(body));
    let id = NSString::from_str(&format!(
        "thatsit-{}",
        std::process::id() as u64 ^ rand_id()
    ));
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(&id, &content, None);
    UNUserNotificationCenter::currentNotificationCenter()
        .addNotificationRequest_withCompletionHandler(&request, None);
}

fn rand_id() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}
