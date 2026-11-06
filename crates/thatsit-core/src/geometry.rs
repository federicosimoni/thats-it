//! Radial layout and point → sector hit-testing.
//!
//! Conventions used throughout this module:
//! - Points are given as offsets `(dx, dy)` from the ring centre, with **y pointing up**
//!   (the AppKit convention for a non-flipped view).
//! - Angles are in **degrees**, measured **clockwise from 12 o'clock** and normalised to
//!   `[0, 360)`. This is how a person reads a dial, which keeps the log output and the
//!   sector numbering intuitive: sector 0 is centred at the top, sector 1 is next clockwise.

/// Maximum number of sectors in one ring (see the project brief, §2).
pub const MAX_SECTORS: usize = 8;

/// Shape of a radial menu ring.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RingLayout {
    /// Number of sectors, `1..=MAX_SECTORS`.
    pub sectors: usize,
    /// Radius of the central dead zone. A drop inside it cancels.
    pub dead_zone_radius: f64,
    /// Outer radius of the ring. Points farther than this are outside.
    pub outer_radius: f64,
}

/// Result of hit-testing a point against a [`RingLayout`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// Inside the central dead zone (drop = cancel).
    DeadZone,
    /// Over the sector with this index (0-based, clockwise from the top).
    Sector(usize),
    /// Beyond the outer radius.
    Outside,
}

impl RingLayout {
    /// Creates a layout, clamping `sectors` to `1..=MAX_SECTORS` and making sure
    /// `dead_zone_radius <= outer_radius`.
    pub fn new(sectors: usize, dead_zone_radius: f64, outer_radius: f64) -> Self {
        let outer_radius = outer_radius.max(0.0);
        Self {
            sectors: sectors.clamp(1, MAX_SECTORS),
            dead_zone_radius: dead_zone_radius.clamp(0.0, outer_radius),
            outer_radius,
        }
    }

    /// Angular width of one sector, in degrees.
    pub fn sector_width(&self) -> f64 {
        360.0 / self.sectors as f64
    }

    /// Start and end angle (degrees, clockwise from 12 o'clock) of sector `index`.
    ///
    /// Sector 0 is centred on 0°, so its start angle is negative
    /// (e.g. `-22.5..22.5` with 8 sectors). `end - start == sector_width()`.
    pub fn sector_span(&self, index: usize) -> (f64, f64) {
        let w = self.sector_width();
        let centre = index as f64 * w;
        (centre - w / 2.0, centre + w / 2.0)
    }

    /// Hit-tests an offset from the ring centre.
    pub fn hit_test(&self, dx: f64, dy: f64) -> Hit {
        let r = dx.hypot(dy);
        if r <= self.dead_zone_radius {
            return Hit::DeadZone;
        }
        if r > self.outer_radius {
            return Hit::Outside;
        }
        Hit::Sector(self.sector_at_angle(angle_deg(dx, dy)))
    }

    /// Index of the sector containing `angle` (any value; normalised internally).
    pub fn sector_at_angle(&self, angle: f64) -> usize {
        let w = self.sector_width();
        // Shift by half a sector so that sector 0 is centred on 0°.
        let shifted = normalize_deg(angle + w / 2.0);
        // `min` guards against floating-point edge cases right below 360°.
        ((shifted / w) as usize).min(self.sectors - 1)
    }
}

/// Angle of an offset `(dx, dy)` (y up), in degrees clockwise from 12 o'clock, in `[0, 360)`.
/// Returns `0.0` for the origin.
pub fn angle_deg(dx: f64, dy: f64) -> f64 {
    if dx == 0.0 && dy == 0.0 {
        return 0.0;
    }
    // atan2(dx, dy) measures from +y towards +x, i.e. clockwise from the top.
    normalize_deg(dx.atan2(dy).to_degrees())
}

/// Normalises an angle in degrees to `[0, 360)`.
pub fn normalize_deg(angle: f64) -> f64 {
    let a = angle.rem_euclid(360.0);
    // rem_euclid can return exactly 360.0 for tiny negative inputs due to rounding.
    if a >= 360.0 { 0.0 } else { a }
}

/// Converts a "clockwise from 12 o'clock" angle to the standard mathematical angle
/// (counter-clockwise from 3 o'clock), in degrees. Useful for drawing APIs such as
/// `NSBezierPath` / Core Graphics arcs.
pub fn to_math_deg(clock_deg: f64) -> f64 {
    normalize_deg(90.0 - clock_deg)
}

