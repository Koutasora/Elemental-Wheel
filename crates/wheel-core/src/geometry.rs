//! Wheel geometry: 12 sectors of 30 degrees, sector 0 at the top (its middle is 0 degrees),
//! angle grows clockwise. Coordinates are screen coordinates relative to the wheel center:
//! x to the right, y down.

/// Number of sectors in each ring.
pub const SECTORS: u8 = 12;

const SECTOR_DEG: f32 = 360.0 / SECTORS as f32;

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

/// Sector index (`0..12`) for an angle in degrees; sector 0 covers -15..15.
pub fn sector_at(angle_deg: f32) -> u8 {
    let a = (angle_deg + SECTOR_DEG / 2.0).rem_euclid(360.0);
    ((a / SECTOR_DEG) as u8).min(SECTORS - 1)
}

/// Finds what is under the point. Outside the wheel the outer ring is chosen by direction.
pub fn hit_test(x: f32, y: f32, r: &Radii) -> Hit {
    let dist = x.hypot(y);
    if dist < r.center {
        return Hit::Center;
    }
    let sector = sector_at(angle_deg(x, y));
    if dist < r.inner {
        Hit::Inner(sector)
    } else {
        Hit::Outer(sector)
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
        assert_eq!(hit_test(0.0, 0.0, &R), Hit::Center);
        assert_eq!(hit_test(10.0, -10.0, &R), Hit::Center);
    }

    #[test]
    fn every_sector_in_both_rings() {
        for i in 0..SECTORS {
            let deg = i as f32 * 30.0;
            let (x, y) = point(deg, 80.0);
            assert_eq!(hit_test(x, y, &R), Hit::Inner(i));
            let (x, y) = point(deg, 200.0);
            assert_eq!(hit_test(x, y, &R), Hit::Outer(i));
        }
    }

    #[test]
    fn far_outside_uses_direction() {
        for i in 0..SECTORS {
            let (x, y) = point(i as f32 * 30.0, 10_000.0);
            assert_eq!(hit_test(x, y, &R), Hit::Outer(i));
        }
    }

    #[test]
    fn sector_boundaries() {
        assert_eq!(sector_at(15.0), 1);
        assert_eq!(sector_at(14.9), 0);
        assert_eq!(sector_at(345.0), 0);
        assert_eq!(sector_at(344.9), 11);
    }

    #[test]
    fn cardinal_directions() {
        assert_eq!(sector_at(angle_deg(0.0, -1.0)), 0);
        assert_eq!(sector_at(angle_deg(1.0, 0.0)), 3);
        assert_eq!(sector_at(angle_deg(0.0, 1.0)), 6);
        assert_eq!(sector_at(angle_deg(-1.0, 0.0)), 9);
    }
}
