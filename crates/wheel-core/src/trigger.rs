//! State machine of the right-mouse-button trigger. Pure logic, no Win32: the app feeds it input
//! events and carries out the returned verdict and effects.
//!
//! Layout: the machine classifies the release over the open wheel, so the caller must call
//! `set_layout` with the wheel's `Layout` when it handles `Effect::OpenWheel` (before the next
//! event). Without it, a default 96 DPI, 100% layout is used.
//!
//! Submenu arc: the wheel window owns the arc state (it follows the cursor). The caller passes the
//! arc that is shown to `set_arc` before it feeds a release or a left press, so that the machine
//! picks what is on screen. The arc is forgotten whenever the wheel is not open.

use crate::geometry::{Hit, SubArc, hit_test_arc};
use crate::layout::Layout;

/// Smallest allowed hold time, in milliseconds.
pub const MIN_HOLD_MS: u32 = 50;
/// Largest allowed hold time, in milliseconds.
pub const MAX_HOLD_MS: u32 = 2000;
/// Default hold time before the wheel opens, in milliseconds.
pub const DEFAULT_HOLD_MS: u32 = 180;
/// Smallest allowed movement threshold, in pixels.
pub const MIN_MOVE_PX: i32 = 1;
/// Largest allowed movement threshold, in pixels.
pub const MAX_MOVE_PX: i32 = 50;
/// Default movement threshold, in pixels.
pub const DEFAULT_MOVE_PX: i32 = 6;

/// Clamps the hold time to `MIN_HOLD_MS..=MAX_HOLD_MS`.
pub fn clamp_hold_ms(ms: u32) -> u32 {
    ms.clamp(MIN_HOLD_MS, MAX_HOLD_MS)
}

/// Clamps the movement threshold to `MIN_MOVE_PX..=MAX_MOVE_PX`.
pub fn clamp_move_px(px: i32) -> i32 {
    px.clamp(MIN_MOVE_PX, MAX_MOVE_PX)
}

/// Trigger thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriggerSettings {
    /// How long the right button must be held without moving to open the wheel.
    pub hold_ms: u32,
    /// Movement further than this (strictly) turns the press into a normal drag.
    pub move_px: i32,
    /// The wheel replaces the right button: it opens on the press, without the hold time and the
    /// movement threshold, so a click or a drag never reaches the program.
    pub replace_right: bool,
}

impl Default for TriggerSettings {
    fn default() -> Self {
        TriggerSettings {
            hold_ms: DEFAULT_HOLD_MS,
            move_px: DEFAULT_MOVE_PX,
            replace_right: false,
        }
    }
}

impl TriggerSettings {
    /// Both values clamped to their allowed ranges.
    pub fn clamped(self) -> TriggerSettings {
        TriggerSettings {
            hold_ms: clamp_hold_ms(self.hold_ms),
            move_px: clamp_move_px(self.move_px),
            replace_right: self.replace_right,
        }
    }
}

/// Screen position in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub fn new(x: i32, y: i32) -> Point {
        Point { x, y }
    }

    fn dist_sq(self, other: Point) -> i64 {
        let dx = i64::from(self.x) - i64::from(other.x);
        let dy = i64::from(self.y) - i64::from(other.y);
        dx * dx + dy * dy
    }
}

/// What happened. `time` is in milliseconds, like `MSLLHOOKSTRUCT.time`; it wraps around.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    RightDown {
        pos: Point,
        time: u32,
    },
    RightUp {
        pos: Point,
        time: u32,
    },
    Move {
        pos: Point,
        time: u32,
    },
    LeftDown {
        pos: Point,
        time: u32,
    },
    LeftUp {
        pos: Point,
        time: u32,
    },
    Escape,
    /// The options key (O) was pressed.
    OptionsKey,
    /// The hold timer started by `Effect::StartHoldTimer` fired.
    HoldElapsed {
        time: u32,
    },
}

/// Who produced the event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Real hardware input (or injected input the caller chose to treat as physical).
    Physical,
    /// Injected by this program (marker set). Always passes, never changes state.
    Own,
    /// Injected by another program. Always passes, never changes state.
    Foreign,
}

/// One input event with its origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    pub kind: EventKind,
    pub origin: Origin,
}

impl Event {
    pub fn physical(kind: EventKind) -> Event {
        Event {
            kind,
            origin: Origin::Physical,
        }
    }
}

/// What to do with the original event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Swallow,
}

/// Something the app has to do. Effects are listed in the order to carry them out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Start a one-shot timer; on expiry feed `EventKind::HoldElapsed`.
    StartHoldTimer(u32),
    /// Replay a right-button press at `down_pos`, then move the cursor back to `current_pos`.
    ReplayDragStart { down_pos: Point, current_pos: Point },
    /// Replay a complete right click.
    ReplayClick { down_pos: Point, up_pos: Point },
    /// Show the wheel centred at `center`; follow with `set_layout`.
    OpenWheel { center: Point },
    /// The wheel stays open and is now driven by the left button.
    EnterClickMode,
    /// The user picked this sector; the wheel closes.
    Select(Hit),
    /// Close the wheel without selecting.
    Cancel,
    /// Close the wheel and open the wheel options (as the `wheel_options` action does).
    OpenOptions,
}

/// Result of one step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub verdict: Verdict,
    pub effects: Vec<Effect>,
}

impl Step {
    fn pass() -> Step {
        Step {
            verdict: Verdict::Pass,
            effects: Vec::new(),
        }
    }

    fn swallow() -> Step {
        Step {
            verdict: Verdict::Swallow,
            effects: Vec::new(),
        }
    }

