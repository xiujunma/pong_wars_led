//! Core Pong Wars simulation.
//!
//! ## Coordinate system
//!
//! The simulation lives in a logical pixel grid that is `PIXEL_SIZE` ×
//! `PIXEL_SIZE` pixels.  Cells are `SQUARE_SIZE_F` pixels wide so the grid is
//! `GRID_SIZE` × `GRID_SIZE` cells (each cell paints a `SQUARE_SIZE`×
//! `SQUARE_SIZE` block on the LED matrix).  Ball positions are stored as
//! floating-point pixels in `[0, PIXEL_SIZE)`.
//!
//! ## Why fixed sizes?
//!
//! The original WASM port stores cells in a `Vec<Vec<&str>>` keyed by canvas
//! pixel size.  On a microcontroller we know the panel up-front, the heap is
//! small, and `const`-sized arrays let us put the whole game state on the
//! stack with zero allocations.
//!
//! ## Determinism
//!
//! Randomness comes from an injected `oorandom::Rand32`.  The host tests use
//! a fixed seed so a regression in collision math will surface as a diff in
//! the recorded grid state rather than a flake.

use core::f32::consts::PI;

use libm::{cosf, fabsf, floorf, sinf};
use oorandom::Rand32;

use crate::color::Team;

/// Number of pixels along one side of the LED matrix.
pub const PIXEL_SIZE: usize = 64;

/// Pixels per cell.  Two pixels per cell on a 64-pixel display yields a
/// 32×32 logical grid, which preserves the chunky feel of the web demo while
/// leaving room for a visible ball.
pub const SQUARE_SIZE: usize = 2;

/// Same as [`SQUARE_SIZE`] but as `f32` so we don't pepper the math with
/// `as f32` conversions.
pub const SQUARE_SIZE_F: f32 = SQUARE_SIZE as f32;

/// Number of cells along one side of the grid.
pub const GRID_SIZE: usize = PIXEL_SIZE / SQUARE_SIZE;

/// Soft floor on horizontal/vertical ball speed (pixels per tick).  Below
/// this magnitude the ball gets "kicked" back up to avoid degenerate
/// trajectories that hug a wall forever.
pub const MIN_SPEED: f32 = 0.40;

/// Hard ceiling on ball speed.  Past ~1 px per tick the ball can teleport
/// past a cell without flipping it, which looks broken.
pub const MAX_SPEED: f32 = 0.90;

/// Per-tick jitter added to each velocity component (uniform on
/// `[-RANDOM_JITTER, RANDOM_JITTER]`).  Kept small so emergent motion looks
/// organic but the ball never visibly teleports.
pub const RANDOM_JITTER: f32 = 0.004;

/// Number of points sampled around the ball's perimeter when checking for
/// cell collisions.  Eight matches the original implementation
/// (`angle += PI / 4`).
pub const COLLISION_SAMPLES: u8 = 8;

/// A 2-D vector — used for both positions and velocities.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec2 {
    /// X component (pixels for positions, pixels/tick for velocities).
    pub x: f32,
    /// Y component.
    pub y: f32,
}

impl Vec2 {
    /// Construct a vector.
    #[inline]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// One of the two combatants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ball {
    /// Center position in logical pixels.
    pub pos: Vec2,
    /// Velocity in pixels per tick.
    pub vel: Vec2,
    /// Team this ball belongs to (which color it paints cells).
    pub team: Team,
}

impl Ball {
    /// The radius of the ball, equal to half a cell — same as the web port.
    pub const RADIUS: f32 = SQUARE_SIZE_F / 2.0;
}

/// Full simulation state — the grid + the two balls + an RNG.
pub struct PongWars {
    /// Row-major grid of owning teams.  `cells[y * GRID_SIZE + x]`.
    cells: [Team; GRID_SIZE * GRID_SIZE],
    /// The two competing balls.  Slot 0 is day, slot 1 is night.
    balls: [Ball; 2],
    /// Deterministic PRNG used for jitter.
    rng: Rand32,
}

impl PongWars {
    /// Create a fresh game seeded with `seed`.
    ///
    /// `seed` controls only the per-tick jitter; the initial layout and
    /// velocities are fixed so two engines started with the same seed will
    /// trace identical histories.
    pub fn new(seed: u64) -> Self {
        let mut cells = [Team::Day; GRID_SIZE * GRID_SIZE];
        // Left half day, right half night — matches the WASM port's seed
        // pattern (it filled column-wise, but on a square grid the result is
        // identical: a vertical split).
        for y in 0..GRID_SIZE {
            for x in (GRID_SIZE / 2)..GRID_SIZE {
                cells[y * GRID_SIZE + x] = Team::Night;
            }
        }

        let half = PIXEL_SIZE as f32 / 2.0;
        let quarter = PIXEL_SIZE as f32 / 4.0;
        // Starting velocities scaled down for our smaller field — the WASM
        // version used ±12.5 px on an 800-pixel canvas (12.5/800 ≈ 1.6 %).
        // 0.6 px on a 64-pixel field is ~0.9 %; tuned by eye to feel right.
        let init_speed = 0.6;

        let balls = [
            Ball {
                pos: Vec2::new(quarter, half),
                vel: Vec2::new(init_speed, -init_speed),
                team: Team::Day,
            },
            Ball {
                pos: Vec2::new(quarter * 3.0, half),
                vel: Vec2::new(-init_speed, init_speed),
                team: Team::Night,
            },
        ];

        Self { cells, balls, rng: Rand32::new(seed) }
    }

