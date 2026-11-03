//! Everything about ErisMenus that doesn't need AppKit: where a menu goes on screen, how its
//! rows are laid out, its outline, its springs, and how key equivalents are written. `no_std`
//! like the payload that uses it, so it can run inside arm64e apps; the tests run with std.
#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod geometry;
pub mod keys;
pub mod layout;
pub mod math;
pub mod nav;
pub mod settings;
pub mod shape;
pub mod spring;

pub use geometry::{Point, Rect, Size};
