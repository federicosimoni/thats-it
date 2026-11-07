//! Summon mode: during a file drag, if the desktop widget cannot be reached (covered by
//! windows or a full-screen app), a second widget appears next to the pointer, above
//! every window. Dropping on it works exactly like the desktop widget.
//!
//! macOS has no "a drag started" notification for other apps' drags, so a timer samples
//! the drag pasteboard's `changeCount` and the mouse buttons ten times a second;
//! `thatsit_core::summon::DragWatcher` turns the samples into events.

use std::cell::RefCell;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSEvent, NSPasteboard, NSPasteboardNameDrag, NSPasteboardTypeFileURL, NSScreen, NSWindow,
    NSWindowOcclusionState,
};
use objc2_core_foundation::CFArray;
use objc2_core_graphics::{CGWindowListCopyWindowInfo, CGWindowListOption};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSPoint, NSRect, NSString, NSTimer};
use thatsit_core::geometry::Rect;
use thatsit_core::summon::{DragWatcher, Event, summon_center};

use crate::catalog::Catalog;
use crate::ring_view::{centre_of, collapsed_size};
use crate::window::Widget;
use crate::{log, prefs};

/// Sampling interval of the drag pasteboard and the mouse buttons.
const TICK: f64 = 0.1;
/// Distance between the pointer and the summoned widget's centre.
const OFFSET: f64 = 130.0;
/// Radius that must fit on screen around the summoned widget (the open ring + a gap).
const RING_FIT: f64 = 170.0;

fn now() -> Duration {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed()
}

struct State {
    watcher: DragWatcher,
    summoned: Widget,
    desktop: Retained<NSWindow>,
    drag_pasteboard: Retained<NSPasteboard>,
}

/// Owns the timer and the summoned widget. Keep it alive for the app's lifetime.
pub struct Summon {
    _timer: Retained<NSTimer>,
    _state: Rc<RefCell<State>>,
}

impl Summon {
    pub fn install(
        mtm: MainThreadMarker,
        catalog: Rc<Catalog>,
        desktop: Retained<NSWindow>,
    ) -> Self {
        // SAFETY: `NSPasteboardNameDrag` is an immutable NSString constant exported by AppKit.
        let name = unsafe { NSPasteboardNameDrag };
        let state = Rc::new(RefCell::new(State {
            watcher: DragWatcher::new(),
            summoned: Widget::new_summon(mtm, catalog),
            desktop,
            drag_pasteboard: NSPasteboard::pasteboardWithName(name),
        }));
        let for_timer = Rc::clone(&state);
        let block = RcBlock::new(move |_timer: NonNull<NSTimer>| {
            // Skip a tick rather than panic if a previous tick is still running.
            if let Ok(mut state) = for_timer.try_borrow_mut() {
                tick(&mut state, mtm);
            }
        });
        // SAFETY: the block only touches main-thread state, and scheduled timers fire on
        // the run loop of the thread that created them (the main thread).
        let timer =
            unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(TICK, true, &block) };
        Self {
            _timer: timer,
            _state: state,
        }
    }
}

fn tick(state: &mut State, mtm: MainThreadMarker) {
    let mouse_down = NSEvent::pressedMouseButtons() & 1 == 1;
    let change_count = state.drag_pasteboard.changeCount();
    match state.watcher.sample(change_count, mouse_down, now()) {
        Event::Nothing => {}
        Event::DragStarted => {
            if prefs::summon_enabled() && !prefs::widget_hidden() && has_files(state) {
                state.watcher.track(now());
            }
        }
        Event::ShowDue => {
            if desktop_widget_covered(&state.desktop) {
                show(state, mtm);
            }
        }
        Event::DragEnded => {
            // Wait until a drop on the summoned widget has been handled.
            if !state.summoned.view().in_session() {
                state.summoned.window().orderOut(None);
                state.watcher.hidden();
            }
        }
    }
}

fn has_files(state: &State) -> bool {
    // SAFETY: `NSPasteboardTypeFileURL` is an immutable NSString constant exported by AppKit.
    let file_url = unsafe { NSPasteboardTypeFileURL };
    state
        .drag_pasteboard
        .types()
        .is_some_and(|types| types.iter().any(|t| &*t == file_url))
}

