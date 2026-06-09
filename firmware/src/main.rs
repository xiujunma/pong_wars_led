//! ESP32 firmware that runs the Pong Wars simulation on a 64×64 HUB75 LED
//! matrix using the I2S-parallel peripheral via [`esp_hub75`].
//!
//! ## Target
//!
//! Original **ESP32** (Xtensa LX6) on a DevKitC-1-class board.  ESP32-S3
//! uses the LCD_CAM driver instead — see `git log` for the S3 variant.
//!
//! ## Wiring (default; matches the official esp-hub75 I2S example)
//!
//! | HUB75 signal | GPIO | HUB75 signal   | GPIO |
//! |--------------|------|----------------|------|
//! | R1           | 16   | A (addr0)      | 15   |
//! | G1           | 4    | B (addr1)      | 13   |
//! | B1           | 17   | C (addr2)      | 12   |
//! | R2           | 18   | D (addr3)      | 14   |
//! | G2           | 5    | E (addr4)      | 2    |
//! | B2           | 19   | LAT            | 26   |
//! | CLK          | 27   | OE / BLANK     | 25   |
//!
//! All GND pins on the matrix connector go to any GND on the ESP32 *and* to
//! the PSU's `-`.  Matrix `+5V` to the PSU's `+` (do **not** power from USB).
//!
//! ## Notes on the strapping pins
//!
//! The default map reuses GPIO 2, 12, and 15, which are strapping pins
//! (they influence boot mode / flash voltage at reset).  Once the I2S
//! peripheral takes over the pins, the strapping role is released, so this
//! is safe in practice — the upstream `esp-hub75` examples use the same
//! assignment.  Re-map the HUB75 signals to any output-capable,
//! non-strapping GPIO (e.g. 32, 33) in the `Hub75Pins16 { ... }` block if
//! you'd rather avoid them.
//!
//! ## What it does
//!
//! 1. Initialises the ESP32 at full CPU clock, esp-alloc heap, panic
//!    handler.
//! 2. Configures the HUB75 driver in 16-bit I2S-parallel mode at 20 MHz
//!    pixel clock (the value used by the upstream example).
//! 3. Constructs a `pong_wars_core::PongWars` simulation.
//! 4. Loops forever: tick the simulation, paint it to the framebuffer, send
//!    the framebuffer to the panel.

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use embedded_graphics::geometry::{Point, Size};
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::Pin;
use esp_hal::main;
use esp_hal::time::Rate;
use esp_hub75::framebuffer::bitplane::plain::DmaFrameBuffer;
use esp_hub75::framebuffer::compute_rows;
use esp_hub75::{Hub75, Hub75Pins16};

use pong_wars_core::{Ball, PongWars, GRID_SIZE, SQUARE_SIZE_F};

// esp-bootloader-esp-idf needs this app descriptor so the second-stage
// bootloader can validate our image at flash time.
esp_bootloader_esp_idf::esp_app_desc!();

// ---- display geometry -----------------------------------------------------

/// Rows on the HUB75 panel.  64 means the addressing lines must drive A–E
/// (32 unique address values × 2 halves = 64 rows).
const ROWS: usize = 64;
/// Columns on the HUB75 panel.
const COLS: usize = 64;
/// Address scan rows = ROWS / 2 — [`compute_rows`] expresses this in the
/// form esp-hub75 wants.
const NROWS: usize = compute_rows(ROWS);
/// Color depth bitplanes; 7 → 7-bit (128-step) brightness per channel.
/// 14 KB of framebuffer + descriptors fits comfortably in the WROOM-32's
/// 320 KB SRAM even without PSRAM.
const PLANES: usize = 7;

/// Type alias so the framebuffer's verbose generics only appear in one place.
type FrameBuffer = DmaFrameBuffer<NROWS, COLS, PLANES>;

/// Game ticks per render frame.  The HUB75 driver pushes a new frame as
/// fast as DMA completes (~5–8 ms each, so ~150 Hz), but the game state
/// only advances on every `TICK_DIVISOR`-th render.  This is the single
/// knob to slow the game down or speed it up:
///
///   * `1` — tick every frame (fastest; original behaviour)
///   * `3` — ~50 Hz ticks — comfortably watchable
///   * `5` — ~30 Hz ticks — relaxed, gives the eye time to track
///   * `10` — slow-motion, useful for debugging the simulation
const TICK_DIVISOR: u32 = 4;