    /// All cells, row-major.  Useful for renderers that want to bulk-blit.
    #[inline]
    pub fn cells(&self) -> &[Team; GRID_SIZE * GRID_SIZE] {
        &self.cells
    }

    /// Both balls — `[day, night]`.
    #[inline]
    pub fn balls(&self) -> &[Ball; 2] {
        &self.balls
    }

    /// Owning team of a single cell.
    ///
    /// Returns `None` if the coordinates are off-grid; this is the only
    /// boundary check callers need.
    #[inline]
    pub fn cell(&self, x: usize, y: usize) -> Option<Team> {
        if x >= GRID_SIZE || y >= GRID_SIZE {
            return None;
        }
        Some(self.cells[y * GRID_SIZE + x])
    }

    /// Count of cells owned by each team — `(day, night)`.
    pub fn score(&self) -> (u32, u32) {
        let mut day = 0u32;
        let mut night = 0u32;
        for &t in self.cells.iter() {
            match t {
                Team::Day => day += 1,
                Team::Night => night += 1,
            }
        }
        (day, night)
    }

    /// Advance the simulation by one frame.
    ///
    /// Order matches the WASM port: collide → bounce off walls → move →
    /// jitter.  Doing all of this in one pass means a follow-up render call
    /// sees the new positions on the new cell map.
    pub fn tick(&mut self) {
        for i in 0..self.balls.len() {
            self.check_square_collision(i);
            self.check_boundary_collision(i);
            self.update_ball(i);
            self.add_randomness(i);
        }
    }

    // ---- private helpers --------------------------------------------------

    /// Sample 8 points on the ball's perimeter; any sampled cell that's not
    /// our team gets flipped, and the appropriate velocity component is
    /// negated to reflect off it.
    fn check_square_collision(&mut self, ball_index: usize) {
        // Copy ball so the borrow checker lets us write `self.cells` below.
        let mut ball = self.balls[ball_index];
        let reverse = ball.team; // the color we're flipping cells to

        let mut angle: f32 = 0.0;
        let step = PI / (COLLISION_SAMPLES as f32 / 2.0); // PI/4 for 8 samples
        for _ in 0..COLLISION_SAMPLES {
            let check_x = ball.pos.x + cosf(angle) * Ball::RADIUS;
            let check_y = ball.pos.y + sinf(angle) * Ball::RADIUS;

            let i = floorf(check_x / SQUARE_SIZE_F) as isize;
            let j = floorf(check_y / SQUARE_SIZE_F) as isize;

            if (0..GRID_SIZE as isize).contains(&i) && (0..GRID_SIZE as isize).contains(&j) {
                let idx = (j as usize) * GRID_SIZE + (i as usize);
                if self.cells[idx] != reverse {
                    self.cells[idx] = reverse;
                    if fabsf(cosf(angle)) > fabsf(sinf(angle)) {
                        ball.vel.x = -ball.vel.x;
                    } else {
                        ball.vel.y = -ball.vel.y;
                    }
                }
            }
            angle += step;
        }

        self.balls[ball_index] = ball;
    }

    /// Reflect the ball off the panel edges.  The check is on the *next*
    /// position so the ball never crosses the wall before bouncing.
    fn check_boundary_collision(&mut self, ball_index: usize) {
        let ball = &mut self.balls[ball_index];
        let next_x = ball.pos.x + ball.vel.x;
        let next_y = ball.pos.y + ball.vel.y;
        let pixel_size_f = PIXEL_SIZE as f32;

        if next_x > pixel_size_f - Ball::RADIUS || next_x < Ball::RADIUS {
            ball.vel.x = -ball.vel.x;
        }
        if next_y > pixel_size_f - Ball::RADIUS || next_y < Ball::RADIUS {
            ball.vel.y = -ball.vel.y;
        }
    }

    /// Integrate one step.
    fn update_ball(&mut self, ball_index: usize) {
        let ball = &mut self.balls[ball_index];
        ball.pos.x += ball.vel.x;
        ball.pos.y += ball.vel.y;
    }

