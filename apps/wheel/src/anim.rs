//! Animation timing: durations, easing and the state of the highlight and submenu-arc
//! transitions. Pure logic; the opening and closing fades are run by DirectComposition
//! (see `Renderer::start_open`), the rest is stepped by the window's poll timer.

use std::time::Instant;

use wheel_core::Hit;
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
};
use windows::core::BOOL;

/// Fade and scale of the opening wheel.
pub const OPEN_MS: u32 = 120;
/// The scale the opening wheel starts from (it ends at 1).
pub const OPEN_SCALE_FROM: f32 = 0.92;
/// Fade of the closing wheel; the window is hidden after it.
pub const CLOSE_MS: u32 = 80;
/// A tile fully gaining or losing its highlight.
const HIGHLIGHT_MS: f32 = 90.0;
/// The submenu arc sliding out.
const ARC_MS: f32 = 110.0;
/// How far (at scale 1) the arc starts closer to the wheel than its final place.
pub const ARC_SLIDE_PX: f32 = 8.0;
/// Longest step taken at once, so that a stalled timer does not skip the animation.
const MAX_STEP_MS: f32 = 50.0;

/// Whether to animate: the `animations` option and the Windows setting must both allow it.
pub fn should_animate(option: bool, windows: bool) -> bool {
    option && windows
}

