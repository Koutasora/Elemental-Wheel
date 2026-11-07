//! Wheel dimensions in physical pixels, from monitor DPI and user scale.
//! Angles use the same convention as `geometry`: 0 = up, clockwise; sector 0 is centred at 0.

use crate::geometry::{Radii, SubArc, clamp_inner_sectors, clamp_outer_sectors, sub_tile_span};

/// Smallest allowed scale, in percent.
pub const MIN_SCALE_PCT: u16 = 60;
/// Largest allowed scale, in percent.
pub const MAX_SCALE_PCT: u16 = 140;
/// Default scale, in percent.
pub const DEFAULT_SCALE_PCT: u16 = 100;

// Base dimensions at 100% and 96 DPI, from the mockup in `docs/koncept.html`
// (R.c = 60, inner ring 64..122, outer ring 126..210). The ring boundary sits in the middle
// of the 122..126 gap. The submenu arc band is 218..292; the outer ring boundary for hits
// beyond the wheel sits in the middle of the 210..218 gap.
const BASE_CENTER: f32 = 60.0;
const BASE_INNER: f32 = 124.0;
const BASE_OUTER: f32 = 210.0;
const BASE_OUTER_LIMIT: f32 = 214.0;
const BASE_SUB_INNER: f32 = 218.0;
const BASE_SUB_OUTER: f32 = 292.0;
// Free space around the arc band, so its outline and antialiasing are not clipped by the window.
const BASE_EDGE_MARGIN: f32 = 4.0;

/// Clamps the scale to `MIN_SCALE_PCT..=MAX_SCALE_PCT`.
pub fn clamp_scale_pct(pct: u16) -> u16 {
    pct.clamp(MIN_SCALE_PCT, MAX_SCALE_PCT)
}

/// Wheel size for a given DPI, scale and ring sector counts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    factor: f32,
    inner_sectors: u8,
    outer_sectors: u8,
}

impl Layout {
    /// Clamps `scale_pct`, `inner_sectors` and `outer_sectors`; a `dpi` of 0 is treated as 96.
    pub fn new(dpi: u32, scale_pct: u16, inner_sectors: u8, outer_sectors: u8) -> Layout {
        let dpi = if dpi == 0 { 96 } else { dpi };
        Layout {
            factor: dpi as f32 / 96.0 * clamp_scale_pct(scale_pct) as f32 / 100.0,
            inner_sectors: clamp_inner_sectors(inner_sectors),
            outer_sectors: clamp_outer_sectors(outer_sectors),
        }
    }

    /// Ring boundaries in physical pixels.
    pub fn radii(&self) -> Radii {
        Radii {
            center: BASE_CENTER * self.factor,
            inner: BASE_INNER * self.factor,
            outer: BASE_OUTER_LIMIT * self.factor,
        }
    }

    /// Outer edge of the outer ring in physical pixels.
    pub fn outer_radius(&self) -> f32 {
        BASE_OUTER * self.factor
    }

    /// Inner and outer edge of the submenu arc band in physical pixels.
    pub fn sub_radii(&self) -> (f32, f32) {
        (BASE_SUB_INNER * self.factor, BASE_SUB_OUTER * self.factor)
    }

    /// Distance beyond which a click in click mode closes the wheel even in the direction of an
    /// open arc: the outer edge of the arc band plus the margin (the mockup's `R.s2 + 4`).
    pub fn sub_click_limit(&self) -> f32 {
        (BASE_SUB_OUTER + BASE_EDGE_MARGIN) * self.factor
    }

    pub fn inner_sectors(&self) -> u8 {
        self.inner_sectors
    }

    pub fn outer_sectors(&self) -> u8 {
        self.outer_sectors
    }

    /// Start and end angle of an outer ring sector, in degrees.
    pub fn outer_span(&self, i: u8) -> (f32, f32) {
        span(i, self.outer_sectors)
    }

    /// Start and end angle of an inner ring sector, in degrees.
    pub fn inner_span(&self, i: u8) -> (f32, f32) {
        span(i, self.inner_sectors)
    }

    /// Start and end angle of tile `k` of the submenu arc `arc`, in degrees.
    pub fn sub_span(&self, arc: SubArc, k: u8) -> (f32, f32) {
        sub_tile_span(arc, k, self.outer_sectors)
    }

    /// Side length of a square window that fits the wheel with its submenu arc band and a small
    /// margin around it.
    pub fn size_px(&self) -> u32 {
        ((BASE_SUB_OUTER + BASE_EDGE_MARGIN) * self.factor * 2.0).ceil() as u32
    }
}

