//! Core logic of Elemental Wheel.

pub mod config;
pub mod fit;
pub mod geometry;
pub mod icon;
pub mod keys;
pub mod layout;
pub mod log;
pub mod look;
pub mod submenu;
pub mod trigger;

pub use config::{
    Action, Config, EditCommand, Item, Loaded, Options, Source, SystemAction, ThemeChoice, ViewMode,
};
pub use geometry::{
    DEFAULT_INNER_SECTORS, DEFAULT_OUTER_SECTORS, Hit, MAX_INNER_SECTORS, MAX_OUTER_SECTORS,
    MAX_SUB_TILES, MIN_INNER_SECTORS, MIN_OUTER_SECTORS, Radii, SUB_TILE_DEG, SubArc, angle_deg,
    clamp_inner_sectors, clamp_outer_sectors, hit_test, hit_test_arc, sector_at, sub_tile_span,
};
pub use keys::{Shortcut, is_extended, parse_shortcut};
pub use layout::{DEFAULT_SCALE_PCT, Layout, MAX_SCALE_PCT, MIN_SCALE_PCT, clamp_scale_pct};
pub use look::{DEFAULT_GLASS_PCT, MAX_GLASS_PCT, MIN_GLASS_PCT, clamp_glass_pct};
pub use submenu::{arc_for_hit, item_for_hit};
pub use trigger::{
    DEFAULT_HOLD_MS, DEFAULT_MOVE_PX, Effect, Event, EventKind, MAX_HOLD_MS, MAX_MOVE_PX,
    MIN_HOLD_MS, MIN_MOVE_PX, Origin, Point, Step, Trigger, TriggerSettings, Verdict,
    clamp_hold_ms, clamp_move_px,
};
