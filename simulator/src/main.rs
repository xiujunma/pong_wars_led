//! Desktop simulator for the 64×64 HUB75 LED matrix firmware.
//!
//! Renders the exact same [`pong_wars_core::PongWars`] simulation the
//! firmware does, but to a regular OS window via [`minifb`].  Each
//! simulated LED is drawn as a small circular dot on a near-black
//! background, mimicking the look of a real HUB75 panel.
//!
//! ## Run
//!
//! ```bash
//! cargo run --release -p pong-wars-sim
//! ```
//!
//! ## Keys
//!
//! - `Space` — pause / resume
//! - `R`     — reset the simulation with a fresh seed
//! - `+` / `-` — speed up / slow down (1, 2, 4, 8, 16 ticks per frame)
//! - `ESC`   — quit
//!
//! ## Window layout
//!
//! Each LED is rendered as a [`LED_SIZE`]×[`LED_SIZE`] cell with a circular
//! dot of diameter [`LED_DIAMETER`] centered inside it.  The dot's color is
//! the simulation's pixel color at that grid position; the cell's background
//! is [`BACKGROUND`] (a very dark grey, to suggest unpowered LED bezels).

use std::time::SystemTime;

use minifb::{Key, KeyRepeat, Scale, ScaleMode, Window, WindowOptions};
use pong_wars_core::{Ball, Palette, PongWars, GRID_SIZE, PIXEL_SIZE, SQUARE_SIZE_F};

// ---- visual tuning --------------------------------------------------------

/// Pixels per simulated LED in the output window.  12 is a comfortable size
/// on a typical 1080p display — the whole 64×64 panel ends up ~768 px wide.
const LED_SIZE: usize = 12;

/// Diameter of the lit dot inside each LED cell.  Slightly smaller than
/// `LED_SIZE` so the LEDs read as discrete dots, not as a solid wall.
const LED_DIAMETER: usize = 10;

/// Width and height of the rendered window in pixels.
const WINDOW_W: usize = PIXEL_SIZE * LED_SIZE;
const WINDOW_H: usize = PIXEL_SIZE * LED_SIZE;

/// Background color of each cell — ARGB.  A near-black neutral grey reads
/// as "LED that's off but visible" rather than the void.
const BACKGROUND: u32 = 0xFF_08_08_08;

// ---- main loop ------------------------------------------------------------

fn main() {
    let mut window = Window::new(
        "Pong Wars — 64×64 LED simulator",
        WINDOW_W,
        WINDOW_H,
        WindowOptions {
            // Scale=X1 because we already upscale via `LED_SIZE`; letting
            // the OS scale on top of that produces blurry, non-square dots.
            scale: Scale::X1,
            scale_mode: ScaleMode::Stretch,
            resize: false,
            ..WindowOptions::default()
        },
    )
    .expect("failed to open simulator window");

    // 60 FPS cap.  The simulation step itself is way faster than 16 ms; the
    // cap is purely so we don't pin a CPU core for nothing.
    window.set_target_fps(60);

    let mut framebuffer = vec![BACKGROUND; WINDOW_W * WINDOW_H];
    let palette = Palette::classic();
    let mut game = PongWars::new(initial_seed());
    let mut paused = false;
    let mut ticks_per_frame: u32 = 1;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        handle_input(&window, &mut paused, &mut ticks_per_frame, &mut game);

        if !paused {
            for _ in 0..ticks_per_frame {
                game.tick();
            }
        }

        draw_frame(&mut framebuffer, &game, &palette);

        // Reflect the current state in the title bar so we don't need an
        // overlay font — score + status fits there comfortably.
        let (day, night) = game.score();
        let status = if paused { " (paused)" } else { "" };
        window.set_title(&format!(
            "Pong Wars — day {day} / night {night} — {ticks_per_frame}x{status}"
        ));

        window
            .update_with_buffer(&framebuffer, WINDOW_W, WINDOW_H)
            .expect("failed to update window");
    }
}

// ---- input ----------------------------------------------------------------

