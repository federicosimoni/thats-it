//! The custom view that receives file drags and draws the (spike) ring.
//!
//! Phase 0 uses plain `drawRect:` with `NSBezierPath`; the layer-based ring with
//! animations replaces it in Phase 2.

use std::cell::Cell;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSBezierPath, NSColor, NSDragOperation, NSDraggingDestination, NSDraggingInfo,
    NSPasteboardTypeFileURL, NSScreen, NSView,
};
use objc2_foundation::{NSArray, NSObjectProtocol, NSPoint, NSRect};
use thatsit_core::geometry::{Hit, RingLayout};

use crate::window::{COLLAPSED_SIZE, EXPANDED_SIZE, center_of, clamp_into, square_around};
use crate::{files, log};

/// Number of placeholder sectors shown by the spike.
const SPIKE_SECTORS: usize = 8;
/// Radius of the central dead zone, in points.
const DEAD_ZONE_RADIUS: f64 = 36.0;
/// Gap between the ring and the window edge, in points.
const RING_MARGIN: f64 = 8.0;

#[derive(Debug, Default)]
pub struct DropViewIvars {
    /// Whether the window is currently in its expanded (ring) size.
    expanded: Cell<bool>,
    /// The last hit-test result during the current drag (for highlight + log de-dup).
    hit: Cell<Option<Hit>>,
}

define_class!(
    // SAFETY:
    // - NSView has no special subclassing requirements beyond being used on the main thread,
    //   which `thread_kind = MainThreadOnly` enforces.
    // - `DropView` does not implement `Drop`.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = DropViewIvars]
    pub struct DropView;

    impl DropView {
        // SAFETY: the signature matches `-[NSView drawRect:]`.
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            if self.ivars().expanded.get() {
                self.draw_ring();
            } else {
                self.draw_collapsed();
            }
        }

        // Let clicks on an inactive (accessory) app's window be handled immediately,
        // so the circle can be dragged around without first activating the app.
        // SAFETY: the signature matches `-[NSView acceptsFirstMouse:]`.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&objc2_app_kit::NSEvent>) -> bool {
            true
        }
    }

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for DropView {}

    // SAFETY: every method below has the exact signature declared by the
    // `NSDraggingDestination` protocol.
    unsafe impl NSDraggingDestination for DropView {
        #[unsafe(method(draggingEntered:))]
        fn dragging_entered(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            let dragged = files::read_files(&sender.draggingPasteboard());
            if dragged.is_empty() {
                log::info("drag entered with no file URLs → not supported");
                return NSDragOperation::None;
            }
            log::info(format!("drag entered with {} file(s):", dragged.len()));
            for f in &dragged {
                log::info(format!(
                    "    {}  [{}]",
                    f.path,
                    f.uti.as_deref().unwrap_or("unknown UTType")
                ));
            }
            self.ivars().hit.set(None);
            self.set_expanded(true);
            self.track(sender);
            NSDragOperation::Copy
        }

        #[unsafe(method(draggingUpdated:))]
        fn dragging_updated(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            if !self.ivars().expanded.get() {
                return NSDragOperation::None;
            }
            self.track(sender);
            NSDragOperation::Copy
        }

        #[unsafe(method(draggingExited:))]
        fn dragging_exited(&self, _sender: Option<&ProtocolObject<dyn NSDraggingInfo>>) {
            if self.ivars().expanded.get() {
                log::info("drag exited → collapse");
                self.set_expanded(false);
            }
        }

        #[unsafe(method(prepareForDragOperation:))]
        fn prepare_for_drag_operation(&self, _sender: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
            self.ivars().expanded.get()
        }

        #[unsafe(method(performDragOperation:))]
        fn perform_drag_operation(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
            let hit = self.hit_at(sender);
            let accepted = match hit {
                Hit::Sector(i) => {
                    log::info(format!("DROP on sector {i} → (Phase 0: nothing to run)"));
                    true
                }
                Hit::DeadZone => {
                    log::info("DROP in dead zone → cancelled");
                    false
                }
                Hit::Outside => {
                    log::info("DROP outside the ring → cancelled");
                    false
                }
            };
            self.set_expanded(false);
            accepted
        }
    }
);

