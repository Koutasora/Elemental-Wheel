//! Wheel geometry: the inner ring has 12 sectors, the outer ring 4..=12. Sector 0 is at the top
//! (its middle is 0 degrees), angle grows clockwise. Coordinates are screen coordinates relative
//! to the wheel center: x to the right, y down.

/// Number of sectors in the inner ring.
pub const INNER_SECTORS: u8 = 12;
/// Default number of sectors in the outer ring.
pub const DEFAULT_OUTER_SECTORS: u8 = 12;
/// Smallest allowed number of outer ring sectors.
pub const MIN_OUTER_SECTORS: u8 = 4;
/// Largest allowed number of outer ring sectors.
pub const MAX_OUTER_SECTORS: u8 = 12;

/// Clamps the outer ring sector count to `MIN_OUTER_SECTORS..=MAX_OUTER_SECTORS`.
pub fn clamp_outer_sectors(n: u8) -> u8 {
    n.clamp(MIN_OUTER_SECTORS, MAX_OUTER_SECTORS)
}

/// Ring boundaries, as distances from the wheel center.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Radii {
    /// Below this distance the point is in the center.
    pub center: f32,
    /// Below this distance (and not in the center) the point is in the inner ring.
    pub inner: f32,
}

/// Result of a hit test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Center,
    Inner(u8),
    Outer(u8),
}

/// Angle of the point in degrees, in `0..360`: 0 is up, clockwise.
pub fn angle_deg(x: f32, y: f32) -> f32 {
    let a = x.atan2(-y).to_degrees();
    let a = if a < 0.0 { a + 360.0 } else { a };
    // Tiny negative angles can round up to exactly 360.
    if a >= 360.0 { 0.0 } else { a }
}

/// Sector index (`0..sectors`) for an angle in degrees; sector 0 covers `-w/2..w/2`
/// where `w = 360 / sectors`. `sectors` of 0 is treated as 1.
pub fn sector_at(angle_deg: f32, sectors: u8) -> u8 {
    let sectors = sectors.max(1);
    let width = 360.0 / sectors as f32;
    let a = (angle_deg + width / 2.0).rem_euclid(360.0);
    // rem_euclid can round up to exactly 360, hence the min.
    ((a / width) as u8).min(sectors - 1)
}

/// Finds what is under the point. The inner ring always has `INNER_SECTORS` sectors, the outer
/// ring `outer_sectors`; outside the wheel the outer ring is chosen by direction.
pub fn hit_test(x: f32, y: f32, r: &Radii, outer_sectors: u8) -> Hit {
    let dist = x.hypot(y);
    if dist < r.center {
        return Hit::Center;
    }
    let angle = angle_deg(x, y);
    if dist < r.inner {
        Hit::Inner(sector_at(angle, INNER_SECTORS))
    } else {
        Hit::Outer(sector_at(angle, outer_sectors))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Radii = Radii {
        center: 40.0,
        inner: 120.0,
    };

    /// Point at `dist` from the center in the direction `deg` (0 = up, clockwise).
    fn point(deg: f32, dist: f32) -> (f32, f32) {
        let rad = deg.to_radians();
        (dist * rad.sin(), -dist * rad.cos())
    }

    #[test]
    fn center_hits() {
        assert_eq!(hit_test(0.0, 0.0, &R, 12), Hit::Center);
        assert_eq!(hit_test(10.0, -10.0, &R, 12), Hit::Center);
    }

    #[test]
    fn every_outer_sector_near_and_far() {
        for n in [4u8, 6, 8, 12] {
            let w = 360.0 / n as f32;
            for i in 0..n {
                let (x, y) = point(i as f32 * w, 200.0);
                assert_eq!(hit_test(x, y, &R, n), Hit::Outer(i), "n={n} i={i}");
                let (x, y) = point(i as f32 * w, 10_000.0);
                assert_eq!(hit_test(x, y, &R, n), Hit::Outer(i), "far n={n} i={i}");
            }
        }
    }

    #[test]
    fn inner_ring_always_twelve() {
        for n in [4u8, 6, 8, 12] {
            for i in 0..INNER_SECTORS {
                let (x, y) = point(i as f32 * 30.0, 80.0);
                assert_eq!(hit_test(x, y, &R, n), Hit::Inner(i), "n={n} i={i}");
            }
        }
    }

    #[test]
    fn sector_boundaries_12() {
        assert_eq!(sector_at(15.0, 12), 1);
        assert_eq!(sector_at(14.9, 12), 0);
        assert_eq!(sector_at(345.0, 12), 0);
        assert_eq!(sector_at(344.9, 12), 11);
    }

    #[test]
    fn sector_boundaries_4() {
        assert_eq!(sector_at(45.0, 4), 1);
        assert_eq!(sector_at(44.9, 4), 0);
        assert_eq!(sector_at(315.0, 4), 0);
        assert_eq!(sector_at(314.9, 4), 3);
        assert_eq!(sector_at(180.0, 4), 2);
    }

    #[test]
    fn sector_at_handles_wraparound() {
        assert_eq!(sector_at(-0.001, 12), 0);
        assert_eq!(sector_at(359.999, 4), 0);
        assert_eq!(sector_at(-1e-9, 7), 0);
    }

    #[test]
    fn ring_boundaries() {
        let eps = 0.01;
        assert_eq!(hit_test(R.center, 0.0, &R, 12), Hit::Inner(3));
        assert_eq!(hit_test(R.center - eps, 0.0, &R, 12), Hit::Center);
        assert_eq!(hit_test(R.inner, 0.0, &R, 12), Hit::Outer(3));
        assert_eq!(hit_test(R.inner - eps, 0.0, &R, 12), Hit::Inner(3));
    }

    #[test]
    fn clamps_outer_sectors() {
        for (input, want) in [(0, 4), (3, 4), (4, 4), (12, 12), (13, 12), (255, 12)] {
            assert_eq!(clamp_outer_sectors(input), want, "input={input}");
        }
    }

    #[test]
    fn cardinal_directions() {
        assert_eq!(sector_at(angle_deg(0.0, -1.0), 12), 0);
        assert_eq!(sector_at(angle_deg(1.0, 0.0), 12), 3);
        assert_eq!(sector_at(angle_deg(0.0, 1.0), 12), 6);
        assert_eq!(sector_at(angle_deg(-1.0, 0.0), 12), 9);
    }

    #[test]
    fn degenerate_inputs() {
        assert_eq!(sector_at(123.0, 0), 0);
        assert_eq!(sector_at(123.0, 1), 0);
        for n in [4, 12] {
            assert_eq!(sector_at(360.0, n), 0);
            assert_eq!(sector_at(-0.0, n), 0);
            assert_eq!(sector_at(359.9999, n), 0);
        }
    }
}