fn span(i: u8, sectors: u8) -> (f32, f32) {
    let w = 360.0 / sectors as f32;
    (i as f32 * w - w / 2.0, i as f32 * w + w / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn clamps_scale() {
        for (input, want) in [
            (0, 60),
            (59, 60),
            (60, 60),
            (100, 100),
            (140, 140),
            (141, 140),
        ] {
            assert_eq!(clamp_scale_pct(input), want, "input={input}");
        }
    }

    #[test]
    fn new_clamps_inputs() {
        let l = Layout::new(96, 500, 12, 0);
        assert_eq!(l.outer_sectors(), 4);
        assert_eq!(l, Layout::new(96, 140, 12, 4));
        assert_eq!(Layout::new(96, 0, 12, 99), Layout::new(96, 60, 12, 12));
        let l = Layout::new(96, 100, 0, 12);
        assert_eq!(l.inner_sectors(), 4);
        assert_eq!(Layout::new(96, 100, 99, 12).inner_sectors(), 12);
    }

    #[test]
    fn default_scale_96_dpi_matches_base() {
        let l = Layout::new(96, DEFAULT_SCALE_PCT, 12, 12);
        assert!(close(l.radii().center, 60.0));
        assert!(close(l.radii().inner, 124.0));
        assert!(close(l.outer_radius(), 210.0));
        assert!(close(l.radii().outer, 214.0));
        assert!(close(l.sub_radii().0, 218.0));
        assert!(close(l.sub_radii().1, 292.0));
        assert_eq!(l.size_px(), 592);
    }

    #[test]
    fn scales_with_dpi_and_percent() {
        let base = Layout::new(96, 100, 12, 12);
        for pct in [60u16, 100, 140] {
            for dpi in [96u32, 144] {
                let l = Layout::new(dpi, pct, 12, 12);
                let k = dpi as f32 / 96.0 * pct as f32 / 100.0;
                assert!(close(l.radii().center, base.radii().center * k));
                assert!(close(l.radii().inner, base.radii().inner * k));
                assert!(close(l.outer_radius(), base.outer_radius() * k));
                assert!(close(l.radii().outer, base.radii().outer * k));
                assert!(close(l.sub_radii().0, base.sub_radii().0 * k));
                assert!(close(l.sub_radii().1, base.sub_radii().1 * k));
            }
        }
        let l144 = Layout::new(144, 100, 12, 12);
        assert!(close(l144.radii().center, base.radii().center * 1.5));
        assert!(close(l144.outer_radius(), base.outer_radius() * 1.5));
    }

    #[test]
    fn dpi_zero_is_96() {
        assert_eq!(Layout::new(0, 100, 12, 12), Layout::new(96, 100, 12, 12));
    }

    #[test]
    fn size_fits_sub_band_rounded_up() {
        for dpi in [96u32, 120, 144, 192] {
            for pct in [60u16, 77, 100, 140] {
                let l = Layout::new(dpi, pct, 12, 12);
                let margin = BASE_EDGE_MARGIN * l.factor;
                let side = (l.sub_radii().1 + margin) * 2.0;
                assert!(l.size_px() as f32 >= side);
                assert!((l.size_px() as f32) < side + 1.0);
                // At least 2 px free on each side even at the smallest scale.
                assert!(margin >= 2.0);
            }
        }
    }

    #[test]
    fn sub_span_is_centred_on_parent() {
        let l = Layout::new(96, 100, 12, 8);
        let arc = SubArc {
            parent: 2,
            count: 4,
        };
        assert!(close(l.sub_span(arc, 0).0, 90.0 - 52.0));
        assert!(close(l.sub_span(arc, 3).1, 90.0 + 52.0));
    }

    #[test]
    fn sub_span_wraps_over_zero() {
        let l = Layout::new(96, 100, 12, 12);
        let arc = SubArc {
            parent: 0,
            count: 5,
        };
        assert!(close(l.sub_span(arc, 0).0, -65.0));
        assert!(close(l.sub_span(arc, 2).0, -13.0));
        assert!(close(l.sub_span(arc, 4).1, 65.0));
    }

    #[test]
    fn radii_are_ordered() {
        for pct in [60, 100, 140] {
            let l = Layout::new(144, pct, 12, 12);
            assert!(l.radii().inner < l.outer_radius());
            assert!(l.outer_radius() < l.radii().outer);
            assert!(l.radii().outer < l.sub_radii().0);
            assert!(l.sub_radii().0 < l.sub_radii().1);
        }
    }

    #[test]
    fn spans_are_consecutive_and_cover_circle() {
        for n in [4u8, 6, 8, 12] {
            let l = Layout::new(96, 100, 12, n);
            assert!(close(l.outer_span(0).0, -180.0 / n as f32));
            let mut total = 0.0;
            for i in 0..n {
                let (s, e) = l.outer_span(i);
                total += e - s;
                if i + 1 < n {
                    assert!(close(e, l.outer_span(i + 1).0));
                }
                assert!(close((s + e) / 2.0, i as f32 * 360.0 / n as f32));
            }
            assert!(close(total, 360.0));
        }
        for n in [4u8, 6, 12] {
            let l = Layout::new(96, 100, n, 5);
            let mut total = 0.0;
            for i in 0..n {
                let (s, e) = l.inner_span(i);
                total += e - s;
                assert!(close((s + e) / 2.0, i as f32 * 360.0 / n as f32));
            }
            assert!(close(total, 360.0));
            assert!(close(l.inner_span(0).0, -180.0 / n as f32));
        }
    }

    #[test]
    fn sub_click_limit_is_band_edge_plus_margin() {
        assert!(close(Layout::new(96, 100, 12, 12).sub_click_limit(), 296.0));
        assert!(close(Layout::new(144, 100, 12, 12).sub_click_limit(), 444.0));
    }
}
