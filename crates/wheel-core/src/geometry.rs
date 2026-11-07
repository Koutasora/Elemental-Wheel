//! Wheel geometry: the inner and the outer ring each have 4..=12 sectors. Sector 0 is at the top
//! (its middle is 0 degrees), angle grows clockwise. Coordinates are screen coordinates relative
//! to the wheel center: x to the right, y down.

/// Default number of sectors in the inner ring.
pub const DEFAULT_INNER_SECTORS: u8 = 12;
/// Smallest allowed number of inner ring sectors.
pub const MIN_INNER_SECTORS: u8 = 4;
/// Largest allowed number of inner ring sectors (and of inner items kept in the config).
pub const MAX_INNER_SECTORS: u8 = 12;
/// Default number of sectors in the outer ring.
pub const DEFAULT_OUTER_SECTORS: u8 = 12;
/// Smallest allowed number of outer ring sectors.
pub const MIN_OUTER_SECTORS: u8 = 4;
/// Largest allowed number of outer ring sectors.
pub const MAX_OUTER_SECTORS: u8 = 12;

/// Angular width of one submenu tile, in degrees.
pub const SUB_TILE_DEG: f32 = 26.0;
/// Largest number of tiles in a submenu arc.
pub const MAX_SUB_TILES: u8 = 5;

/// Clamps the inner ring sector count to `MIN_INNER_SECTORS..=MAX_INNER_SECTORS`.
pub fn clamp_inner_sectors(n: u8) -> u8 {
    n.clamp(MIN_INNER_SECTORS, MAX_INNER_SECTORS)
}

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
    /// Below this distance (and not in the inner ring) the point is in the outer ring. Beyond it
    /// an open submenu arc can be hit; it sits in the middle of the gap before the arc band.
    pub outer: f32,
}

/// Result of a hit test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Center,
    Inner(u8),
    Outer(u8),
    /// A tile of an open submenu arc: the outer sector of the parent and the index in its submenu.
    Sub(u8, u8),
}

/// An open submenu arc, centred on the outer sector `parent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubArc {
    /// Outer sector the arc belongs to.
    pub parent: u8,
    /// Number of tiles, `1..=MAX_SUB_TILES`.
    pub count: u8,
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

/// Start and end angle, in degrees, of tile `k` of the arc `arc` for `outer_sectors` outer
/// sectors. The arc is `count * SUB_TILE_DEG` wide and centred on the middle of its parent sector.
pub fn sub_tile_span(arc: SubArc, k: u8, outer_sectors: u8) -> (f32, f32) {
    let middle = arc.parent as f32 * 360.0 / outer_sectors.max(1) as f32;
    let start = middle - arc.count as f32 * SUB_TILE_DEG / 2.0 + k as f32 * SUB_TILE_DEG;
    (start, start + SUB_TILE_DEG)
}

/// Finds what is under the point with no submenu arc open; see `hit_test_arc`.
pub fn hit_test(x: f32, y: f32, r: &Radii, inner_sectors: u8, outer_sectors: u8) -> Hit {
    hit_test_arc(x, y, r, inner_sectors, outer_sectors, None)
}

