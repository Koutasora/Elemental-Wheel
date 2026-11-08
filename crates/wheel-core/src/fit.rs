//! Label fitting: picks line split and font scale from measured text sizes. Pure logic, so it is
//! tested with a fake measure function instead of DirectWrite. Shared by the wheel and the editor
//! preview.

/// Smallest font scale (relative to the base size) tried before falling back to ellipsis
/// trimming; callers can raise it with an absolute minimum size.
pub const MIN_SCALE: f32 = 0.75;
const STEP: f32 = 0.05;

#[derive(Debug, Clone, PartialEq)]
pub struct Fitted {
    /// Text to draw; a `\n` separates the two lines.
    pub text: String,
    /// Font size relative to the base size (the floor given to `fit` up to 1.0, or the floor itself
    /// when that is above 1.0).
    pub scale: f32,
    /// True when nothing fits even at `MIN_SCALE` and the text has to be trimmed.
    pub trim: bool,
}

/// Chooses how to draw `text`: one line at the base size, else two lines at the base size, else
/// the same two tries at smaller sizes down to the floor, else trimmed at the floor. The floor is
/// `MIN_SCALE` or `min_scale` (an absolute minimum font size expressed relative to the base size),
/// whichever is larger; when it is above 1.0 the base size itself is raised to it.
///
/// `measure(text, scale)` returns the (width, height) of the text at that scale (`\n` starts a new
/// line); `avail(height)` returns the width available for a block of that height, or 0 or less
/// when it does not fit at all.
pub fn fit(
    text: &str,
    measure: impl Fn(&str, f32) -> (f32, f32),
    avail: impl Fn(f32) -> f32,
    min_scale: f32,
) -> Fitted {
    let floor = MIN_SCALE.max(min_scale);
    let fits = |t: &str, s: f32| {
        let (w, h) = measure(t, s);
        w <= avail(h)
    };
    let mut scale = 1.0f32.max(floor);
    loop {
        if fits(text, scale) {
            return Fitted {
                text: text.to_string(),
                scale,
                trim: false,
            };
        }
        if let Some(two) = best_split(text, |t| measure(t, scale).0)
            && fits(&two, scale)
        {
            return Fitted {
                text: two,
                scale,
                trim: false,
            };
        }
        if scale <= floor {
            break;
        }
        scale = (scale - STEP).max(floor);
    }
    // Trimmed: use two lines when their height fits at all, like a long name in the mockup.
    let two =
        best_split(text, |t| measure(t, floor).0).filter(|t| avail(measure(t, floor).1) > 0.0);
    Fitted {
        text: two.unwrap_or_else(|| text.to_string()),
        scale: floor,
        trim: true,
    }
}

/// The two-line version of `text` split at the space that gives the narrowest widest line.
fn best_split(text: &str, width: impl Fn(&str) -> f32) -> Option<String> {
    text.match_indices(' ')
        .map(|(i, _)| format!("{}\n{}", &text[..i], &text[i + 1..]))
        .map(|t| (width(&t), t))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, t)| t)
}

/// Widest block of height `h` centred at `at` (relative to the wheel centre) whose corners all
/// lie in the annular sector `ring` x `a1..a2` (degrees, 0 = up, clockwise).
pub fn sector_width(at: (f32, f32), ring: (f32, f32), a1: f32, a2: f32, h: f32) -> f32 {
    max_width(h, |dx, dy| in_sector((at.0 + dx, at.1 + dy), ring, a1, a2))
}

/// Like `sector_width`, but the block may slide along the sector's middle line. Returns the
/// width and centre (relative to the wheel centre) of a block of height `h`: the position
/// closest to the ring's mid radius where the width is at least `need`; when none is wide enough
/// (e.g. `need` is infinite) the widest position, which is toward the sector's outer edge.
pub fn sector_slot(ring: (f32, f32), a1: f32, a2: f32, h: f32, need: f32) -> (f32, (f32, f32)) {
    const STEPS: u32 = 48;
    let mid = ((a1 + a2) / 2.0).to_radians();
    let mid_r = (ring.0 + ring.1) / 2.0;
    let mut widest = (0.0, (0.0, 0.0));
    let mut nearest: Option<(f32, f32, (f32, f32))> = None; // (distance from mid, width, centre)
    for i in 0..=STEPS {
        let r = ring.0 + (ring.1 - ring.0) * i as f32 / STEPS as f32;
        let at = (r * mid.sin(), -r * mid.cos());
        let w = sector_width(at, ring, a1, a2, h);
        if w > widest.0 {
            widest = (w, at);
        }
        let d = (r - mid_r).abs();
        if w >= need && nearest.is_none_or(|n| d < n.0) {
            nearest = Some((d, w, at));
        }
    }
    nearest.map_or(widest, |(_, w, at)| (w, at))
}

