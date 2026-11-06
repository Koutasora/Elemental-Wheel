//! Core logic of Elemental Wheel.

pub mod geometry;

pub use geometry::{Hit, Radii, SECTORS, angle_deg, hit_test, sector_at};
