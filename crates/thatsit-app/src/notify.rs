//! User notifications (`UNUserNotificationCenter`).
//!
//! Notifications only work from a bundled `.app` (the center needs a bundle identifier);
//! in `cargo run` dev mode they are only logged.

use std::ptr::NonNull;

use block2::{DynBlock, RcBlock};
use objc2::rc::Retained;
use objc2::runtime::{Bool, NSObject, ProtocolObject};
use objc2::{AllocAnyThread, define_class, msg_send};
use objc2_foundation::{NSBundle, NSError, NSObjectProtocol, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationSettings,
    UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};

use crate::log;

define_class!(
    // SAFETY:
    // - NSObject has no subclassing requirements.
    // - `Presenter` does not implement `Drop` and has no ivars, so it is safe on any thread
    //   (the notification center calls its delegate on a background queue).
    #[unsafe(super(NSObject))]
    struct Presenter;

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for Presenter {}

    // SAFETY: the method has the signature declared by `UNUserNotificationCenterDelegate`.
    unsafe impl UNUserNotificationCenterDelegate for Presenter {
        // Without this, macOS hides notifications while the app is the active app (for
        // example right after using the menu bar or Preferences).
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion: &DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion.call((UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::List
                | UNNotificationPresentationOptions::Sound,));
        }
    }
);

/// `code: description` of an `NSError` pointer from a completion handler, if any.
fn describe(error: *mut NSError) -> Option<String> {
    // SAFETY: completion handlers pass either null or a valid NSError that lives for the
    // duration of the call.
    let error = unsafe { error.as_ref() }?;
    Some(format!(
        "{} {}: {}",
        error.domain(),
        error.code(),
        error.localizedDescription()
    ))
}

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
    // SAFETY: `init` is NSObject's designated initializer and `Presenter` has no ivars.
    let presenter: Retained<Presenter> = unsafe { msg_send![Presenter::alloc(), init] };
    center.setDelegate(Some(ProtocolObject::from_ref(&*presenter)));
    // The center keeps only a weak reference; the presenter must live as long as the app.
    std::mem::forget(presenter);

    let handler = RcBlock::new(|granted: Bool, error: *mut NSError| match describe(error) {
        Some(e) => log::info(format!("notification permission request failed: {e}")),
        None if !granted.as_bool() => log::info("notifications not allowed by the user"),
        None => log::info("notifications allowed"),
    });
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
        &handler,
    );
    let settings = RcBlock::new(|settings: NonNull<UNNotificationSettings>| {
        // SAFETY: the center passes a valid settings object for the duration of the call.
        let settings = unsafe { settings.as_ref() };
        let status = match settings.authorizationStatus() {
            UNAuthorizationStatus::NotDetermined => "not determined",
            UNAuthorizationStatus::Denied => "denied",
            UNAuthorizationStatus::Authorized => "authorized",
            UNAuthorizationStatus::Provisional => "provisional",
            _ => "other",
        };
        log::info(format!("notification permission: {status}"));
    });
    center.getNotificationSettingsWithCompletionHandler(&settings);
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
    let done = RcBlock::new(|error: *mut NSError| {
        if let Some(e) = describe(error) {
            log::info(format!("notification not shown: {e}"));
        }
    });
    UNUserNotificationCenter::currentNotificationCenter()
        .addNotificationRequest_withCompletionHandler(&request, Some(&done));
}

fn rand_id() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}