/// The Windows "Animate controls and elements inside windows" setting (on if it cannot be read).
pub fn windows_animations() -> bool {
    let mut on = BOOL(1);
    // SAFETY: `on` is a BOOL, the out buffer SPI_GETCLIENTAREAANIMATION writes.
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(std::ptr::from_mut(&mut on).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    read.is_err() || on.as_bool()
}

/// Share (0..=1) of an animation of `duration_ms` done after `elapsed_ms`.
pub fn progress(elapsed_ms: f32, duration_ms: f32) -> f32 {
    if duration_ms <= 0.0 {
        return 1.0;
    }
    (elapsed_ms / duration_ms).clamp(0.0, 1.0)
}

/// Cubic ease-out: fast start, slow end.
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// Coefficients (constant, linear, quadratic, cubic; time in seconds) of the cubic that goes from
/// `from` to `to` in `duration_s` along `ease_out`, for `IDCompositionAnimation::AddCubic`.
pub fn ease_out_cubic(from: f32, to: f32, duration_s: f32) -> [f32; 4] {
    let d = to - from;
    [
        from,
        3.0 * d / duration_s,
        -3.0 * d / (duration_s * duration_s),
        d / (duration_s * duration_s * duration_s),
    ]
}

/// What the wheel shows besides the tile under the cursor: how much each tile is highlighted and
/// how far the submenu arc has slid out.
pub struct Anim {
    enabled: bool,
    /// Highlight (0..=1, not yet eased) of the tiles that are or were highlighted.
    levels: Vec<(Hit, f32)>,
    target: Hit,
    /// Time since the arc opened; negative while it waits.
    arc_ms: f32,
    /// Something is still moving.
    moving: bool,
    last: Instant,
}

impl Anim {
    /// State for a wheel just shown with `hit` highlighted. With `arc_delay_ms` the arc already
    /// open slides out after that wait; `None` leaves it in place.
    pub fn new(enabled: bool, hit: Hit, arc_delay_ms: Option<f32>, now: Instant) -> Anim {
        let slide = enabled && arc_delay_ms.is_some();
        Anim {
            enabled,
            levels: vec![(hit, 1.0)],
            target: hit,
            arc_ms: match arc_delay_ms {
                Some(delay) if enabled => -delay,
                _ => ARC_MS,
            },
            moving: slide,
            last: now,
        }
    }

    /// A finished state with `hit` highlighted and nothing moving.
    pub fn settled(hit: Hit) -> Anim {
        Anim::new(false, hit, None, Instant::now())
    }

    /// The highlight moves to `hit`.
    pub fn set_target(&mut self, hit: Hit) {
        if hit == self.target {
            return;
        }
        self.target = hit;
        if !self.enabled {
            self.levels = vec![(hit, 1.0)];
        } else if !self.levels.iter().any(|(h, _)| *h == hit) {
            self.levels.push((hit, 0.0));
        }
    }

    /// A submenu arc opens (or another one replaces the open one).
    pub fn restart_arc(&mut self) {
        if self.enabled {
            self.arc_ms = 0.0;
        }
    }

    /// Advances by `dt_ms`; true while something is still moving.
    fn step(&mut self, dt_ms: f32) -> bool {
        let delta = dt_ms / HIGHLIGHT_MS;
        let target = self.target;
        for (hit, level) in &mut self.levels {
            *level = if *hit == target {
                (*level + delta).min(1.0)
            } else {
                (*level - delta).max(0.0)
            };
        }
        self.levels
            .retain(|(hit, level)| *hit == target || *level > 0.0);
        self.arc_ms = (self.arc_ms + dt_ms).min(ARC_MS);
        self.arc_ms < ARC_MS
            || self
                .levels
                .iter()
                .any(|(hit, level)| *hit != target || *level < 1.0)
    }

    /// Call on every poll tick; `changed` says that the highlighted tile or the arc just changed.
    /// Returns whether the wheel must be drawn again.
    pub fn tick(&mut self, changed: bool, now: Instant) -> bool {
        let dt = if self.moving {
            (now.saturating_duration_since(self.last).as_secs_f32() * 1000.0).min(MAX_STEP_MS)
        } else {
            0.0
        };
        self.last = now;
        let redraw = changed || self.moving;
        self.moving = self.step(dt);
        redraw
    }

    /// How much `hit` is highlighted, 0..=1 (eased).
    pub fn level(&self, hit: Hit) -> f32 {
        self.levels
            .iter()
            .find(|(h, _)| *h == hit)
            .map_or(0.0, |(_, level)| ease_out(*level))
    }

    /// How far the arc has slid out, 0..=1 (eased).
    pub fn arc(&self) -> f32 {
        ease_out(progress(self.arc_ms, ARC_MS))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn progress_is_clamped() {
        assert_eq!(progress(-5.0, 100.0), 0.0);
        assert_eq!(progress(50.0, 100.0), 0.5);
        assert_eq!(progress(500.0, 100.0), 1.0);
        assert_eq!(progress(1.0, 0.0), 1.0);
    }

    #[test]
    fn ease_out_goes_from_0_to_1_and_starts_fast() {
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
        assert!(ease_out(0.5) > 0.5);
        assert!(ease_out(0.2) < ease_out(0.4));
        assert_eq!(ease_out(2.0), 1.0);
    }

    #[test]
    fn cubic_matches_ease_out() {
        let (from, to, d) = (0.92, 1.0, 0.12);
        let [c0, c1, c2, c3] = ease_out_cubic(from, to, d);
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let s = t * d;
            let value = c0 + c1 * s + c2 * s * s + c3 * s * s * s;
            assert!(close(value, from + (to - from) * ease_out(t)), "t={t}");
        }
        // Falling: 1 to 0.
        let [c0, c1, c2, c3] = ease_out_cubic(1.0, 0.0, 0.08);
        let end = c0 + c1 * 0.08 + c2 * 0.08 * 0.08 + c3 * 0.08 * 0.08 * 0.08;
        assert!(close(end, 0.0));
    }

    #[test]
    fn animate_only_when_both_allow() {
        assert!(should_animate(true, true));
        assert!(!should_animate(false, true));
        assert!(!should_animate(true, false));
        assert!(!should_animate(false, false));
    }

    #[test]
    fn highlight_crossfades_and_settles() {
        let t0 = Instant::now();
        let (a, b) = (Hit::Outer(0), Hit::Outer(1));
        let mut anim = Anim::new(true, a, None, t0);
        assert_eq!((anim.level(a), anim.level(b)), (1.0, 0.0));
        anim.set_target(b);
        // The change itself is drawn at once, then the timer steps.
        assert!(anim.tick(true, t0));
        let mut now = t0;
        let mut frames = 0;
        let mut previous = (1.0, 0.0);
        loop {
            now += Duration::from_millis(16);
            let redraw = anim.tick(false, now);
            let levels = (anim.level(a), anim.level(b));
            assert!(levels.0 <= previous.0 && levels.1 >= previous.1);
            previous = levels;
            if !redraw {
                break;
            }
            frames += 1;
            assert!(frames < 20);
        }
        assert_eq!((anim.level(a), anim.level(b)), (0.0, 1.0));
        assert!((5..=8).contains(&frames), "{frames}");
        // At rest nothing is drawn.
        assert!(!anim.tick(false, now + Duration::from_millis(16)));
    }

    #[test]
    fn interrupted_transition_keeps_levels() {
        let t0 = Instant::now();
        let (a, b, c) = (Hit::Inner(0), Hit::Inner(1), Hit::Inner(2));
        let mut anim = Anim::new(true, a, None, t0);
        anim.set_target(b);
        anim.tick(true, t0);
        anim.tick(false, t0 + Duration::from_millis(30));
        let a_before = anim.level(a);
        anim.set_target(c);
        anim.tick(true, t0 + Duration::from_millis(30));
        assert!(close(anim.level(a), a_before));
        assert!(anim.level(b) > 0.0);
        assert_eq!(anim.level(c), 0.0);
    }

    #[test]
    fn disabled_changes_at_once() {
        let t0 = Instant::now();
        let (a, b) = (Hit::Outer(0), Hit::Outer(1));
        let mut anim = Anim::new(false, a, Some(0.0), t0);
        anim.set_target(b);
        anim.restart_arc();
        assert_eq!((anim.level(a), anim.level(b), anim.arc()), (0.0, 1.0, 1.0));
        assert!(anim.tick(true, t0));
        assert!(!anim.tick(false, t0 + Duration::from_millis(16)));
    }

    #[test]
    fn arc_slides_after_its_delay() {
        let t0 = Instant::now();
        let mut anim = Anim::new(true, Hit::Center, Some(40.0), t0);
        assert_eq!(anim.arc(), 0.0);
        let mut now = t0;
        let mut first_moved = None;
        for frame in 1..30 {
            now += Duration::from_millis(16);
            let redraw = anim.tick(false, now);
            if first_moved.is_none() && anim.arc() > 0.0 {
                first_moved = Some(frame);
            }
            if !redraw {
                break;
            }
        }
        assert_eq!(anim.arc(), 1.0);
        assert!(first_moved.is_some_and(|f| f >= 3));
    }

    #[test]
    fn a_stalled_timer_cannot_skip_the_fade() {
        let t0 = Instant::now();
        let (a, b) = (Hit::Outer(0), Hit::Outer(1));
        let mut anim = Anim::new(true, a, None, t0);
        anim.set_target(b);
        anim.tick(true, t0);
        anim.tick(false, t0 + Duration::from_secs(5));
        assert!(anim.level(b) < 1.0);
    }
}
