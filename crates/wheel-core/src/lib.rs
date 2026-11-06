//! Core logic of Elemental Wheel.

pub mod config;
pub mod geometry;
pub mod keys;
pub mod layout;
pub mod trigger;

pub use config::{
    Action, Config, EditCommand, Item, Loaded, Options, Source, SystemAction, ThemeChoice, ViewMode,
};
pub use geometry::{
    DEFAULT_OUTER_SECTORS, Hit, INNER_SECTORS, MAX_OUTER_SECTORS, MIN_OUTER_SECTORS, Radii,
    angle_deg, clamp_outer_sectors, hit_test, sector_at,
};
pub use keys::{Shortcut, is_extended, parse_shortcut};
pub use layout::{DEFAULT_SCALE_PCT, Layout, MAX_SCALE_PCT, MIN_SCALE_PCT, clamp_scale_pct};
pub use trigger::{
    DEFAULT_HOLD_MS, DEFAULT_MOVE_PX, Effect, Event, EventKind, MAX_HOLD_MS, MAX_MOVE_PX,
    MIN_HOLD_MS, MIN_MOVE_PX, Origin, Point, Step, Trigger, TriggerSettings, Verdict,
    clamp_hold_ms, clamp_move_px,
};