fn show(state: &mut State, mtm: MainThreadMarker) {
    let pointer = NSEvent::mouseLocation();
    let screens = NSScreen::screens(mtm);
    let Some(screen) = screens
        .iter()
        .find(|s| contains(s.frame(), pointer))
        .or_else(|| NSScreen::mainScreen(mtm))
    else {
        return;
    };
    let vf = screen.visibleFrame();
    let bounds = Rect {
        x: vf.origin.x,
        y: vf.origin.y,
        w: vf.size.width,
        h: vf.size.height,
    };
    let (x, y) = summon_center((pointer.x, pointer.y), bounds, RING_FIT, OFFSET);
    log::info(format!("summon mode: widget at ({x:.0}, {y:.0})"));
    state.summoned.view().summon_at(NSPoint::new(x, y));
    state.summoned.window().orderFrontRegardless();
    state.watcher.shown();
}

fn contains(r: NSRect, p: NSPoint) -> bool {
    p.x >= r.origin.x
        && p.x < r.origin.x + r.size.width
        && p.y >= r.origin.y
        && p.y < r.origin.y + r.size.height
}

/// Whether the desktop widget cannot be reached: not visible at all (a full-screen app,
/// another Space), or a normal window overlaps its disc.
fn desktop_widget_covered(desktop: &NSWindow) -> bool {
    if !desktop.isVisible() {
        return false;
    }
    if !desktop
        .occlusionState()
        .contains(NSWindowOcclusionState::Visible)
    {
        return true;
    }
    let disc = {
        let c = centre_of(desktop.frame());
        let r = collapsed_size() / 2.0;
        (c.x - r, c.y - r, 2.0 * r)
    };
    let Some(primary_height) = NSScreen::screens(desktop.mtm())
        .firstObject()
        .map(|s| s.frame().size.height)
    else {
        return false;
    };
    let Ok(window_id) = u32::try_from(desktop.windowNumber()) else {
        return false;
    };
    let Some(list) = CGWindowListCopyWindowInfo(
        CGWindowListOption::OptionOnScreenAboveWindow | CGWindowListOption::ExcludeDesktopElements,
        window_id,
    ) else {
        return false;
    };
    // SAFETY: CFArray and NSArray are toll-free bridged, and CGWindowListCopyWindowInfo
    // returns an array of dictionaries with string keys; `list` outlives the reference.
    let windows: &NSArray<NSDictionary<NSString, AnyObject>> =
        unsafe { &*(&*list as *const CFArray).cast() };
    let number = |dict: &NSDictionary<NSString, AnyObject>, key: &str| -> Option<f64> {
        dict.objectForKey(&NSString::from_str(key))?
            .downcast::<NSNumber>()
            .ok()
            .map(|n| n.doubleValue())
    };
    let own_pid = f64::from(std::process::id());
    windows.iter().any(|w| {
        // Only normal app windows (layer 0) that are not ours and not invisible.
        if number(&w, "kCGWindowLayer") != Some(0.0)
            || number(&w, "kCGWindowOwnerPID") == Some(own_pid)
            || number(&w, "kCGWindowAlpha").is_some_and(|a| a <= 0.0)
        {
            return false;
        }
        let Some(bounds) = w
            .objectForKey(&NSString::from_str("kCGWindowBounds"))
            .and_then(|b| b.downcast::<NSDictionary>().ok())
        else {
            return false;
        };
        // SAFETY: the bounds dictionary has string keys and number values.
        let bounds: Retained<NSDictionary<NSString, AnyObject>> =
            unsafe { Retained::cast_unchecked(bounds) };
        let (Some(x), Some(y), Some(width), Some(height)) = (
            number(&bounds, "X"),
            number(&bounds, "Y"),
            number(&bounds, "Width"),
            number(&bounds, "Height"),
        ) else {
            return false;
        };
        // Core Graphics measures from the top of the primary screen; AppKit from the bottom.
        let bottom = primary_height - (y + height);
        let (dx, dy, side) = disc;
        x < dx + side && x + width > dx && bottom < dy + side && bottom + height > dy
    })
}
