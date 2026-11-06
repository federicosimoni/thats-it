//! *That's it* — drag & drop file converter for macOS.
//!
//! Phase 0 spike: a transparent widget on the desktop layer that accepts file drags and
//! logs file paths, UTTypes, cursor angle and the computed sector index.

#[cfg(target_os = "macos")]
mod drop_view;
#[cfg(target_os = "macos")]
mod files;
#[cfg(target_os = "macos")]
mod log;
#[cfg(target_os = "macos")]
mod window;

#[cfg(target_os = "macos")]
fn main() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

    let mtm = MainThreadMarker::new().expect("must run on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    // No Dock icon, no Cmd+Tab entry (the bundled app will also set LSUIElement).
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let widget = window::Widget::new(mtm);
    widget.show();
    log::info("widget ready — drag files from the Finder onto the circle (Ctrl+C to quit)");

    app.run();
    drop(widget);
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("That's it currently runs on macOS only.");
    std::process::exit(1);
}