fn handle_input(
    window: &Window,
    paused: &mut bool,
    ticks_per_frame: &mut u32,
    game: &mut PongWars,
) {
    if window.is_key_pressed(Key::Space, KeyRepeat::No) {
        *paused = !*paused;
    }
    if window.is_key_pressed(Key::R, KeyRepeat::No) {
        *game = PongWars::new(initial_seed());
    }
    if window.is_key_pressed(Key::Equal, KeyRepeat::No)
        || window.is_key_pressed(Key::NumPadPlus, KeyRepeat::No)
    {
        // Cap at 16x — beyond that the cells flip too fast to follow.
        *ticks_per_frame = (*ticks_per_frame * 2).min(16);
    }
    if window.is_key_pressed(Key::Minus, KeyRepeat::No)
        || window.is_key_pressed(Key::NumPadMinus, KeyRepeat::No)
    {
        *ticks_per_frame = (*ticks_per_frame / 2).max(1);
    }
}

/// Seed derived from current wall-clock time so each fresh launch /
/// `R`-reset diverges into a new game.  For a fully deterministic preview
/// (e.g. to reproduce a specific trajectory) replace the body with a
/// constant.
fn initial_seed() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        // Fallback to a non-zero constant if the system clock can't be read;
        // we just need *some* seed, the simulation tolerates any u64.
        .unwrap_or(0xA11C_E0DE)
}

// ---- rendering ------------------------------------------------------------

/// Paint one frame into the ARGB pixel buffer.
fn draw_frame(buffer: &mut [u32], game: &PongWars, palette: &Palette) {
    // Build a 64×64 "LED color" map first — this lets us draw cells + balls
    // by writing a single color per LED, then expand each LED to its
    // upscaled cell in one pass.
    let mut led: [u32; PIXEL_SIZE * PIXEL_SIZE] = [BACKGROUND; PIXEL_SIZE * PIXEL_SIZE];

    fill_cells(&mut led, game, palette);
    fill_balls(&mut led, game, palette);

    expand_to_window(buffer, &led);
}

/// Paint every cell as a `SQUARE_SIZE`×`SQUARE_SIZE` block on the LED map.
fn fill_cells(led: &mut [u32; PIXEL_SIZE * PIXEL_SIZE], game: &PongWars, palette: &Palette) {
    let square = SQUARE_SIZE_F as usize;
    for cy in 0..GRID_SIZE {
        for cx in 0..GRID_SIZE {
            let team = game.cell(cx, cy).expect("in-bounds cell");
            let color = rgb565_to_argb(palette.cell_color(team));
            for dy in 0..square {
                let row = (cy * square + dy) * PIXEL_SIZE;
                for dx in 0..square {
                    led[row + cx * square + dx] = color;
                }
            }
        }
    }
}

/// Paint both balls.  Each ball is a small filled circle (radius =
/// `Ball::RADIUS`) on top of whatever cell colors we already drew.
fn fill_balls(led: &mut [u32; PIXEL_SIZE * PIXEL_SIZE], game: &PongWars, palette: &Palette) {
    for ball in game.balls() {
        let color = rgb565_to_argb(palette.ball_color(ball.team));
        rasterize_filled_circle(led, ball, color);
    }
}

/// Naive circle rasterizer — perfect for our tiny radius (1 px on a 32×32
/// logical grid).  Iterates the bounding box and includes any LED whose
/// center is within the ball's radius from the ball's position.
fn rasterize_filled_circle(
    led: &mut [u32; PIXEL_SIZE * PIXEL_SIZE],
    ball: &Ball,
    color: u32,
) {
    let radius = Ball::RADIUS;
    let r2 = radius * radius;
    let min_x = ((ball.pos.x - radius).floor() as i32).max(0) as usize;
    let max_x = ((ball.pos.x + radius).ceil() as i32).min(PIXEL_SIZE as i32 - 1) as usize;
    let min_y = ((ball.pos.y - radius).floor() as i32).max(0) as usize;
    let max_y = ((ball.pos.y + radius).ceil() as i32).min(PIXEL_SIZE as i32 - 1) as usize;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let cx = x as f32 + 0.5;
            let cy = y as f32 + 0.5;
            let dx = cx - ball.pos.x;
            let dy = cy - ball.pos.y;
            if dx * dx + dy * dy <= r2 {
                led[y * PIXEL_SIZE + x] = color;
            }
        }
    }
}