// ---- entry point ----------------------------------------------------------

#[main]
fn main() -> ! {
    // 240 MHz is the top of the LX6 clock band.  Bitplane assembly happens
    // on the CPU, so headroom matters.
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    // esp-hub75 generates the DMA descriptor table sized to our concrete
    // framebuffer type — the macro is the supported way to get the count
    // right.
    let tx_descriptors = esp_hub75::hub75_dma_descriptors!(FrameBuffer);

    // Take each pin out of the `Peripherals` singleton by value and erase
    // its type so the driver's 16-bit pin array is homogeneous.  This is
    // the same pattern used by the canonical `hello_i2s_parallel.rs`
    // example for the original ESP32.
    //
    // NOTE: Reverted to the strapping-pin (GPIO 15, 12, 2) address-line
    // pinout because the user's adapter PCB only exposes those pins — the
    // remap to GPIO 33/22/23 left the panel dark.  The strapping-pin
    // glitch is a known cosmetic issue on some panels; the working fix is
    // physical (move the LAT/OE wires, or flip the IDC ribbon) rather
    // than a pinout change.
    let pins = Hub75Pins16 {
        red1:  peripherals.GPIO16.degrade(),
        grn1:  peripherals.GPIO4.degrade(),
        blu1:  peripherals.GPIO17.degrade(),
        red2:  peripherals.GPIO18.degrade(),
        grn2:  peripherals.GPIO5.degrade(),
        blu2:  peripherals.GPIO19.degrade(),
        addr0: peripherals.GPIO15.degrade(),
        addr1: peripherals.GPIO13.degrade(),
        addr2: peripherals.GPIO12.degrade(),
        addr3: peripherals.GPIO14.degrade(),
        addr4: peripherals.GPIO2.degrade(),
        blank: peripherals.GPIO25.degrade(),
        clock: peripherals.GPIO27.degrade(),
        latch: peripherals.GPIO26.degrade(),
    };

    // The original ESP32 drives the 16 HUB75 signals via its I2S0
    // peripheral in 16-bit parallel mode.  `.into_async()` returns the
    // same render-loop API as the S3 LCD_CAM path; the underlying DMA
    // completion is interrupt-driven either way.
    let mut hub75 = Hub75::new(
        peripherals.I2S0.into(),
        pins,
        peripherals.DMA_I2S0,
        tx_descriptors,
        Rate::from_mhz(20),
    )
    .expect("failed to create HUB75 driver")
    .into_async();

    let mut fb = FrameBuffer::new();

    // Seed the RNG with a hardcoded value for now.  Reading a true
    // hardware RNG is possible via `esp_hal::rng::Rng` but the seed feeds
    // only the jitter — the chunky cell-flipping motion is dominated by the
    // deterministic ball trajectory, so a fixed seed looks the same every
    // boot.  Swap in `Rng::new(peripherals.RNG).random()` if you'd rather
    // have a fresh game each power-cycle.
    let mut game = PongWars::new(0xC0FFEE);

    let mut frame_count: u32 = 0;

    loop {
        // Only advance the simulation on every `TICK_DIVISOR`-th frame.
        // The render always happens — the panel keeps refreshing at the
        // full DMA rate, which means a moving ball has sub-pixel
        // smoothness even when the game state is "frozen" between ticks.
        if frame_count % TICK_DIVISOR == 0 {
            // Step the simulation forward one tick.  Cells flip on
            // collisions; balls move; jitter is applied to the velocities.
            game.tick();

            // Re-render the new game state into the framebuffer.  We
            // only need to redraw on a tick, not on every frame.
            draw_frame(&mut fb, &game);
        }
        frame_count = frame_count.wrapping_add(1);

        // Hand the framebuffer to DMA and block until the transfer is done.
        // `xfer.wait()` consumes the driver and returns it back so we can
        // reuse it on the next iteration.
        let xfer = hub75
            .render(&fb)
            .map_err(|(e, _hub75)| e)
            .expect("failed to start HUB75 transfer");
        let (result, new_hub75) = xfer.wait();
        hub75 = new_hub75;
        result.expect("HUB75 transfer failed");
    }
}