/// Finds what is under the point. The inner ring has `inner_sectors` sectors, the outer
/// ring `outer_sectors`. Beyond `r.outer`, a point within the angular range of the open `arc`
/// is a `Sub` tile at any distance (so releasing further out still picks the tile); any other
/// point is the outer sector in its direction. An arc with a parent outside the outer ring or
/// a count outside `1..=MAX_SUB_TILES` is ignored.
pub fn hit_test_arc(
    x: f32,
    y: f32,
    r: &Radii,
    inner_sectors: u8,
    outer_sectors: u8,
    arc: Option<SubArc>,
) -> Hit {
    let dist = x.hypot(y);
    if dist < r.center {
        return Hit::Center;
    }
    let angle = angle_deg(x, y);
    if dist < r.inner {
        return Hit::Inner(sector_at(angle, inner_sectors));
    }
    if dist >= r.outer
        && let Some(arc) =
            arc.filter(|a| a.parent < outer_sectors && (1..=MAX_SUB_TILES).contains(&a.count))
    {
        let middle = arc.parent as f32 * 360.0 / outer_sectors as f32;
        let delta = (angle - middle + 180.0).rem_euclid(360.0) - 180.0;
        let half = arc.count as f32 * SUB_TILE_DEG / 2.0;
        if delta.abs() <= half {
            let k = (((delta + half) / SUB_TILE_DEG) as u8).min(arc.count - 1);
            return Hit::Sub(arc.parent, k);
        }
    }
    Hit::Outer(sector_at(angle, outer_sectors))
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Radii = Radii {
        center: 40.0,
        inner: 120.0,
        outer: 214.0,
    };

    /// Point at `dist` from the center in the direction `deg` (0 = up, clockwise).
    fn point(deg: f32, dist: f32) -> (f32, f32) {
        let rad = deg.to_radians();
        (dist * rad.sin(), -dist * rad.cos())
    }

    #[test]
    fn center_hits() {
        assert_eq!(hit_test(0.0, 0.0, &R, 12, 12), Hit::Center);
        assert_eq!(hit_test(10.0, -10.0, &R, 12, 12), Hit::Center);
    }

    #[test]
    fn every_outer_sector_near_and_far() {
        for n in [4u8, 6, 8, 12] {
            let w = 360.0 / n as f32;
            for i in 0..n {
                let (x, y) = point(i as f32 * w, 200.0);
                assert_eq!(hit_test(x, y, &R, 12, n), Hit::Outer(i), "n={n} i={i}");
                let (x, y) = point(i as f32 * w, 10_000.0);
                assert_eq!(hit_test(x, y, &R, 12, n), Hit::Outer(i), "far n={n} i={i}");
            }
        }
    }

    #[test]
    fn every_inner_sector_for_each_count() {
        for n in [4u8, 6, 12] {
            let w = 360.0 / n as f32;
            for i in 0..n {
                // Middle of the sector, and just inside both of its boundaries.
                for off in [0.0, w / 2.0 - 0.1, -(w / 2.0 - 0.1)] {
                    let (x, y) = point(i as f32 * w + off, 80.0);
                    assert_eq!(hit_test(x, y, &R, n, 12), Hit::Inner(i), "n={n} i={i} off={off}");
                }
            }
            // The inner count does not change the outer ring.
            let (x, y) = point(0.0, 200.0);
            assert_eq!(hit_test(x, y, &R, n, 12), Hit::Outer(0));
        }
    }

    #[test]
    fn inner_boundary_with_four_sectors() {
        let (x, y) = point(44.9, 80.0);
        assert_eq!(hit_test(x, y, &R, 4, 12), Hit::Inner(0));
        let (x, y) = point(45.1, 80.0);
        assert_eq!(hit_test(x, y, &R, 4, 12), Hit::Inner(1));
        let (x, y) = point(314.0, 80.0);
        assert_eq!(hit_test(x, y, &R, 4, 12), Hit::Inner(3));
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
        assert_eq!(hit_test(R.center, 0.0, &R, 12, 12), Hit::Inner(3));
        assert_eq!(hit_test(R.center - eps, 0.0, &R, 12, 12), Hit::Center);
        assert_eq!(hit_test(R.inner, 0.0, &R, 12, 12), Hit::Outer(3));
        assert_eq!(hit_test(R.inner - eps, 0.0, &R, 12, 12), Hit::Inner(3));
    }

    #[test]
    fn clamps_inner_sectors() {
        for (input, want) in [(0, 4), (3, 4), (4, 4), (12, 12), (13, 12), (255, 12)] {
            assert_eq!(clamp_inner_sectors(input), want, "input={input}");
        }
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
    fn arc_tiles_cover_arc_around_parent() {
        for n in 4u8..=12 {
            for parent in 0..n {
                for count in 1u8..=5 {
                    let arc = SubArc { parent, count };
                    let middle = parent as f32 * 360.0 / n as f32;
                    let first = sub_tile_span(arc, 0, n);
                    let last = sub_tile_span(arc, count - 1, n);
                    let half = count as f32 * SUB_TILE_DEG / 2.0;
                    assert!((first.0 - (middle - half)).abs() < 1e-3);
                    assert!((last.1 - (middle + half)).abs() < 1e-3);
                    for k in 0..count {
                        let (s, e) = sub_tile_span(arc, k, n);
                        assert!((e - s - SUB_TILE_DEG).abs() < 1e-3);
                        // The middle of every tile hits that tile, also far beyond the band.
                        for dist in [250.0, 10_000.0] {
                            let (x, y) = point((s + e) / 2.0, dist);
                            assert_eq!(
                                hit_test_arc(x, y, &R, 12, n, Some(arc)),
                                Hit::Sub(parent, k),
                                "n={n} parent={parent} count={count} k={k} dist={dist}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn arc_angular_edges() {
        let arc = SubArc {
            parent: 3,
            count: 3,
        };
        let middle = 90.0; // sector 3 of 12
        let half = 3.0 * SUB_TILE_DEG / 2.0;
        let hit = |deg: f32| {
            let (x, y) = point(deg, 250.0);
            hit_test_arc(x, y, &R, 12, 12, Some(arc))
        };
        assert_eq!(hit(middle - half + 0.05), Hit::Sub(3, 0));
        assert_eq!(hit(middle - half - 0.05), Hit::Outer(2));
        assert_eq!(hit(middle + half - 0.05), Hit::Sub(3, 2));
        assert_eq!(hit(middle + half + 0.05), Hit::Outer(4));
        assert_eq!(hit(middle - 13.0 + 0.05), Hit::Sub(3, 1));
        assert_eq!(hit(middle - 13.0 - 0.05), Hit::Sub(3, 0));
    }

    #[test]
    fn arc_wraps_over_zero() {
        for (parent, n) in [(0u8, 12u8), (11, 12), (0, 4), (3, 4)] {
            for count in 1u8..=5 {
                let arc = SubArc { parent, count };
                let middle = parent as f32 * 360.0 / n as f32;
                for k in 0..count {
                    let (s, e) = sub_tile_span(arc, k, n);
                    let (x, y) = point((s + e) / 2.0, 240.0);
                    assert_eq!(
                        hit_test_arc(x, y, &R, 12, n, Some(arc)),
                        Hit::Sub(parent, k),
                        "parent={parent} n={n} count={count} k={k}"
                    );
                }
                let half = count as f32 * SUB_TILE_DEG / 2.0;
                let (x, y) = point(middle + half + 0.05, 240.0);
                assert!(matches!(
                    hit_test_arc(x, y, &R, 12, n, Some(arc)),
                    Hit::Outer(_)
                ));
            }
        }
    }

    #[test]
    fn arc_radial_boundaries() {
        let arc = SubArc {
            parent: 0,
            count: 2,
        };
        let eps = 0.01;
        let at = |d: f32| hit_test_arc(0.0, -d, &R, 12, 12, Some(arc));
        assert_eq!(at(R.outer - eps), Hit::Outer(0));
        assert_eq!(at(R.outer), Hit::Sub(0, 1));
        assert_eq!(at(R.inner), Hit::Outer(0));
        let one = SubArc {
            parent: 0,
            count: 1,
        };
        assert_eq!(hit_test_arc(0.0, -250.0, &R, 12, 12, Some(one)), Hit::Sub(0, 0));
    }

    #[test]
    fn arc_ignores_inner_center_and_invalid_arc() {
        let arc = SubArc {
            parent: 0,
            count: 5,
        };
        assert_eq!(hit_test_arc(0.0, 0.0, &R, 12, 12, Some(arc)), Hit::Center);
        assert_eq!(hit_test_arc(0.0, -80.0, &R, 12, 12, Some(arc)), Hit::Inner(0));
        for (parent, count) in [(6u8, 2u8), (0, 0), (0, 6)] {
            let bad = SubArc { parent, count };
            assert_eq!(hit_test_arc(0.0, -250.0, &R, 12, 6, Some(bad)), Hit::Outer(0));
        }
        assert_eq!(hit_test_arc(0.0, -250.0, &R, 12, 12, None), Hit::Outer(0));
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
