//! *That's it* — drag & drop file converter for macOS.

#[cfg(target_os = "macos")]
mod catalog;
#[cfg(target_os = "macos")]
mod files;
#[cfg(target_os = "macos")]
mod log;
#[cfg(target_os = "macos")]
mod prefs;
#[cfg(target_os = "macos")]
mod ring_view;
#[cfg(target_os = "macos")]
mod window;

#[cfg(target_os = "macos")]
fn main() {
    use std::rc::Rc;

    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

    let mtm = MainThreadMarker::new().expect("must run on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    // No Dock icon, no Cmd+Tab entry (the bundled app will also set LSUIElement).
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let catalog = Rc::new(catalog::Catalog::load());
    let widget = window::Widget::new(mtm, catalog);
    widget.show();

    // Visual self-test used by CI: `THATSIT_DEMO=photo.heic[,more files]` opens the ring
    // as if those files were dragged over it, with the first sector highlighted.
    if let Ok(demo) = std::env::var("THATSIT_DEMO") {
        let paths: Vec<String> = demo.split(',').map(str::to_owned).collect();
        log::info(format!("demo mode: {paths:?}"));
        widget.view().start_demo(&paths, true);
    } else {
        log::info("widget ready — drag files from the Finder onto the circle (Ctrl+C to quit)");
    }

    app.run();
    drop(widget);
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("That's it currently runs on macOS only.");
    std::process::exit(1);
}
