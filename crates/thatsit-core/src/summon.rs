//! Summon mode: while a file drag is in progress and the desktop widget cannot be reached
//! (covered by windows or a full-screen app), a second widget appears near the pointer.
//!
//! macOS does not tell an app that a drag started elsewhere, so the app polls the drag
//! pasteboard's `changeCount` and the mouse button state; [`DragWatcher`] turns those
//! samples into events. [`summon_center`] decides where the widget appears.

use std::time::Duration;

use crate::geometry::{Rect, clamp_center};

/// How long a file drag must last before the summoned widget appears.
pub const SUMMON_DELAY: Duration = Duration::from_millis(500);

/// What the app should do after a sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Nothing,
    /// A new drag started: check whether it carries files, then call
    /// [`DragWatcher::track`] (or ignore it).
    DragStarted,
    /// The tracked drag has lasted [`SUMMON_DELAY`]: show the widget if the desktop is
    /// covered, then call [`DragWatcher::shown`].
    ShowDue,
    /// The mouse button was released: hide the summoned widget (once it is not in the
    /// middle of handling a drop), then call [`DragWatcher::hidden`].
    DragEnded,
}

/// Turns periodic samples of the drag pasteboard and mouse into drag events.
#[derive(Debug, Clone, Default)]
pub struct DragWatcher {
    change_count: Option<isize>,
    /// Start time of the file drag being tracked.
    tracking: Option<Duration>,
    /// `ShowDue` was already reported for the tracked drag.
    due_reported: bool,
    /// The summoned widget is on screen.
    visible: bool,
}

impl DragWatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one sample: the drag pasteboard's change count, whether the left mouse
    /// button is down, and the current time.
    pub fn sample(&mut self, change_count: isize, mouse_down: bool, now: Duration) -> Event {
        let previous = self.change_count.replace(change_count);
        if !mouse_down {
            self.tracking = None;
            self.due_reported = false;
            return if self.visible {
                Event::DragEnded
            } else {
                Event::Nothing
            };
        }
        if previous.is_some_and(|p| p != change_count) {
            self.tracking = None;
            self.due_reported = false;
            return Event::DragStarted;
        }
        match self.tracking {
            Some(start) if !self.due_reported && now.saturating_sub(start) >= SUMMON_DELAY => {
                self.due_reported = true;
                Event::ShowDue
            }
            _ => Event::Nothing,
        }
    }

    /// The drag reported by `DragStarted` carries files: track it from `now`.
    pub fn track(&mut self, now: Duration) {
        self.tracking = Some(now);
        self.due_reported = false;
    }

    /// The summoned widget was shown.
    pub fn shown(&mut self) {
        self.visible = true;
    }

    /// The summoned widget was hidden.
    pub fn hidden(&mut self) {
        self.visible = false;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }
}

/// Centre of the summoned widget: `offset` to the right of the pointer (to the left when
/// there is no room), placed so that a ring of `radius` around it fits inside `bounds`.
pub fn summon_center(pointer: (f64, f64), bounds: Rect, radius: f64, offset: f64) -> (f64, f64) {
    let right = pointer.0 + offset;
    let x = if right + radius <= bounds.x + bounds.w {
        right
    } else {
        pointer.0 - offset
    };
    clamp_center((x, pointer.1), radius, bounds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn file_drag_shows_after_the_delay_and_hides_on_release() {
        let mut w = DragWatcher::new();
        assert_eq!(w.sample(5, false, ms(0)), Event::Nothing);
        // A drag starts: the pasteboard changes while the button is down.
        assert_eq!(w.sample(6, true, ms(100)), Event::DragStarted);
        w.track(ms(100));
        assert_eq!(w.sample(6, true, ms(400)), Event::Nothing);
        assert_eq!(w.sample(6, true, ms(600)), Event::ShowDue);
        w.shown();
        assert_eq!(w.sample(6, true, ms(700)), Event::Nothing); // reported once
        assert_eq!(w.sample(6, false, ms(900)), Event::DragEnded);
        // Still visible (e.g. a drop is being handled): reported again until hidden.
        assert_eq!(w.sample(6, false, ms(1000)), Event::DragEnded);
        w.hidden();
        assert_eq!(w.sample(6, false, ms(1100)), Event::Nothing);
    }

    #[test]
    fn untracked_drags_and_clicks_do_nothing() {
        let mut w = DragWatcher::new();
        w.sample(1, false, ms(0));
        // A text drag: reported, but the app does not track it.
        assert_eq!(w.sample(2, true, ms(100)), Event::DragStarted);
        assert_eq!(w.sample(2, true, ms(900)), Event::Nothing);
        assert_eq!(w.sample(2, false, ms(1000)), Event::Nothing);
        // A plain click (no pasteboard change) does nothing.
        assert_eq!(w.sample(2, true, ms(1100)), Event::Nothing);
        assert_eq!(w.sample(2, true, ms(2000)), Event::Nothing);
        // The first sample never counts as a new drag.
        let mut fresh = DragWatcher::new();
        assert_eq!(fresh.sample(9, true, ms(0)), Event::Nothing);
    }

    #[test]
    fn short_drags_never_show() {
        let mut w = DragWatcher::new();
        w.sample(1, false, ms(0));
        assert_eq!(w.sample(2, true, ms(50)), Event::DragStarted);
        w.track(ms(50));
        assert_eq!(w.sample(2, true, ms(300)), Event::Nothing);
        assert_eq!(w.sample(2, false, ms(400)), Event::Nothing);
        assert!(!w.is_visible());
    }

    #[test]
    fn a_new_drag_restarts_the_delay() {
        let mut w = DragWatcher::new();
        w.sample(1, false, ms(0));
        w.sample(2, true, ms(0));
        w.track(ms(0));
        assert_eq!(w.sample(3, true, ms(400)), Event::DragStarted);
        w.track(ms(400));
        assert_eq!(w.sample(3, true, ms(600)), Event::Nothing);
        assert_eq!(w.sample(3, true, ms(900)), Event::ShowDue);
    }

    #[test]
    fn placed_beside_the_pointer_and_inside_the_screen() {
        let screen = Rect {
            x: 0.0,
            y: 0.0,
            w: 1440.0,
            h: 900.0,
        };
        // Room on the right.
        assert_eq!(
            summon_center((400.0, 450.0), screen, 170.0, 130.0),
            (530.0, 450.0)
        );
        // Near the right edge: goes to the left.
        assert_eq!(
            summon_center((1400.0, 450.0), screen, 170.0, 130.0),
            (1270.0, 450.0)
        );
        // Near the top: pushed down so the ring fits.
        assert_eq!(
            summon_center((400.0, 890.0), screen, 170.0, 130.0),
            (530.0, 730.0)
        );
        // Near the left edge: clamped.
        assert_eq!(
            summon_center((10.0, 450.0), screen, 170.0, 130.0),
            (170.0, 450.0)
        );
    }
}
