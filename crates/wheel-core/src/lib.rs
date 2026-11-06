//! Core logic of Elemental Wheel.

pub mod demo;
pub mod geometry;
pub mod layout;

pub use demo::{INNER_LABELS, OUTER_LABELS, outer_labels};
pub use geometry::{
    DEFAULT_OUTER_SECTORS, Hit, INNER_SECTORS, MAX_OUTER_SECTORS, MIN_OUTER_SECTORS, Radii,
    angle_deg, clamp_outer_sectors, hit_test, sector_at,
};
pub use layout::{DEFAULT_SCALE_PCT, Layout, MAX_SCALE_PCT, MIN_SCALE_PCT, clamp_scale_pct};
