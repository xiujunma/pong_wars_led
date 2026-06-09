//! Headless port of [`vnglst/pong-wars`] suitable for `no_std` targets.
//!
//! Two opposing balls fight over a square grid: each ball flips the cell it
//! touches to its own team's color and bounces off cells that are still its
//! opponent's color (and off the walls).  The original [WASM crate][wasm]
//! drew straight to an HTML canvas; this crate exposes only the simulation
//! state — rendering is the embedder's job.
//!
//! [`vnglst/pong-wars`]: https://github.com/vnglst/pong-wars
//! [wasm]: https://github.com/maxj/wasmhub-dev/tree/main/pong_wars.rs

#![cfg_attr(not(test), no_std)]
#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod color;
pub mod game;

pub use color::{Palette, Team, TEAM_DAY, TEAM_NIGHT};
pub use game::{Ball, PongWars, Vec2, GRID_SIZE, PIXEL_SIZE, SQUARE_SIZE_F};