/// Widest block of height `h` centred in a circle of radius `r`.
pub fn circle_width(r: f32, h: f32) -> f32 {
    max_width(h, |dx, dy| dx.hypot(dy) <= r)
}

/// Largest width for which the corners (+-w/2, +-h/2) satisfy `inside`; 0 when none does.
fn max_width(h: f32, inside: impl Fn(f32, f32) -> bool) -> f32 {
    let ok = |w: f32| {
        [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
            .iter()
            .all(|(sx, sy)| inside(sx * w / 2.0, sy * h / 2.0))
    };
    if !ok(0.0) {
        return 0.0;
    }
    let (mut lo, mut hi) = (0.0, 1000.0);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if ok(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

fn in_sector((x, y): (f32, f32), ring: (f32, f32), a1: f32, a2: f32) -> bool {
    let r = x.hypot(y);
    let mid = (a1 + a2) / 2.0;
    let diff = (x.atan2(-y).to_degrees() - mid + 540.0).rem_euclid(360.0) - 180.0;
    r >= ring.0 && r <= ring.1 && diff.abs() <= (a2 - a1) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 10 px per character per line at scale 1, 12 px per line.
    fn measure(text: &str, scale: f32) -> (f32, f32) {
        let lines = text.split('\n');
        let w = lines.clone().map(|l| l.chars().count()).max().unwrap_or(0) as f32 * 10.0;
        (w * scale, lines.count() as f32 * 12.0 * scale)
    }

    /// Fixed width, but only blocks up to 30 px tall.
    fn avail_for(width: f32) -> impl Fn(f32) -> f32 {
        move |h| if h <= 30.0 { width } else { 0.0 }
    }

    #[test]
    fn one_line_at_base_size() {
        let f = fit("Kopiuj", measure, avail_for(100.0), 0.0);
        assert_eq!((f.text.as_str(), f.scale, f.trim), ("Kopiuj", 1.0, false));
    }

    #[test]
    fn two_lines_at_base_size_with_balanced_split() {
        // 17 chars do not fit in 120 px; the best split is after "bbbbbbb".
        let f = fit("aaa bbbbbbb cc dd", measure, avail_for(120.0), 0.0);
        assert_eq!(f.text, "aaa bbbbbbb\ncc dd");
        assert_eq!((f.scale, f.trim), (1.0, false));
        let f = fit("aaaa bb cccccc", measure, avail_for(80.0), 0.0);
        assert_eq!(f.text, "aaaa bb\ncccccc");
    }

    #[test]
    fn single_word_shrinks_in_steps() {
        // 10 chars = 100 px at 1.0; 91 px needs scale 0.9.
        let f = fit("aaaaaaaaaa", measure, avail_for(91.0), 0.0);
        assert_eq!(f.text, "aaaaaaaaaa");
        assert!((f.scale - 0.9).abs() < 1e-4);
        assert!(!f.trim);
    }

    #[test]
    fn prefers_base_size_two_lines_over_smaller_one_line() {
        let f = fit("aaaaa bbbbb", measure, avail_for(60.0), 0.0);
        assert_eq!(f.text, "aaaaa\nbbbbb");
        assert_eq!(f.scale, 1.0);
    }

    #[test]
    fn trims_at_min_scale_when_nothing_fits() {
        let f = fit("aaaaaaaaaaaa", measure, avail_for(50.0), 0.0);
        assert_eq!(f.text, "aaaaaaaaaaaa");
        assert_eq!((f.scale, f.trim), (MIN_SCALE, true));
    }

    #[test]
    fn trimmed_text_keeps_two_lines_when_they_fit_the_height() {
        let f = fit("aaaaaaaaaaaa bbbbbbbbbbbb", measure, avail_for(50.0), 0.0);
        assert_eq!(f.text, "aaaaaaaaaaaa\nbbbbbbbbbbbb");
        assert!(f.trim);
    }

    #[test]
    fn height_limit_forces_one_line() {
        // Two lines are 19.2 px tall even at the minimum scale: only 15 px are available.
        let avail = |h: f32| if h <= 15.0 { 100.0 } else { 0.0 };
        let f = fit("aaaaaaaaaa bbbbbbbbbb", measure, avail, 0.0);
        assert_eq!(f.text, "aaaaaaaaaa bbbbbbbbbb");
        assert!(f.trim);
    }

    #[test]
    fn absolute_minimum_raises_the_floor_and_trims() {
        // Floor 0.9: 12 chars need 108 px at 0.9, only 100 available -> trimmed at 0.9.
        let f = fit("aaaaaaaaaaaa", measure, avail_for(100.0), 0.9);
        assert_eq!((f.scale, f.trim), (0.9, true));
        // The same text fits at 0.8 when the floor does not forbid it.
        let f = fit("aaaaaaaaaaaa", measure, avail_for(100.0), 0.0);
        assert!(f.scale < 0.9 && !f.trim, "{f:?}");
    }

    #[test]
    fn floor_above_base_size_raises_the_start() {
        let f = fit("aaaaa", measure, avail_for(100.0), 1.4);
        assert!((f.scale - 1.4).abs() < 1e-4 && !f.trim, "{f:?}");
        let f = fit("aaaaa", measure, avail_for(60.0), 1.4);
        assert!((f.scale - 1.4).abs() < 1e-4 && f.trim, "{f:?}");
    }

    #[test]
    fn sector_width_follows_the_wedge() {
        // Sector centred at the top, ring 64..122, 28 degrees wide: label at radius 95.
        let w = sector_width((0.0, -95.0), (64.0, 122.0), -14.0, 14.0, 14.0);
        assert!(w > 40.0 && w < 50.0, "{w}");
        // Too tall for the ring: nothing fits.
        assert_eq!(
            sector_width((0.0, -95.0), (64.0, 122.0), -14.0, 14.0, 80.0),
            0.0
        );
    }

    #[test]
    fn sector_at_the_side_is_wide_in_the_radial_direction() {
        // Sector centred at 90 degrees (right): horizontal text spans the ring thickness.
        let w = sector_width((95.0, 0.0), (64.0, 122.0), 76.0, 104.0, 14.0);
        assert!(w > 40.0 && w < 58.0, "{w}");
    }

    #[test]
    fn slot_stays_at_mid_radius_when_the_label_fits_there() {
        let (ring, a1, a2, h) = ((64.0, 122.0), -13.0, 13.0, 14.0);
        let at_mid = sector_width((0.0, -93.0), ring, a1, a2, h);
        let (w, at) = sector_slot(ring, a1, a2, h, at_mid - 2.0);
        assert!(w >= at_mid - 2.0);
        assert!((at.1.abs() - 93.0).abs() < 0.7, "{at:?}");
    }

    #[test]
    fn slot_slides_outward_only_as_far_as_needed_and_stays_inside() {
        let (ring, a1, a2, h) = ((64.0, 122.0), -13.0, 13.0, 14.0);
        let at_mid = sector_width((0.0, -93.0), ring, a1, a2, h);
        let need = at_mid + 3.0;
        let (w, at) = sector_slot(ring, a1, a2, h, need);
        assert!(w >= need, "{w} < {need}");
        assert!(at.1.abs() > 93.5, "{at:?}");
        // Not the outermost position: the narrowest sufficient slide is chosen.
        let (widest, widest_at) = sector_slot(ring, a1, a2, h, f32::INFINITY);
        assert!(widest >= w && widest_at.1.abs() >= at.1.abs());
        for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let p = (at.0 + sx * w / 2.0, at.1 + sy * h / 2.0);
            assert!(in_sector(p, ring, a1, a2), "{p:?}");
        }
    }

    #[test]
    fn slot_is_zero_when_too_tall() {
        assert_eq!(sector_slot((64.0, 122.0), -13.0, 13.0, 100.0, 10.0).0, 0.0);
    }

    #[test]
    fn circle_width_matches_chord() {
        let w = circle_width(50.0, 60.0);
        assert!((w - 80.0).abs() < 0.01, "{w}");
    }
}
