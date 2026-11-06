//! The desktop-level widget window.
//!
//! A borderless, transparent window that sits just above the desktop icons and below every
//! normal window, on every Space. While collapsed its frame is only the size of the icon so
//! it never blocks clicks on desktop icons around it; while a drag hovers it, the frame
//! grows (centred) to the size of the expanded ring.

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSScreen, NSWindow, NSWindowCollectionBehavior, NSWindowLevel,
    NSWindowStyleMask,
};
use objc2_core_graphics::{CGWindowLevelForKey, CGWindowLevelKey};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use crate::drop_view::DropView;

/// Side of the window while collapsed, in points.
pub const COLLAPSED_SIZE: f64 = 80.0;
/// Side of the window while expanded, in points.
pub const EXPANDED_SIZE: f64 = 320.0;
/// Margin from the screen edges for the initial position, in points.
const SCREEN_MARGIN: f64 = 24.0;

/// The widget: its window plus the drop view inside it.
pub struct Widget {
    window: Retained<NSWindow>,
    _view: Retained<DropView>,
}

impl Widget {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let center = initial_center(mtm);
        let frame = square_around(center, COLLAPSED_SIZE);

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
        // Temporary convenience for the spike: drag the circle to move it.
        // Proper drag-to-reposition with a persisted position comes in Phase 2.
        window.setMovableByWindowBackground(true);

        let view = DropView::new(
            mtm,
            NSRect::new(NSPoint::ZERO, NSSize::new(COLLAPSED_SIZE, COLLAPSED_SIZE)),
        );
        window.setContentView(Some(&view));

        Self {
            window,
            _view: view,
        }
    }

    pub fn show(&self) {
        // `orderFrontRegardless` because an accessory app is never the active app at launch.
        self.window.orderFrontRegardless();
    }
}

/// Window level of the desktop icons (Finder's desktop), as an `NSWindowLevel`.
fn desktop_icon_level() -> NSWindowLevel {
    CGWindowLevelForKey(CGWindowLevelKey::DesktopIconWindowLevelKey) as NSWindowLevel
}

/// Square frame of side `size` centred on `center`.
pub fn square_around(center: NSPoint, size: f64) -> NSRect {
    NSRect::new(
        NSPoint::new(center.x - size / 2.0, center.y - size / 2.0),
        NSSize::new(size, size),
    )
}

/// Centre of `rect`.
pub fn center_of(rect: NSRect) -> NSPoint {
    NSPoint::new(
        rect.origin.x + rect.size.width / 2.0,
        rect.origin.y + rect.size.height / 2.0,
    )
}

/// Moves `frame` (without resizing it) so that it lies inside `bounds`, if it fits.
pub fn clamp_into(mut frame: NSRect, bounds: NSRect) -> NSRect {
    let max_x = bounds.origin.x + bounds.size.width - frame.size.width;
    let max_y = bounds.origin.y + bounds.size.height - frame.size.height;
    frame.origin.x = frame.origin.x.min(max_x).max(bounds.origin.x);
    frame.origin.y = frame.origin.y.min(max_y).max(bounds.origin.y);
    frame
}

/// Initial centre: bottom-right corner of the main screen's visible frame, far enough from
/// the edges for the expanded ring to fit.
fn initial_center(mtm: MainThreadMarker) -> NSPoint {
    let visible = NSScreen::mainScreen(mtm)
        .map(|s| s.visibleFrame())
        .unwrap_or(NSRect::new(NSPoint::ZERO, NSSize::new(1440.0, 900.0)));
    let inset = EXPANDED_SIZE / 2.0 + SCREEN_MARGIN;
    NSPoint::new(
        visible.origin.x + visible.size.width - inset,
        visible.origin.y + inset,
    )
}