/// Expand the 64×64 LED color map into the window buffer.  Each logical LED
/// becomes a `LED_SIZE`×`LED_SIZE` cell with a `LED_DIAMETER`-wide filled
/// dot of the LED's color in its center; the rest of the cell stays
/// [`BACKGROUND`].  The result reads like discrete LEDs on a dark mask.
fn expand_to_window(buffer: &mut [u32], led: &[u32; PIXEL_SIZE * PIXEL_SIZE]) {
    debug_assert_eq!(buffer.len(), WINDOW_W * WINDOW_H);

    // Pre-compute the per-pixel mask for one LED cell: 1 if the pixel is
    // inside the LED's circular footprint, 0 otherwise.  We only need to
    // build this once because every cell shares the same geometry.
    let mask = led_cell_mask();

    for ly in 0..PIXEL_SIZE {
        for lx in 0..PIXEL_SIZE {
            let color = led[ly * PIXEL_SIZE + lx];
            let cell_origin_x = lx * LED_SIZE;
            let cell_origin_y = ly * LED_SIZE;

            for py in 0..LED_SIZE {
                let row = (cell_origin_y + py) * WINDOW_W + cell_origin_x;
                for px in 0..LED_SIZE {
                    let lit = mask[py * LED_SIZE + px];
                    buffer[row + px] = if lit { color } else { BACKGROUND };
                }
            }
        }
    }
}

/// Build a `LED_SIZE × LED_SIZE` boolean mask of which pixels in one LED's
/// cell belong to the lit circle.  Cached behind a `OnceLock` so we only
/// rasterise it once for the lifetime of the program.
fn led_cell_mask() -> &'static [bool] {
    use std::sync::OnceLock;
    static MASK: OnceLock<Vec<bool>> = OnceLock::new();
    MASK.get_or_init(|| {
        let mut m = vec![false; LED_SIZE * LED_SIZE];
        let center = LED_SIZE as f32 / 2.0;
        let radius = LED_DIAMETER as f32 / 2.0;
        let r2 = radius * radius;
        for y in 0..LED_SIZE {
            for x in 0..LED_SIZE {
                let dx = x as f32 + 0.5 - center;
                let dy = y as f32 + 0.5 - center;
                if dx * dx + dy * dy <= r2 {
                    m[y * LED_SIZE + x] = true;
                }
            }
        }
        m
    })
    .as_slice()
}

/// Convert one of the core crate's packed RGB565 entries into an ARGB
/// 32-bit word — the format minifb's pixel buffer expects.
fn rgb565_to_argb(c: pong_wars_core::color::Rgb565) -> u32 {
    // RGB565 → 8-bit per channel by left-shifting and copying the high bits
    // into the low ones (the standard "fill the missing bits" trick that
    // avoids banding at the bright/dark extremes).
    let r5 = ((c >> 11) & 0x1F) as u32;
    let g6 = ((c >> 5) & 0x3F) as u32;
    let b5 = (c & 0x1F) as u32;
    let r = (r5 << 3) | (r5 >> 2);
    let g = (g6 << 2) | (g6 >> 4);
    let b = (b5 << 3) | (b5 >> 2);
    0xFF00_0000 | (r << 16) | (g << 8) | b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb565_white_round_trips_close_to_white() {
        // 0xFFFF in RGB565 is pure white; our converter should hit 0xFF,0xFF,0xFF.
        let argb = rgb565_to_argb(0xFFFF);
        assert_eq!(argb, 0xFFFF_FFFF);
    }

    #[test]
    fn rgb565_black_is_black() {
        assert_eq!(rgb565_to_argb(0x0000), 0xFF00_0000);
    }

    #[test]
    fn led_mask_has_about_the_right_lit_count() {
        let mask = led_cell_mask();
        let lit = mask.iter().filter(|&&b| b).count();
        // Area of a 10-wide circle is ~78.5; a discrete rasterisation in a
        // 12×12 cell should land within a handful of pixels of that.
        assert!(
            (70..=90).contains(&lit),
            "LED dot mask had {lit} lit pixels, expected ~78"
        );
    }
}