// ---- rendering ------------------------------------------------------------

/// Convert one of our packed RGB565 palette entries into the
/// 24-bit RGB888 color type the framebuffer expects.
///
/// Bit-replication (low bits copied from the high bits) avoids banding
/// at the dark and bright ends of the gradient — without it, `Rgb565 →
/// Rgb888` would round all dark values to multiples of 8/8/8 and you'd
/// see posterisation.
#[inline]
fn to_eg(c: pong_wars_core::color::Rgb565) -> Rgb888 {
    let r5 = ((c >> 11) & 0x1F) as u8;
    let g6 = ((c >> 5) & 0x3F) as u8;
    let b5 = (c & 0x1F) as u8;
    Rgb888::new(
        (r5 << 3) | (r5 >> 2),
        (g6 << 2) | (g6 >> 4),
        (b5 << 3) | (b5 >> 2),
    )
}

/// Paint one frame into the framebuffer.
///
/// We bypass the embedded-graphics `Rectangle`/`Circle` primitives
/// entirely and drive the framebuffer's `DrawTarget` API directly with
/// `fill_solid` (for the cell blocks) and `set_pixel` (for the ball
/// dots).  The eg primitives hardcode their color type to `Rgb888` in
/// their impl, which doesn't match our `Rgb565` framebuffer — using the
/// lower-level `DrawTarget` methods sidesteps that and is also a few
/// percent faster (no per-cell primitive bookkeeping).
fn draw_frame(fb: &mut FrameBuffer, game: &PongWars) {
    let palette = pong_wars_core::Palette::classic();

    // Cells.  Each cell paints a `SQUARE_SIZE`×`SQUARE_SIZE` block.  A full
    // pass is ~1024 fill operations of 2×2 pixels — comfortably under a
    // millisecond on the ESP32 at 240 MHz.
    let square = SQUARE_SIZE_F as u32;
    for y in 0..GRID_SIZE {
        for x in 0..GRID_SIZE {
            let team = game.cell(x, y).expect("in-bounds cell");
            let color = to_eg(palette.cell_color(team));
            let area = Rectangle::new(
                Point::new((x as i32) * square as i32, (y as i32) * square as i32),
                Size::new(square, square),
            );
            // Rectangle here is just an area descriptor (no color type
            // involved), so the hardcoded-Rgb888 trap doesn't apply.
            fb.fill_solid(&area, color).ok();
        }
    }

    // Balls.  Drawn last so they always appear on top of the freshly
    // painted cells.  Each ball is a small filled circle of radius
    // `Ball::RADIUS`.
    for ball in game.balls() {
        draw_ball(fb, ball, to_eg(palette.ball_color(ball.team)));
    }
}

/// Rasterise a single ball as a filled circle using per-pixel writes.
///
/// Walks the bounding box of the circle and emits a pixel whenever the
/// pixel center is within `Ball::RADIUS` of the ball's center.  Coarse
/// (only a few pixels at 2× scaling) but matches the chunky look of the
/// web demo.  We use `libm::floorf` / `libm::ceilf` because the no_std
/// `f32::floor` / `f32::ceil` methods require a `Float` trait import that
/// we don't otherwise need.
fn draw_ball(fb: &mut FrameBuffer, ball: &Ball, color: Rgb888) {
    let radius = Ball::RADIUS;
    let r2 = radius * radius;
    let pix = pong_wars_core::PIXEL_SIZE as i32;
    let min_x = (libm::floorf(ball.pos.x - radius) as i32).max(0);
    let max_x = (libm::ceilf(ball.pos.x + radius) as i32).min(pix - 1);
    let min_y = (libm::floorf(ball.pos.y - radius) as i32).max(0);
    let max_y = (libm::ceilf(ball.pos.y + radius) as i32).min(pix - 1);

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f32 + 0.5 - ball.pos.x;
            let dy = y as f32 + 0.5 - ball.pos.y;
            if dx * dx + dy * dy <= r2 {
                // `DmaFrameBuffer::set_pixel` is its own inherent method
                // (not the `DrawTarget` trait's) and returns `()`.  We
                // accept silently because a write failure at this point
                // means the panel driver is already dead — there's no
                // recovery path.
                fb.set_pixel(Point::new(x, y), color);
            }
        }
    }
}