    fn with(mut self, effect: Effect) -> Step {
        self.effects.push(effect);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    /// Right button is down and swallowed; waiting for the hold time, movement or release.
    Pending {
        down_pos: Point,
        down_time: u32,
    },
    /// Movement started a normal drag; everything passes until the right button is released.
    Dragging,
    /// Wheel open, right button still held. `left_center` is set once the pointer has been
    /// outside the center: a release back in the center then cancels instead of entering the
    /// click mode.
    Open {
        center: Point,
        left_center: bool,
    },
    /// Wheel open after a release in the center; driven by the left button.
    ClickMode {
        center: Point,
    },
    /// Swallow the right-button release that is still to come, then go idle.
    SwallowRightUp,
    /// Swallow the left-button release that is still to come, then go idle.
    SwallowLeftUp,
}

/// The trigger state machine.
#[derive(Debug, Clone)]
pub struct Trigger {
    settings: TriggerSettings,
    layout: Layout,
    /// The submenu arc shown over the open wheel.
    arc: Option<SubArc>,
    state: State,
}

impl Trigger {
    /// Creates the machine in the idle state; `settings` are clamped.
    pub fn new(settings: TriggerSettings) -> Trigger {
        Trigger {
            settings: settings.clamped(),
            layout: Layout::new(96, 100, 12, 12),
            arc: None,
            state: State::Idle,
        }
    }

    pub fn settings(&self) -> TriggerSettings {
        self.settings
    }

    /// Sets the wheel layout used to classify the release; call it when handling `OpenWheel`.
    pub fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
    }

    /// Sets the submenu arc that is currently shown; call it before feeding the event that
    /// selects. It is cleared again when the wheel closes.
    pub fn set_arc(&mut self, arc: Option<SubArc>) {
        self.arc = arc;
    }

    /// Forgets everything and goes idle (e.g. after the hook was detached).
    pub fn reset(&mut self) {
        self.state = State::Idle;
        self.arc = None;
    }

    /// Goes idle after a failure in the program (not a lost hook). Unlike [`reset`](Self::reset)
    /// it keeps swallowing the release of a button whose press was swallowed, so the program
    /// under the cursor never gets a release without its press.
    pub fn recover(&mut self) {
        self.state = match self.state {
            State::Pending { .. } | State::Open { .. } | State::SwallowRightUp => {
                State::SwallowRightUp
            }
            State::SwallowLeftUp => State::SwallowLeftUp,
            // Dragging: the press went to the program, so its release must too. Click mode:
            // the right button is already released.
            _ => State::Idle,
        };
        self.arc = None;
    }

    /// True while the wheel is shown (button held or click mode).
    pub fn is_wheel_open(&self) -> bool {
        matches!(self.state, State::Open { .. } | State::ClickMode { .. })
    }

    /// Feeds one event and returns what to do with it.
    pub fn step(&mut self, event: Event) -> Step {
        let step = self.step_event(event);
        if !self.is_wheel_open() {
            self.arc = None;
        }
        step
    }

    fn step_event(&mut self, event: Event) -> Step {
        if event.origin != Origin::Physical {
            return Step::pass();
        }
        // A physical right-button press starts afresh (it also recovers from a lost release),
        // except while the wheel is open.
        if let EventKind::RightDown { pos, time } = event.kind {
            return match self.state {
                State::Open { .. } => Step::swallow(),
                State::ClickMode { .. } => {
                    self.state = State::SwallowRightUp;
                    Step::swallow().with(Effect::Cancel)
                }
                _ if self.settings.replace_right => {
                    self.state = State::Open {
                        center: pos,
                        left_center: false,
                    };
                    Step::swallow().with(Effect::OpenWheel { center: pos })
                }
                _ => {
                    self.state = State::Pending {
                        down_pos: pos,
                        down_time: time,
                    };
                    Step::swallow().with(Effect::StartHoldTimer(self.settings.hold_ms))
                }
            };
        }
        match self.state {
            State::Idle => Step::pass(),
            State::Dragging => {
                if matches!(event.kind, EventKind::RightUp { .. }) {
                    self.state = State::Idle;
                }
                Step::pass()
            }
            State::SwallowRightUp => self.step_swallow(event.kind, true),
            State::SwallowLeftUp => self.step_swallow(event.kind, false),
            State::Pending {
                down_pos,
                down_time,
            } => self.step_pending(down_pos, down_time, event.kind),
            State::Open {
                center,
                left_center,
            } => self.step_open(center, left_center, event.kind),
            State::ClickMode { center } => self.step_click_mode(center, event.kind),
        }
    }

    /// Waits for the release of the button whose press was already handled.
    fn step_swallow(&mut self, kind: EventKind, right: bool) -> Step {
        let released = match kind {
            EventKind::RightUp { .. } => right,
            EventKind::LeftUp { .. } => !right,
            _ => false,
        };
        if released {
            self.state = State::Idle;
            Step::swallow()
        } else {
            Step::pass()
        }
    }

    fn step_pending(&mut self, down_pos: Point, down_time: u32, kind: EventKind) -> Step {
        match kind {
            EventKind::Move { pos, .. } => {
                let limit = i64::from(self.settings.move_px);
                if pos.dist_sq(down_pos) > limit * limit {
                    self.state = State::Dragging;
                    Step::pass().with(Effect::ReplayDragStart {
                        down_pos,
                        current_pos: pos,
                    })
                } else {
                    Step::pass()
                }
            }
            EventKind::RightUp { pos, .. } => {
                self.state = State::Idle;
                Step::swallow().with(Effect::ReplayClick {
                    down_pos,
                    up_pos: pos,
                })
            }
            EventKind::HoldElapsed { time } => {
                let elapsed = time.wrapping_sub(down_time);
                if elapsed >= self.settings.hold_ms {
                    self.state = State::Open {
                        center: down_pos,
                        left_center: false,
                    };
                    Step::pass().with(Effect::OpenWheel { center: down_pos })
                } else {
                    // Too early (coarse timer or a timer from an earlier press): wait the rest.
                    Step::pass().with(Effect::StartHoldTimer(self.settings.hold_ms - elapsed))
                }
            }
            _ => Step::pass(),
        }
    }