/// An axis-aligned rectangle (origin at the bottom-left, like AppKit screen coordinates).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Moves `center` so that a circle of `radius` around it fits inside `bounds`.
/// If the circle is larger than `bounds`, it is centred on `bounds` along that axis.
pub fn clamp_center(center: (f64, f64), radius: f64, bounds: Rect) -> (f64, f64) {
    let axis = |c: f64, lo: f64, len: f64| {
        if 2.0 * radius >= len {
            lo + len / 2.0
        } else {
            c.clamp(lo + radius, lo + len - radius)
        }
    };
    (
        axis(center.0, bounds.x, bounds.w),
        axis(center.1, bounds.y, bounds.h),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    fn ring8() -> RingLayout {
        RingLayout::new(8, 30.0, 150.0)
    }

    #[test]
    fn angle_cardinal_directions() {
        assert!((angle_deg(0.0, 1.0) - 0.0).abs() < EPS); // up
        assert!((angle_deg(1.0, 0.0) - 90.0).abs() < EPS); // right
        assert!((angle_deg(0.0, -1.0) - 180.0).abs() < EPS); // down
        assert!((angle_deg(-1.0, 0.0) - 270.0).abs() < EPS); // left
        assert!((angle_deg(1.0, 1.0) - 45.0).abs() < EPS);
        assert_eq!(angle_deg(0.0, 0.0), 0.0);
    }

    #[test]
    fn normalize_wraps() {
        assert!((normalize_deg(-90.0) - 270.0).abs() < EPS);
        assert!((normalize_deg(720.0 + 10.0) - 10.0).abs() < EPS);
        assert_eq!(normalize_deg(360.0), 0.0);
        let tiny = normalize_deg(-1e-15);
        assert!((0.0..360.0).contains(&tiny));
    }

    #[test]
    fn dead_zone_and_outside() {
        let ring = ring8();
        assert_eq!(ring.hit_test(0.0, 0.0), Hit::DeadZone);
        assert_eq!(ring.hit_test(10.0, 10.0), Hit::DeadZone);
        assert_eq!(ring.hit_test(0.0, 30.0), Hit::DeadZone); // boundary is dead zone
        assert_eq!(ring.hit_test(0.0, 150.0), Hit::Sector(0)); // boundary is inside
        assert_eq!(ring.hit_test(0.0, 151.0), Hit::Outside);
        assert_eq!(ring.hit_test(-200.0, -200.0), Hit::Outside);
    }

    #[test]
    fn eight_sectors_clockwise_from_top() {
        let ring = ring8();
        let r = 100.0;
        for i in 0..8 {
            let clock = i as f64 * 45.0;
            let rad = clock.to_radians();
            // Clockwise from top: dx = sin, dy = cos.
            let hit = ring.hit_test(r * rad.sin(), r * rad.cos());
            assert_eq!(hit, Hit::Sector(i), "centre of sector {i}");
        }
    }

    #[test]
    fn sector_boundaries() {
        let ring = ring8();
        // Sector 0 spans [-22.5, 22.5).
        assert_eq!(ring.sector_at_angle(22.4), 0);
        assert_eq!(ring.sector_at_angle(22.5), 1);
        assert_eq!(ring.sector_at_angle(337.5), 0);
        assert_eq!(ring.sector_at_angle(337.4), 7);
        assert_eq!(ring.sector_at_angle(359.999), 0);
        assert_eq!(ring.sector_at_angle(-10.0), 0);
        assert_eq!(ring.sector_at_angle(1000.0), ring.sector_at_angle(280.0));
    }

    #[test]
    fn sector_span_matches_hit_test() {
        for n in 1..=MAX_SECTORS {
            let ring = RingLayout::new(n, 10.0, 100.0);
            for i in 0..n {
                let (start, end) = ring.sector_span(i);
                assert!((end - start - ring.sector_width()).abs() < EPS);
                let mid = (start + end) / 2.0;
                assert_eq!(ring.sector_at_angle(mid), i, "n={n} i={i}");
                assert_eq!(ring.sector_at_angle(start + 1e-6), i, "n={n} i={i} start");
                assert_eq!(ring.sector_at_angle(end - 1e-6), i, "n={n} i={i} end");
            }
        }
    }

    #[test]
    fn single_sector_covers_everything() {
        let ring = RingLayout::new(1, 0.0, 10.0);
        for a in [0.0, 90.0, 179.0, 270.0, 359.9] {
            assert_eq!(ring.sector_at_angle(a), 0);
        }
    }

    #[test]
    fn new_clamps_inputs() {
        let ring = RingLayout::new(0, 50.0, 20.0);
        assert_eq!(ring.sectors, 1);
        assert_eq!(ring.dead_zone_radius, 20.0);
        assert_eq!(RingLayout::new(99, 0.0, 1.0).sectors, MAX_SECTORS);
    }

    #[test]
    fn math_angle_conversion() {
        assert!((to_math_deg(0.0) - 90.0).abs() < EPS); // top
        assert!((to_math_deg(90.0) - 0.0).abs() < EPS); // right
        assert!((to_math_deg(180.0) - 270.0).abs() < EPS); // bottom
    }
    #[test]
    fn clamps_ring_inside_bounds() {
        let screen = Rect {
            x: 0.0,
            y: 25.0,
            w: 1440.0,
            h: 850.0,
        };
        assert_eq!(clamp_center((700.0, 400.0), 160.0, screen), (700.0, 400.0));
        assert_eq!(clamp_center((10.0, 10.0), 160.0, screen), (160.0, 185.0));
        assert_eq!(
            clamp_center((1439.0, 874.0), 160.0, screen),
            (1280.0, 715.0)
        );
        // Secondary screen to the left (negative coordinates).
        let left = Rect {
            x: -1920.0,
            y: 0.0,
            w: 1920.0,
            h: 1080.0,
        };
        assert_eq!(clamp_center((-5.0, 500.0), 160.0, left), (-160.0, 500.0));
        // Ring bigger than the screen: centred.
        let tiny = Rect {
            x: 0.0,
            y: 0.0,
            w: 200.0,
            h: 1000.0,
        };
        assert_eq!(clamp_center((0.0, 0.0), 160.0, tiny), (100.0, 160.0));
    }
}