impl DropView {
    pub fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DropViewIvars::default());
        // SAFETY: `initWithFrame:` is NSView's designated initializer and takes an NSRect.
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        // SAFETY: `NSPasteboardTypeFileURL` is an immutable NSString constant exported by AppKit.
        let file_url_type = unsafe { NSPasteboardTypeFileURL };
        view.registerForDraggedTypes(&NSArray::from_slice(&[file_url_type]));
        view
    }

    /// The ring layout for the current view size.
    fn layout(&self) -> RingLayout {
        let b = self.bounds();
        let outer = b.size.width.min(b.size.height) / 2.0 - RING_MARGIN;
        RingLayout::new(SPIKE_SECTORS, DEAD_ZONE_RADIUS, outer)
    }

    /// Hit-tests the current dragging location.
    fn hit_at(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> Hit {
        // `draggingLocation` is in the window's base coordinate system.
        let p = self.convertPoint_fromView(sender.draggingLocation(), None);
        let c = center_of(self.bounds());
        self.layout().hit_test(p.x - c.x, p.y - c.y)
    }

    /// Updates the highlighted sector and logs angle + sector when the sector changes.
    fn track(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) {
        let hit = self.hit_at(sender);
        if self.ivars().hit.get() == Some(hit) {
            return;
        }
        self.ivars().hit.set(Some(hit));

        let p = self.convertPoint_fromView(sender.draggingLocation(), None);
        let c = center_of(self.bounds());
        let (dx, dy) = (p.x - c.x, p.y - c.y);
        let angle = thatsit_core::geometry::angle_deg(dx, dy);
        let radius = dx.hypot(dy);
        let what = match hit {
            Hit::Sector(i) => format!("sector {i}"),
            Hit::DeadZone => "dead zone".to_owned(),
            Hit::Outside => "outside".to_owned(),
        };
        log::info(format!(
            "cursor angle {angle:6.1}°  radius {radius:5.1} pt  → {what}"
        ));
        self.setNeedsDisplay(true);
    }

    /// Grows (centred) or shrinks the window.
    ///
    /// When growing, the expanded frame is clamped into the screen's visible frame so the
    /// whole ring is always on screen; the collapsed widget then stays centred on the
    /// clamped position.
    fn set_expanded(&self, expanded: bool) {
        if self.ivars().expanded.get() == expanded {
            return;
        }
        let Some(window) = self.window() else {
            return;
        };
        let center = center_of(window.frame());
        let frame = if expanded {
            let target = square_around(center, EXPANDED_SIZE);
            let screen = window.screen().or_else(|| NSScreen::mainScreen(self.mtm()));
            match screen {
                Some(s) => clamp_into(target, s.visibleFrame()),
                None => target,
            }
        } else {
            square_around(center, COLLAPSED_SIZE)
        };
        self.ivars().expanded.set(expanded);
        if !expanded {
            self.ivars().hit.set(None);
        }
        window.setFrame_display(frame, true);
        self.setNeedsDisplay(true);
    }

    fn draw_collapsed(&self) {
        let b = self.bounds();
        let inset = 4.0;
        let rect = NSRect::new(
            NSPoint::new(b.origin.x + inset, b.origin.y + inset),
            objc2_foundation::NSSize::new(b.size.width - 2.0 * inset, b.size.height - 2.0 * inset),
        );
        let circle = NSBezierPath::bezierPathWithOvalInRect(rect);
        NSColor::controlAccentColor()
            .colorWithAlphaComponent(0.85)
            .setFill();
        circle.fill();
        NSColor::colorWithWhite_alpha(1.0, 0.9).setStroke();
        circle.setLineWidth(2.0);
        circle.stroke();

        // A small inner dot hints at the drop target.
        let c = center_of(b);
        let dot = NSBezierPath::bezierPathWithOvalInRect(square_around(c, 14.0));
        NSColor::colorWithWhite_alpha(1.0, 0.9).setFill();
        dot.fill();
    }

    fn draw_ring(&self) {
        let layout = self.layout();
        let c = center_of(self.bounds());
        let highlighted = match self.ivars().hit.get() {
            Some(Hit::Sector(i)) => Some(i),
            _ => None,
        };

        for i in 0..layout.sectors {
            let (start, end) = layout.sector_span(i);
            // Clock angles (clockwise from 12) → math angles (counter-clockwise from 3).
            // Use the un-normalised form so `math_end - math_start == sector width`.
            let math_start = 90.0 - end;
            let math_end = 90.0 - start;
            let path = NSBezierPath::bezierPath();
            path.appendBezierPathWithArcWithCenter_radius_startAngle_endAngle_clockwise(
                c,
                layout.outer_radius,
                math_start,
                math_end,
                false,
            );
            path.appendBezierPathWithArcWithCenter_radius_startAngle_endAngle_clockwise(
                c,
                layout.dead_zone_radius,
                math_end,
                math_start,
                true,
            );
            path.closePath();

            let fill = if highlighted == Some(i) {
                NSColor::controlAccentColor().colorWithAlphaComponent(0.9)
            } else {
                NSColor::windowBackgroundColor().colorWithAlphaComponent(0.85)
            };
            fill.setFill();
            path.fill();
            NSColor::separatorColor().setStroke();
            path.setLineWidth(1.0);
            path.stroke();
        }

        // Dead zone.
        let dead =
            NSBezierPath::bezierPathWithOvalInRect(square_around(c, layout.dead_zone_radius * 2.0));
        let dead_fill = if self.ivars().hit.get() == Some(Hit::DeadZone) {
            NSColor::labelColor().colorWithAlphaComponent(0.25)
        } else {
            NSColor::labelColor().colorWithAlphaComponent(0.1)
        };
        dead_fill.setFill();
        dead.fill();
    }
}