    fn step_open(&mut self, center: Point, left_center: bool, kind: EventKind) -> Step {
        match kind {
            EventKind::Move { pos, .. } => {
                if !left_center && self.hit_at(center, pos) != Hit::Center {
                    self.state = State::Open {
                        center,
                        left_center: true,
                    };
                }
                Step::pass()
            }
            EventKind::RightUp { pos, .. } => match self.hit_at(center, pos) {
                // Back in the center after visiting the tiles: "Anuluj" closes the wheel.
                Hit::Center if left_center => {
                    self.state = State::Idle;
                    Step::swallow().with(Effect::Cancel)
                }
                Hit::Center => {
                    self.state = State::ClickMode { center };
                    Step::swallow().with(Effect::EnterClickMode)
                }
                hit => {
                    self.state = State::Idle;
                    Step::swallow().with(Effect::Select(hit))
                }
            },
            EventKind::Escape => {
                self.state = State::SwallowRightUp;
                Step::swallow().with(Effect::Cancel)
            }
            EventKind::OptionsKey => {
                self.state = State::SwallowRightUp;
                Step::swallow().with(Effect::OpenOptions)
            }
            EventKind::LeftDown { .. } | EventKind::LeftUp { .. } => Step::swallow(),
            _ => Step::pass(),
        }
    }

    fn step_click_mode(&mut self, center: Point, kind: EventKind) -> Step {
        match kind {
            EventKind::LeftDown { pos, .. } => {
                self.state = State::SwallowLeftUp;
                let dist_sq = pos.dist_sq(center) as f32;
                // A click selects only on the wheel (up to the outer ring boundary in the gap
                // before the arc band, as in the mockup) or on the arc band; further out it closes.
                let hit = self.hit_at(center, pos);
                let limit = match hit {
                    Hit::Sub(..) => self.layout.sub_click_limit(),
                    _ => self.layout.radii().outer,
                };
                match hit {
                    Hit::Center => Step::swallow().with(Effect::Cancel),
                    _ if dist_sq > limit * limit => Step::swallow().with(Effect::Cancel),
                    hit => Step::swallow().with(Effect::Select(hit)),
                }
            }
            EventKind::Escape => {
                self.state = State::Idle;
                Step::swallow().with(Effect::Cancel)
            }
            EventKind::OptionsKey => {
                self.state = State::Idle;
                Step::swallow().with(Effect::OpenOptions)
            }
            _ => Step::pass(),
        }
    }