    /// Add a tiny uniform perturbation to the velocity, then clamp to the
    /// `[MIN_SPEED, MAX_SPEED]` magnitude band.  Without this the simulation
    /// converges to a periodic orbit and the score stops changing.
    fn add_randomness(&mut self, ball_index: usize) {
        let ball = &mut self.balls[ball_index];
        let jitter_x = (self.rng.rand_float() * 2.0 - 1.0) * RANDOM_JITTER;
        let jitter_y = (self.rng.rand_float() * 2.0 - 1.0) * RANDOM_JITTER;

        ball.vel.x = clamp(ball.vel.x + jitter_x, -MAX_SPEED, MAX_SPEED);
        ball.vel.y = clamp(ball.vel.y + jitter_y, -MAX_SPEED, MAX_SPEED);

        // Floor at `MIN_SPEED` while preserving the ball's sign.  We can't
        // use `libm::copysignf` because per IEEE 754 it always returns
        // `+MIN_SPEED` when the input is `+0.0`/`-0.0` (sign of zero, not
        // value of zero), which would lose the ball's direction.  The
        // explicit `if/else` below is bit-exact for every f32 input.
        if fabsf(ball.vel.x) < MIN_SPEED {
            ball.vel.x = if ball.vel.x < 0.0 { -MIN_SPEED } else { MIN_SPEED };
        }
        if fabsf(ball.vel.y) < MIN_SPEED {
            ball.vel.y = if ball.vel.y < 0.0 { -MIN_SPEED } else { MIN_SPEED };
        }
    }
}

// ---- small numeric helpers -------------------------------------------------

#[inline]
fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

// ---- tests ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_split_is_half_and_half() {
        let game = PongWars::new(1);
        let (day, night) = game.score();
        assert_eq!(day, night, "initial board should be balanced");
        assert_eq!(day + night, (GRID_SIZE * GRID_SIZE) as u32);
    }

    #[test]
    fn initial_balls_are_on_opposite_quarters() {
        let game = PongWars::new(1);
        let [day_ball, night_ball] = *game.balls();
        assert_eq!(day_ball.team, Team::Day);
        assert_eq!(night_ball.team, Team::Night);
        assert!(day_ball.pos.x < night_ball.pos.x);
        // Both start vertically centered.
        assert!((day_ball.pos.y - night_ball.pos.y).abs() < 1e-6);
    }

    #[test]
    fn tick_does_not_change_total_cell_count() {
        let mut game = PongWars::new(42);
        let total_before: u32 = {
            let (d, n) = game.score();
            d + n
        };
        for _ in 0..500 {
            game.tick();
        }
        let (d_after, n_after) = game.score();
        assert_eq!(d_after + n_after, total_before);
    }

    #[test]
    fn ball_stays_inside_the_grid() {
        let mut game = PongWars::new(7);
        for _ in 0..10_000 {
            game.tick();
            for ball in game.balls() {
                assert!(ball.pos.x >= 0.0 && ball.pos.x <= PIXEL_SIZE as f32,
                    "ball escaped on x: {:?}", ball);
                assert!(ball.pos.y >= 0.0 && ball.pos.y <= PIXEL_SIZE as f32,
                    "ball escaped on y: {:?}", ball);
            }
        }
    }

    #[test]
    fn ball_speed_stays_bounded() {
        let mut game = PongWars::new(99);
        for _ in 0..5_000 {
            game.tick();
            for ball in game.balls() {
                assert!(ball.vel.x.abs() <= MAX_SPEED + 1e-3);
                assert!(ball.vel.y.abs() <= MAX_SPEED + 1e-3);
                assert!(ball.vel.x.abs() >= MIN_SPEED - 1e-3);
                assert!(ball.vel.y.abs() >= MIN_SPEED - 1e-3);
            }
        }
    }

    #[test]
    fn determinism_same_seed_same_history() {
        let mut a = PongWars::new(2024);
        let mut b = PongWars::new(2024);
        for _ in 0..1_000 {
            a.tick();
            b.tick();
        }
        assert_eq!(a.score(), b.score());
        assert_eq!(a.balls(), b.balls());
    }

    #[test]
    fn cell_out_of_bounds_returns_none() {
        let game = PongWars::new(0);
        assert!(game.cell(GRID_SIZE, 0).is_none());
        assert!(game.cell(0, GRID_SIZE).is_none());
        assert!(game.cell(GRID_SIZE * 2, GRID_SIZE * 2).is_none());
        assert!(game.cell(0, 0).is_some());
    }

    #[test]
    fn one_side_eventually_makes_progress() {
        // Loose smoke test — after a few thousand ticks the board should
        // no longer be exactly 50/50; if it is, the game has stalled.
        let mut game = PongWars::new(123);
        let (start_day, _) = game.score();
        for _ in 0..3_000 {
            game.tick();
        }
        let (end_day, _) = game.score();
        assert_ne!(start_day, end_day, "no team made any progress in 3000 ticks");
    }
}
