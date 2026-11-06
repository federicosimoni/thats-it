//! The desktop-level widget window.
//!
//! A borderless, transparent window that sits just above the desktop icons and below every
//! normal window, on every Space. While collapsed its frame is only the size of the icon so
//! it never blocks clicks on desktop icons around it; while a drag hovers it, the frame
//! grows (centred) to the size of the expanded ring.

use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSWindow, NSWindowCollectionBehavior, NSWindowLevel,
    NSWindowStyleMask,
};
use objc2_core_graphics::{CGWindowLevelForKey, CGWindowLevelKey};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use crate::catalog::Catalog;
use crate::ring_view::{RingView, collapsed_size};

/// The widget: its window plus the ring view inside it.
pub struct Widget {
    window: Retained<NSWindow>,
    view: Retained<RingView>,
}

impl Widget {
    pub fn new(mtm: MainThreadMarker, catalog: Rc<Catalog>) -> Self {
        let frame = NSRect::new(
            NSPoint::ZERO,
            NSSize::new(collapsed_size(), collapsed_size()),
        );

        // SAFETY: `setReleasedWhenClosed(false)` is called right below, which is required
        // when an NSWindow is created outside of a window controller and owned by Rust.
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                frame,
                NSWindowStyleMask::Borderless,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        // SAFETY: we own the window through `Retained`; AppKit must not release it on close.
        unsafe { window.setReleasedWhenClosed(false) };

        window.setOpaque(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
        window.setHasShadow(false);
        window.setLevel(desktop_icon_level() + 1);
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        // Drag the disc to reposition the widget (the position is saved per screen).
        window.setMovableByWindowBackground(true);

        let view = RingView::new(mtm, catalog);
        window.setContentView(Some(&view));
        window.setDelegate(Some(ProtocolObject::from_ref(&*view)));
        view.place_initially();

        Self { window, view }
    }

    pub fn show(&self) {
        // `orderFrontRegardless` because an accessory app is never the active app at launch.
        self.window.orderFrontRegardless();
    }

    pub fn view(&self) -> &RingView {
        &self.view
    }

    pub fn view_retained(&self) -> Retained<RingView> {
        self.view.clone()
    }

    pub fn window(&self) -> Retained<NSWindow> {
        self.window.clone()
    }
}

/// Window level of the desktop icons (Finder's desktop), as an `NSWindowLevel`.
fn desktop_icon_level() -> NSWindowLevel {
    CGWindowLevelForKey(CGWindowLevelKey::DesktopIconWindowLevelKey) as NSWindowLevel
}