    /// Hit test of `pos` relative to the wheel center; outside the wheel it is the outer sector
    /// in that direction.
    fn hit_at(&self, center: Point, pos: Point) -> Hit {
        let dx = (pos.x - center.x) as f32;
        let dy = (pos.y - center.y) as f32;
        hit_test_arc(
            dx,
            dy,
            &self.layout.radii(),
            self.layout.inner_sectors(),
            self.layout.outer_sectors(),
            self.arc,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: Point = Point { x: 500, y: 400 };

    fn trigger() -> Trigger {
        Trigger::new(TriggerSettings::default())
    }

    fn pt(dx: i32, dy: i32) -> Point {
        Point::new(P.x + dx, P.y + dy)
    }

    fn feed(t: &mut Trigger, kind: EventKind) -> Step {
        t.step(Event::physical(kind))
    }

    fn right_down(t: &mut Trigger, time: u32) -> Step {
        feed(t, EventKind::RightDown { pos: P, time })
    }

    fn right_up(t: &mut Trigger, pos: Point, time: u32) -> Step {
        feed(t, EventKind::RightUp { pos, time })
    }

    fn mv(t: &mut Trigger, pos: Point, time: u32) -> Step {
        feed(t, EventKind::Move { pos, time })
    }

    fn left_down(t: &mut Trigger, pos: Point) -> Step {
        feed(t, EventKind::LeftDown { pos, time: 0 })
    }

    fn left_up(t: &mut Trigger, pos: Point) -> Step {
        feed(t, EventKind::LeftUp { pos, time: 0 })
    }

    fn hold_elapsed(t: &mut Trigger, time: u32) -> Step {
        feed(t, EventKind::HoldElapsed { time })
    }

    fn open_wheel() -> Effect {
        Effect::OpenWheel { center: P }
    }

    /// Opens the wheel at `P` after a full hold.
    fn open(t: &mut Trigger) {
        right_down(t, 1000);
        let s = hold_elapsed(t, 1180);
        assert_eq!(s.effects, vec![open_wheel()]);
        t.set_layout(Layout::new(96, 100, 12, 12));
        assert!(t.is_wheel_open());
    }

    #[test]
    fn recover_swallows_the_release_of_a_swallowed_press() {
        for hold in [false, true] {
            let mut t = trigger();
            if hold {
                open(&mut t);
            } else {
                right_down(&mut t, 1000);
            }
            t.recover();
            assert!(!t.is_wheel_open());
            let s = right_up(&mut t, pt(60, 0), 1500);
            assert!(s.effects.is_empty(), "{:?}", s.effects);
            assert_eq!(s.verdict, Verdict::Swallow);
            // Only that one release.
            assert_eq!(right_up(&mut t, pt(60, 0), 1600).verdict, Verdict::Pass);
        }
    }

    #[test]
    fn recover_passes_the_release_of_a_passed_press() {
        let mut t = trigger();
        t.recover();
        assert_eq!(right_up(&mut t, P, 1500).verdict, Verdict::Pass);
        right_down(&mut t, 1000);
        let s = mv(&mut t, pt(30, 0), 1010);
        assert!(!s.effects.is_empty()); // a normal drag: the press is replayed
        t.recover();
        assert_eq!(right_up(&mut t, pt(30, 0), 1500).verdict, Verdict::Pass);
    }

    #[test]
    fn reset_while_open_makes_the_release_select_nothing() {
        let mut t = trigger();
        open(&mut t);
        t.reset();
        assert!(!t.is_wheel_open());
        let s = right_up(&mut t, pt(60, 0), 1500);
        assert!(s.effects.is_empty(), "{:?}", s.effects);
    }

    #[test]
    fn settings_default_and_clamp() {
        let d = TriggerSettings::default();
        assert_eq!((d.hold_ms, d.move_px), (180, 6));
        for (i, want) in [(0, 50), (49, 50), (50, 50), (2000, 2000), (2001, 2000)] {
            assert_eq!(clamp_hold_ms(i), want, "hold {i}");
        }
        for (i, want) in [(-5, 1), (0, 1), (1, 1), (50, 50), (51, 50)] {
            assert_eq!(clamp_move_px(i), want, "move {i}");
        }
        let c = TriggerSettings {
            hold_ms: 1,
            move_px: 999,
            ..TriggerSettings::default()
        }
        .clamped();
        assert_eq!((c.hold_ms, c.move_px), (50, 50));
        let t = Trigger::new(TriggerSettings {
            hold_ms: 1,
            move_px: 0,
            ..TriggerSettings::default()
        });
        assert_eq!(
            t.settings(),
            TriggerSettings {
                hold_ms: 50,
                move_px: 1,
                replace_right: false
            }
        );
    }

    #[test]
    fn idle_passes_everything_but_right_down() {
        let mut t = trigger();
        for kind in [
            EventKind::RightUp { pos: P, time: 0 },
            EventKind::Move { pos: P, time: 0 },
            EventKind::LeftDown { pos: P, time: 0 },
            EventKind::LeftUp { pos: P, time: 0 },
            EventKind::Escape,
            EventKind::OptionsKey,
            EventKind::HoldElapsed { time: 0 },
        ] {
            assert_eq!(feed(&mut t, kind), Step::pass(), "{kind:?}");
        }
    }

    #[test]
    fn short_click_replays_click() {
        let mut t = trigger();
        let s = right_down(&mut t, 1000);
        assert_eq!(s.verdict, Verdict::Swallow);
        assert_eq!(s.effects, vec![Effect::StartHoldTimer(180)]);
        let s = right_up(&mut t, pt(2, 1), 1050);
        assert_eq!(s.verdict, Verdict::Swallow);
        assert_eq!(
            s.effects,
            vec![Effect::ReplayClick {
                down_pos: P,
                up_pos: pt(2, 1)
            }]
        );
        assert!(!t.is_wheel_open());
        // The late timer is stale.
        assert_eq!(hold_elapsed(&mut t, 1180), Step::pass());
        assert!(!t.is_wheel_open());
    }

    #[test]
    fn hold_then_release_on_sector_selects() {
        let mut t = trigger();
        open(&mut t);
        let s = right_up(&mut t, pt(90, 0), 1500);
        assert_eq!(s.verdict, Verdict::Swallow);
        assert_eq!(s.effects, vec![Effect::Select(Hit::Inner(3))]);
        assert!(!t.is_wheel_open());
        assert_eq!(mv(&mut t, P, 0), Step::pass());
    }

    #[test]
    fn open_moves_pass() {
        let mut t = trigger();
        open(&mut t);
        assert_eq!(mv(&mut t, pt(300, 300), 1300), Step::pass());
        assert!(t.is_wheel_open());
    }

    #[test]
    fn drag_over_threshold_replays_drag() {
        let mut t = trigger();
        right_down(&mut t, 1000);
        let s = mv(&mut t, pt(7, 0), 1010);
        assert_eq!(s.verdict, Verdict::Pass);
        assert_eq!(
            s.effects,
            vec![Effect::ReplayDragStart {
                down_pos: P,
                current_pos: pt(7, 0)
            }]
        );
        // Everything passes while dragging, including a stale timer and Escape.
        assert_eq!(mv(&mut t, pt(50, 50), 1020), Step::pass());
        assert_eq!(hold_elapsed(&mut t, 1180), Step::pass());
        assert_eq!(feed(&mut t, EventKind::Escape), Step::pass());
        assert!(!t.is_wheel_open());
        assert_eq!(right_up(&mut t, pt(50, 50), 1100), Step::pass());
        // Idle again: a release passes and a new press is swallowed.
        assert_eq!(right_up(&mut t, P, 1200), Step::pass());
        assert_eq!(right_down(&mut t, 1300).verdict, Verdict::Swallow);
    }

    #[test]
    fn diagonal_drag_uses_distance() {
        let mut t = trigger();
        right_down(&mut t, 0);
        // 4,4 is 5.66 px: still pending. 5,5 is 7.07 px: drag.
        assert_eq!(mv(&mut t, pt(4, 4), 1), Step::pass());
        assert_eq!(mv(&mut t, pt(5, 5), 2).effects.len(), 1);
    }

    #[test]
    fn move_exactly_at_threshold_stays_pending() {
        let mut t = trigger();
        right_down(&mut t, 1000);
        assert_eq!(mv(&mut t, pt(6, 0), 1010), Step::pass());
        assert_eq!(mv(&mut t, pt(0, -6), 1011), Step::pass());
        let s = hold_elapsed(&mut t, 1180);
        assert_eq!(s.effects, vec![open_wheel()]);
    }

    #[test]
    fn stale_or_early_hold_elapsed_restarts_the_timer() {
        let mut t = trigger();
        right_down(&mut t, 1000);
        let s = hold_elapsed(&mut t, 1179);
        assert_eq!(s, Step::pass().with(Effect::StartHoldTimer(1)));
        assert!(!t.is_wheel_open());
        // A timer from an earlier press fires during a newer one.
        right_up(&mut t, P, 1100);
        right_down(&mut t, 1150);
        let s = hold_elapsed(&mut t, 1180);
        assert_eq!(s, Step::pass().with(Effect::StartHoldTimer(150)));
        assert!(!t.is_wheel_open());
        let s = hold_elapsed(&mut t, 1330);
        assert_eq!(s.effects, vec![open_wheel()]);
    }

    #[test]
    fn time_wraps_around() {
        let mut t = trigger();
        let start = u32::MAX - 50;
        right_down(&mut t, start);
        // 179 ms later is already past the wrap, and still too early.
        let s = hold_elapsed(&mut t, start.wrapping_add(179));
        assert_eq!(s, Step::pass().with(Effect::StartHoldTimer(1)));
        let s = hold_elapsed(&mut t, start.wrapping_add(180));
        assert_eq!(s.effects, vec![open_wheel()]);
    }

    #[test]
    fn release_in_center_after_visiting_tiles_cancels() {
        let mut t = trigger();
        open(&mut t);
        // Small moves inside the center do not count.
        assert_eq!(mv(&mut t, pt(10, 10), 1300), Step::pass());
        assert_eq!(mv(&mut t, pt(150, 0), 1350), Step::pass());
        assert_eq!(mv(&mut t, pt(5, 0), 1380), Step::pass());
        let s = right_up(&mut t, pt(5, 0), 1400);
        assert_eq!(s.verdict, Verdict::Swallow);
        assert_eq!(s.effects, vec![Effect::Cancel]);
        assert!(!t.is_wheel_open());
        // The release was the last event of the press: the next one starts afresh.
        right_down(&mut t, 2000);
        let s = hold_elapsed(&mut t, 2180);
        assert_eq!(s.effects, vec![open_wheel()]);
        let s = right_up(&mut t, pt(5, 5), 2200);
        assert_eq!(s.effects, vec![Effect::EnterClickMode]);
    }

    #[test]
    fn release_in_center_without_leaving_it_enters_click_mode() {
        let mut t = trigger();
        open(&mut t);
        assert_eq!(mv(&mut t, pt(20, -20), 1300), Step::pass());
        let s = right_up(&mut t, pt(20, -20), 1400);
        assert_eq!(s.effects, vec![Effect::EnterClickMode]);
        assert!(t.is_wheel_open());
    }

    #[test]
    fn release_in_center_enters_click_mode_then_left_click_selects() {
        let mut t = trigger();
        open(&mut t);
        let s = right_up(&mut t, pt(5, 5), 1400);
        assert_eq!(s.verdict, Verdict::Swallow);
        assert_eq!(s.effects, vec![Effect::EnterClickMode]);
        assert!(t.is_wheel_open());
        assert_eq!(mv(&mut t, pt(80, 0), 1410), Step::pass());
        let s = left_down(&mut t, pt(0, -90));
        assert_eq!(s.verdict, Verdict::Swallow);
        assert_eq!(s.effects, vec![Effect::Select(Hit::Inner(0))]);
        assert!(!t.is_wheel_open());
        assert_eq!(left_up(&mut t, pt(0, -90)), Step::swallow());
        // Back to idle: a further LeftUp passes.
        assert_eq!(left_up(&mut t, P), Step::pass());
    }

    #[test]
    fn click_mode_outer_ring_selects() {
        let mut t = trigger();
        open(&mut t);
        right_up(&mut t, P, 1400);
        let s = left_down(&mut t, pt(0, 180));
        assert_eq!(s.effects, vec![Effect::Select(Hit::Outer(6))]);
    }

    #[test]
    fn click_mode_left_click_outside_cancels() {
        let mut t = trigger();
        open(&mut t);
        right_up(&mut t, P, 1400);
        let s = left_down(&mut t, pt(400, 0));
        assert_eq!(s.verdict, Verdict::Swallow);
        assert_eq!(s.effects, vec![Effect::Cancel]);
        assert!(!t.is_wheel_open());
        assert_eq!(left_up(&mut t, pt(400, 0)), Step::swallow());
        assert_eq!(left_up(&mut t, P), Step::pass());
    }

    /// Arc of three tiles on outer sector 3 (pointing right at 12 sectors).
    const ARC: SubArc = SubArc {
        parent: 3,
        count: 3,
    };

    #[test]
    fn release_in_arc_band_selects_tile() {
        let mut t = trigger();
        open(&mut t);
        t.set_arc(Some(ARC));
        let s = right_up(&mut t, pt(250, 0), 1400);
        assert_eq!(s.effects, vec![Effect::Select(Hit::Sub(3, 1))]);
    }

    #[test]
    fn release_far_in_arc_direction_selects_tile() {
        let mut t = trigger();
        open(&mut t);
        t.set_arc(Some(ARC));
        let s = right_up(&mut t, pt(900, 0), 1400);
        assert_eq!(s.effects, vec![Effect::Select(Hit::Sub(3, 1))]);
    }

    #[test]
    fn release_outside_arc_angle_selects_outer_by_direction() {
        let mut t = trigger();
        open(&mut t);
        t.set_arc(Some(ARC));
        let s = right_up(&mut t, pt(0, -250), 1400);
        assert_eq!(s.effects, vec![Effect::Select(Hit::Outer(0))]);
    }

    #[test]
    fn release_without_arc_ignores_band() {
        let mut t = trigger();
        open(&mut t);
        let s = right_up(&mut t, pt(250, 0), 1400);
        assert_eq!(s.effects, vec![Effect::Select(Hit::Outer(3))]);
    }

    #[test]
    fn click_mode_arc_tile_selects() {
        let mut t = trigger();
        open(&mut t);
        right_up(&mut t, P, 1400);
        t.set_arc(Some(ARC));
        let s = left_down(&mut t, pt(250, 0));
        assert_eq!(s.effects, vec![Effect::Select(Hit::Sub(3, 1))]);
        // Just inside the outer edge of the band margin.
        let mut t = trigger();
        open(&mut t);
        right_up(&mut t, P, 1400);
        t.set_arc(Some(ARC));
        let s = left_down(&mut t, pt(295, 0));
        assert_eq!(s.effects, vec![Effect::Select(Hit::Sub(3, 1))]);
    }

    #[test]
    fn click_mode_gap_before_arc_band() {
        // 210..214: still the outer ring; 214..218 in the arc angle: the arc tile.
        for (arc, x, want) in [
            (Some(ARC), 212, Effect::Select(Hit::Outer(3))),
            (Some(ARC), 216, Effect::Select(Hit::Sub(3, 1))),
            (None, 212, Effect::Select(Hit::Outer(3))),
            (None, 216, Effect::Cancel),
        ] {
            let mut t = trigger();
            open(&mut t);
            right_up(&mut t, P, 1400);
            t.set_arc(arc);
            let s = left_down(&mut t, pt(x, 0));
            assert_eq!(s.effects, vec![want], "arc {arc:?}, x {x}");
        }
    }

    #[test]
    fn click_mode_beyond_arc_band_cancels() {
        let mut t = trigger();
        open(&mut t);
        right_up(&mut t, P, 1400);
        t.set_arc(Some(ARC));
        let s = left_down(&mut t, pt(300, 0));
        assert_eq!(s.effects, vec![Effect::Cancel]);
    }

    #[test]
    fn click_mode_outside_everything_cancels_with_arc() {
        let mut t = trigger();
        open(&mut t);
        right_up(&mut t, P, 1400);
        t.set_arc(Some(ARC));
        let s = left_down(&mut t, pt(0, -300));
        assert_eq!(s.effects, vec![Effect::Cancel]);
    }

    #[test]
    fn arc_is_forgotten_after_close() {
        let mut t = trigger();
        open(&mut t);
        t.set_arc(Some(ARC));
        right_up(&mut t, pt(250, 0), 1400);
        assert!(!t.is_wheel_open());
        // The next wheel starts without an arc, whatever the previous one showed.
        open(&mut t);
        let s = right_up(&mut t, pt(250, 0), 2400);
        assert_eq!(s.effects, vec![Effect::Select(Hit::Outer(3))]);
        // Also after a cancel and after reset.
        open(&mut t);
        t.set_arc(Some(ARC));
        feed(&mut t, EventKind::Escape);
        right_up(&mut t, P, 3000);
        open(&mut t);
        let s = right_up(&mut t, pt(250, 0), 4000);
        assert_eq!(s.effects, vec![Effect::Select(Hit::Outer(3))]);
        open(&mut t);
        t.set_arc(Some(ARC));
        t.reset();
        open(&mut t);
        let s = right_up(&mut t, pt(250, 0), 5000);
        assert_eq!(s.effects, vec![Effect::Select(Hit::Outer(3))]);
    }

    #[test]
    fn click_mode_left_click_in_center_cancels() {
        let mut t = trigger();
        open(&mut t);
        right_up(&mut t, P, 1400);
        let s = left_down(&mut t, pt(3, 3));
        assert_eq!(s.effects, vec![Effect::Cancel]);
    }

    #[test]
    fn click_mode_escape_cancels() {
        let mut t = trigger();
        open(&mut t);
        right_up(&mut t, P, 1400);
        let s = feed(&mut t, EventKind::Escape);
        assert_eq!(s.verdict, Verdict::Swallow);
        assert_eq!(s.effects, vec![Effect::Cancel]);
        assert!(!t.is_wheel_open());
        assert_eq!(right_up(&mut t, P, 1500), Step::pass());
    }

    #[test]
    fn click_mode_right_down_cancels_and_swallows_release() {
        let mut t = trigger();
        open(&mut t);
        right_up(&mut t, P, 1400);
        let s = right_down(&mut t, 1600);
        assert_eq!(s.verdict, Verdict::Swallow);
        assert_eq!(s.effects, vec![Effect::Cancel]);
        assert!(!t.is_wheel_open());
        assert_eq!(right_up(&mut t, P, 1700), Step::swallow());
        assert_eq!(right_up(&mut t, P, 1800), Step::pass());
    }

    #[test]
    fn escape_while_open_swallows_following_right_up() {
        let mut t = trigger();
        open(&mut t);
        let s = feed(&mut t, EventKind::Escape);
        assert_eq!(s.verdict, Verdict::Swallow);
        assert_eq!(s.effects, vec![Effect::Cancel]);
        assert!(!t.is_wheel_open());
        assert_eq!(mv(&mut t, pt(100, 0), 1500), Step::pass());
        assert_eq!(right_up(&mut t, pt(100, 0), 1600), Step::swallow());
        assert_eq!(right_up(&mut t, P, 1700), Step::pass());
    }

    #[test]
    fn left_buttons_swallowed_while_open() {
        let mut t = trigger();
        open(&mut t);
        assert_eq!(left_down(&mut t, P), Step::swallow());
        assert_eq!(left_up(&mut t, P), Step::swallow());
        assert!(t.is_wheel_open());
    }

    #[test]
    fn release_outside_wheel_selects_by_direction() {
        let mut t = trigger();
        open(&mut t);
        let s = right_up(&mut t, pt(-1000, 0), 1500);
        assert_eq!(s.verdict, Verdict::Swallow);
        assert_eq!(s.effects, vec![Effect::Select(Hit::Outer(9))]);
        assert!(!t.is_wheel_open());
    }

    #[test]
    fn release_uses_layout_outer_sectors() {
        let mut t = trigger();
        open(&mut t);
        t.set_layout(Layout::new(96, 100, 12, 4));
        let s = right_up(&mut t, pt(0, 300), 1500);
        assert_eq!(s.effects, vec![Effect::Select(Hit::Outer(2))]);
    }

    #[test]
    fn own_and_foreign_events_pass_without_state_change() {
        for origin in [Origin::Own, Origin::Foreign] {
            let mut t = trigger();
            let inject = |t: &mut Trigger, kind| t.step(Event { kind, origin });
            // Idle: an injected press does not start a cycle.
            let down = EventKind::RightDown { pos: P, time: 0 };
            assert_eq!(inject(&mut t, down), Step::pass());
            assert_eq!(hold_elapsed(&mut t, 500), Step::pass());
            assert!(!t.is_wheel_open());
            // Pending: injected move, release and press change nothing.
            right_down(&mut t, 1000);
            let far = EventKind::Move {
                pos: pt(100, 0),
                time: 1001,
            };
            assert_eq!(inject(&mut t, far), Step::pass());
            let up = EventKind::RightUp { pos: P, time: 1002 };
            assert_eq!(inject(&mut t, up), Step::pass());
            let down = EventKind::RightDown {
                pos: pt(9, 9),
                time: 1003,
            };
            assert_eq!(inject(&mut t, down), Step::pass());
            let s = hold_elapsed(&mut t, 1180);
            assert_eq!(s.effects, vec![open_wheel()]);
            // Open: injected Escape, release and left buttons change nothing.
            assert_eq!(inject(&mut t, EventKind::Escape), Step::pass());
            assert_eq!(inject(&mut t, up), Step::pass());
            let ldown = EventKind::LeftDown { pos: P, time: 1200 };
            assert_eq!(inject(&mut t, ldown), Step::pass());
            assert!(t.is_wheel_open());
            // Click mode: an injected left press does not select.
            right_up(&mut t, P, 1300);
            let ldown = EventKind::LeftDown {
                pos: pt(0, -90),
                time: 1300,
            };
            assert_eq!(inject(&mut t, ldown), Step::pass());
            assert!(t.is_wheel_open());
        }
    }

    #[test]
    fn right_down_while_pending_or_dragging_restarts() {
        let mut t = trigger();
        right_down(&mut t, 1000);
        let s = feed(
            &mut t,
            EventKind::RightDown {
                pos: pt(30, 0),
                time: 5000,
            },
        );
        assert_eq!(s.verdict, Verdict::Swallow);
        assert_eq!(s.effects, vec![Effect::StartHoldTimer(180)]);
        let s = hold_elapsed(&mut t, 5180);
        assert_eq!(s.effects, vec![Effect::OpenWheel { center: pt(30, 0) }]);

        let mut t = trigger();
        right_down(&mut t, 1000);
        mv(&mut t, pt(20, 0), 1010);
        let s = right_down(&mut t, 2000);
        assert_eq!(s.verdict, Verdict::Swallow);
        assert_eq!(s.effects, vec![Effect::StartHoldTimer(180)]);
        assert_eq!(right_up(&mut t, P, 2050).effects.len(), 1);
    }

    #[test]
    fn escape_passes_when_not_open() {
        let mut t = trigger();
        assert_eq!(feed(&mut t, EventKind::Escape), Step::pass());
        right_down(&mut t, 0);
        assert_eq!(feed(&mut t, EventKind::Escape), Step::pass());
        assert!(!t.is_wheel_open());
    }

    #[test]
    fn options_key_while_open_opens_options_and_swallows_release() {
        let mut t = trigger();
        open(&mut t);
        let s = feed(&mut t, EventKind::OptionsKey);
        assert_eq!(s, Step::swallow().with(Effect::OpenOptions));
        assert!(!t.is_wheel_open());
        assert_eq!(mv(&mut t, pt(100, 0), 1500), Step::pass());
        assert_eq!(right_up(&mut t, pt(100, 0), 1600), Step::swallow());
        assert_eq!(right_up(&mut t, P, 1700), Step::pass());
    }

    #[test]
    fn options_key_in_click_mode_opens_options() {
        let mut t = trigger();
        open(&mut t);
        right_up(&mut t, P, 1400);
        let s = feed(&mut t, EventKind::OptionsKey);
        assert_eq!(s, Step::swallow().with(Effect::OpenOptions));
        assert!(!t.is_wheel_open());
        assert_eq!(right_up(&mut t, P, 1500), Step::pass());
    }

    #[test]
    fn options_key_passes_when_not_open() {
        let mut t = trigger();
        assert_eq!(feed(&mut t, EventKind::OptionsKey), Step::pass());
        right_down(&mut t, 0);
        assert_eq!(feed(&mut t, EventKind::OptionsKey), Step::pass());
        mv(&mut t, pt(100, 0), 5);
        assert_eq!(feed(&mut t, EventKind::OptionsKey), Step::pass());
        right_up(&mut t, pt(100, 0), 10);
        // After the key closed the wheel and the release was swallowed, it passes again.
        open(&mut t);
        feed(&mut t, EventKind::OptionsKey);
        assert_eq!(feed(&mut t, EventKind::OptionsKey), Step::pass());
        right_up(&mut t, P, 2000);
        assert_eq!(feed(&mut t, EventKind::OptionsKey), Step::pass());
        assert!(!t.is_wheel_open());
    }

    #[test]
    fn options_key_from_own_and_foreign_origin_passes() {
        for origin in [Origin::Own, Origin::Foreign] {
            let mut t = trigger();
            open(&mut t);
            let step = t.step(Event {
                kind: EventKind::OptionsKey,
                origin,
            });
            assert_eq!(step, Step::pass());
            assert!(t.is_wheel_open());
            right_up(&mut t, P, 1400);
            let step = t.step(Event {
                kind: EventKind::OptionsKey,
                origin,
            });
            assert_eq!(step, Step::pass());
            assert!(t.is_wheel_open());
        }
    }

    #[test]
    fn replace_options_key_opens_options() {
        let mut t = replacing();
        right_down(&mut t, 1000);
        let s = feed(&mut t, EventKind::OptionsKey);
        assert_eq!(s, Step::swallow().with(Effect::OpenOptions));
        assert_eq!(right_up(&mut t, P, 1010), Step::swallow());
        assert_eq!(right_up(&mut t, P, 1020), Step::pass());

        let mut t = replacing();
        right_down(&mut t, 1000);
        right_up(&mut t, P, 1010);
        let s = feed(&mut t, EventKind::OptionsKey);
        assert_eq!(s, Step::swallow().with(Effect::OpenOptions));
        assert!(!t.is_wheel_open());
        assert_eq!(feed(&mut t, EventKind::OptionsKey), Step::pass());
    }

    #[test]
    fn reset_returns_to_idle() {
        let mut t = trigger();
        open(&mut t);
        t.reset();
        assert!(!t.is_wheel_open());
        assert_eq!(right_up(&mut t, P, 0), Step::pass());
    }

    #[test]
    fn custom_settings_are_used() {
        let mut t = Trigger::new(TriggerSettings {
            hold_ms: 300,
            move_px: 10,
            ..TriggerSettings::default()
        });
        let s = right_down(&mut t, 0);
        assert_eq!(s.effects, vec![Effect::StartHoldTimer(300)]);
        assert_eq!(mv(&mut t, pt(10, 0), 1), Step::pass());
        let s = hold_elapsed(&mut t, 299);
        assert_eq!(s, Step::pass().with(Effect::StartHoldTimer(1)));
        assert_eq!(hold_elapsed(&mut t, 300).effects, vec![open_wheel()]);
    }

    fn replacing() -> Trigger {
        let mut t = Trigger::new(TriggerSettings {
            replace_right: true,
            ..TriggerSettings::default()
        });
        t.set_layout(Layout::new(96, 100, 12, 12));
        t
    }

    #[test]
    fn replace_opens_on_press_and_swallows() {
        let mut t = replacing();
        let s = right_down(&mut t, 1000);
        assert_eq!(s, Step::swallow().with(open_wheel()));
        assert!(t.is_wheel_open());
    }

    #[test]
    fn replace_click_never_replays() {
        let mut t = replacing();
        right_down(&mut t, 1000);
        let s = right_up(&mut t, P, 1010);
        assert_eq!(s, Step::swallow().with(Effect::EnterClickMode));
        // Click mode: a left click on the centre cancels, nothing is replayed.
        let s = left_down(&mut t, P);
        assert_eq!(s.effects, vec![Effect::Cancel]);
    }

    #[test]
    fn replace_drag_never_replays() {
        let mut t = replacing();
        right_down(&mut t, 1000);
        // Far beyond any movement threshold: the wheel stays open, the move passes as usual.
        assert_eq!(mv(&mut t, pt(300, 0), 1010), Step::pass());
        assert!(t.is_wheel_open());
        let s = right_up(&mut t, pt(300, 0), 1020);
        assert!(matches!(s.effects.as_slice(), [Effect::Select(_)]));
        assert_eq!(s.verdict, Verdict::Swallow);
    }

    #[test]
    fn replace_release_over_tile_selects() {
        let mut t = replacing();
        right_down(&mut t, 1000);
        let s = right_up(&mut t, pt(0, -150), 1500);
        assert_eq!(s.verdict, Verdict::Swallow);
        assert!(matches!(
            s.effects.as_slice(),
            [Effect::Select(Hit::Outer(0))]
        ));
        assert!(!t.is_wheel_open());
    }

    #[test]
    fn replace_release_in_centre_after_tiles_cancels() {
        let mut t = replacing();
        right_down(&mut t, 1000);
        mv(&mut t, pt(0, -150), 1100);
        let s = right_up(&mut t, P, 1200);
        assert_eq!(s.effects, vec![Effect::Cancel]);
    }

    #[test]
    fn replace_escape_cancels_and_swallows_release() {
        let mut t = replacing();
        right_down(&mut t, 1000);
        let s = feed(&mut t, EventKind::Escape);
        assert_eq!(s, Step::swallow().with(Effect::Cancel));
        assert_eq!(right_up(&mut t, P, 1100), Step::swallow());
        assert!(!t.is_wheel_open());
    }

    #[test]
    fn replace_second_press_in_click_mode_cancels() {
        let mut t = replacing();
        right_down(&mut t, 1000);
        right_up(&mut t, P, 1010);
        let s = right_down(&mut t, 2000);
        assert_eq!(s, Step::swallow().with(Effect::Cancel));
        assert_eq!(right_up(&mut t, P, 2010), Step::swallow());
    }

    #[test]
    fn replace_exceptions_reset_and_pass() {
        // The app resets the machine and lets the press through for elevated windows, excluded
        // programs and full screen; the release then passes too.
        let mut t = replacing();
        right_down(&mut t, 1000);
        t.reset();
        assert_eq!(right_up(&mut t, P, 1010), Step::pass());
        assert!(!t.is_wheel_open());
    }

    #[test]
    fn replace_ignores_thresholds_and_default_mode_keeps_them() {
        let mut t = Trigger::new(TriggerSettings::default());
        let s = right_down(&mut t, 0);
        assert_eq!(s.effects, vec![Effect::StartHoldTimer(180)]);
    }
}
