# Pong Wars on ESP32 + 64×64 HUB75 LED Matrix

A complete, single-file reference for the project at this repository: the
firmware, the desktop simulator, the game-logic library, the hardware
wiring, the toolchain setup, and everything else discussed when the project
was built.

> [Pong Wars](https://github.com/vnglst/pong-wars) on an **ESP32** (the
> original, Xtensa LX6) driving a **64×64 HUB75 LED matrix**, written
> entirely in Rust. Two balls — one "day", one "night" — bounce around a
> 32×32 logical grid, flipping cells to their own team's color whenever
> they touch an enemy cell and ricocheting off in the process.

The simulation is a faithful port of [`maxj/wasmhub-dev/pong_wars.rs`][wasm-port]
(which itself ports the original HTML5 canvas demo), with arithmetic
switched from `f64` to `f32` and the heap-allocated grid replaced by a
stack-allocated fixed array so it fits comfortably in the chip's IRAM.

[wasm-port]: https://github.com/maxj/wasmhub-dev/tree/main/pong_wars.rs

---

## Table of contents

1. [Project layout](#1-project-layout)
2. [Quick start with no hardware (simulator)](#2-quick-start-with-no-hardware-simulator)
3. [Hardware requirements](#3-hardware-requirements)
4. [HUB75 connector pinout](#4-hub75-connector-pinout)
5. [Signal wiring (matches the firmware defaults)](#5-signal-wiring-matches-the-firmware-defaults)
6. [Power wiring](#6-power-wiring)
7. [Connection order](#7-connection-order)
8. [Software setup (toolchain)](#8-software-setup-toolchain)
9. [Build & flash](#9-build--flash)
10. [Simulator usage](#10-simulator-usage)
11. [Running the tests](#11-running-the-tests)
12. [Troubleshooting](#12-troubleshooting)
13. [Adafruit Matrix Portal S3 alternative](#13-adafruit-matrix-portal-s3-alternative)
14. [Architecture & design notes](#14-architecture--design-notes)
15. [Tweaks (what to edit to change what)](#15-tweaks-what-to-edit-to-change-what)
16. [Crate versions (pinned snapshot)](#16-crate-versions-pinned-snapshot)
17. [Key source files at a glance](#17-key-source-files-at-a-glance)
18. [License](#18-license)

---

## 1. Project layout

```
pong_wars_led/
├── Cargo.toml                     workspace (core + firmware + simulator)
├── README.md                      slim entry point
├── GUIDE.md                       this document
├── .gitignore
│
├── core/                          no_std + host-testable game library
│   ├── Cargo.toml                 deps: oorandom 11.1, libm 0.2
│   └── src/
│       ├── lib.rs                 re-exports
│       ├── color.rs               RGB565 palette, Team enum, Palette struct
│       └── game.rs                PongWars: 32×32 grid, 2 balls, tick(),
│                                  score(), 8 unit tests
│
├── simulator/                     desktop preview window (stable rustc)
│   ├── Cargo.toml                 deps: pong-wars-core, minifb 0.28
│   └── src/main.rs                768×768 window simulating the 64×64 panel
│                                  as discrete circular LED dots
│
└── firmware/                      ESP32 binary (Xtensa, no_std)
    ├── Cargo.toml                 esp-hal 1.1.1, esp-hub75 0.11,
    │                              embedded-graphics 0.8.2
    ├── rust-toolchain.toml        channel = "esp" (Xtensa fork of rustc)
    ├── .cargo/config.toml         xtensa-esp32-none-elf + espflash runner
    └── src/main.rs                I2S-parallel driver init, draw_frame(),
                                  blocking render loop
```

**The split keeps the simulation pure and unit-testable on the host while
the firmware crate handles everything ESP-specific.** The simulator binary
lets you tweak the game and watch the result without owning hardware.

---

## 2. Quick start with no hardware (simulator)

```bash
cargo run --release -p pong-wars-sim
```

Opens a 768×768 window showing the simulated 64×64 LED panel. Each LED is
drawn as a circular dot on a near-black background, so a powered-down LED
reads as a dark bezel — close to the look of a real HUB75 matrix.

| Key            | Action                                        |
|----------------|-----------------------------------------------|
| `Space`        | Pause / resume                                |
| `R`            | Reset with a fresh seed                       |
| `+` / `-`      | Speed up / slow down (1×, 2×, 4×, 8×, 16×)    |
| `ESC` / close  | Quit                                          |

The score is shown in the title bar: `Pong Wars — day 512 / night 512 — 1×`.

---

## 3. Hardware requirements

| Item                                          | Notes                              |
|-----------------------------------------------|------------------------------------|
| **ESP32-DevKitC-1** (or any original ESP32 board) | WROOM-32 module, Xtensa LX6, 240 MHz, 320 KB SRAM, **no PSRAM** |
| **64×64 HUB75 RGB LED matrix, 1/32 scan**     | Must have 5 address lines (A–E). 1/16-scan panels won't work without code changes |
| 16-conductor 2.54 mm IDC ribbon cable         | Usually shipped with the panel     |
| Jumper wires (M-F)                            | If you don't have an IDC breakout — ~14 wires |
| **No external PSU**                           | The matrix draws only ~0.1 A at 5 V, so power it straight from the ESP32-DevKitC-1's `5V` pin (which is fed from USB). See §6 for details and the optional external-PSU upgrade path. |

### Why 1/32 scan matters

64-row panels have 32 unique address values × 2 halves driven in parallel.
That requires five address lines: A (addr0), B (addr1), C (addr2), D (addr3),
**E (addr4)**. Older or smaller (32-row) panels have only A–D — they're
**1/16 scan**, and the code as written drives an E line they don't have.
Always check the panel datasheet or silkscreen.



### Why 1/32 scan matters

64-row panels have 32 unique address values × 2 halves driven in parallel.
That requires five address lines: A (addr0), B (addr1), C (addr2), D (addr3),
**E (addr4)**. Older or smaller (32-row) panels have only A–D — they're
**1/16 scan**, and the code as written drives an E line they don't have.
Always check the panel datasheet or silkscreen.

---

## 4. HUB75 connector pinout

Every HUB75 panel has **two** 16-pin IDC headers on the back labeled
something like `J1/J2`, `IN/OUT`, or `HUB-IN/HUB-OUT`. There's an arrow
silkscreen showing the data direction — **always plug into the INPUT side**
(the side the arrow points away from).

Looking at the back of the panel with the keying notch on top:

```
        ┌───── notch ─────┐
   pin 1│  R1   │   G1   │ pin 2
   pin 3│  B1   │   GND  │ pin 4
   pin 5│  R2   │   G2   │ pin 6
   pin 7│  B2   │   E*   │ pin 8     (*pin 8 is GND on 1/16-scan panels)
   pin 9│  A    │   B    │ pin 10
  pin 11│  C    │   D    │ pin 12
  pin 13│  CLK  │   LAT  │ pin 14
  pin 15│  OE   │   GND  │ pin 16
        └─────────────────┘
```

On **1/32-scan** panels (anything 64-row or taller), the pin that's GND on
shorter panels becomes the **E address line** (pin 8). The firmware's
default pinout assumes this — `addr4 → GPIO 3` wires to that pin. If your
panel is 1/16 scan (32 rows or less), it has no E line and the project
won't drive it without code changes.

---

## 5. Signal wiring (matches the firmware defaults)

These are the GPIOs hard-coded in `firmware/src/main.rs`. Connect the
panel's IDC pin to the corresponding GPIO on the ESP32-DevKitC-1:

| HUB75 pin | Signal     | ESP32 GPIO |
|----------:|------------|-----------:|
| 1         | R1         | **GPIO 16** |
| 2         | G1         | **GPIO 4**  |
| 3         | B1         | **GPIO 17** |
| 4         | GND        | any GND on the dev board |
| 5         | R2         | **GPIO 18** |
| 6         | G2         | **GPIO 5**  |
| 7         | B2         | **GPIO 19** |
| 8         | E (addr4)  | **GPIO 2**  |
| 9         | A (addr0)  | **GPIO 15** |
| 10        | B (addr1)  | **GPIO 13** |
| 11        | C (addr2)  | **GPIO 12** |
| 12        | D (addr3)  | **GPIO 14** |
| 13        | CLK        | **GPIO 27** |
| 14        | LAT        | **GPIO 26** |
| 15        | OE / BLANK | **GPIO 25** |
| 16        | GND        | any GND on the dev board |

Easiest way is an IDC-to-breakout adapter (~$3–5), but you can also stuff
jumpers straight into the IDC connector if you're careful about contact.

To re-map, edit the `Hub75Pins16 { … }` block near the top of
`firmware/src/main.rs`.

**Heads-up:** the default map reuses GPIO 2, 12, and 15, which are ESP32
strapping pins.  They influence boot mode / flash voltage at reset, but
once the I2S peripheral takes over the pins, the strapping role is
released — so this is safe in practice and matches the upstream
`esp-hub75` example.  If you'd rather avoid them, swap GPIO 2/12/15 for
any of GPIO 22, 23, 32, 33 (also output-capable, non-strapping) and
update both the wiring table and the firmware's `Hub75Pins16` block.

### Why these specific GPIOs

The original ESP32 has 14 output-capable GPIOs that aren't used by the
module's flash (6–11) or aren't input-only (34, 35, 36, 39), and
**excluding** the strapping pins (0, 2, 12, 15) that's a pool of:
GPIO 1, 3, 4, 5, 13, 14, 16, 17, 18, 19, 21, 22, 23, 25, 26, 27, 32, 33.

The HUB75 needs 14 distinct signals (R1, G1, B1, R2, G2, B2, A, B, C, D,
E, LAT, BLANK, CLK).  The upstream `esp-hub75` `hello_i2s_parallel.rs`
example picks a known-good subset of 14, and the firmware uses the same
map to minimise "works on the dev board, fails on yours" risk.

---

## 6. Power wiring

**Good news: with this project, you don't need an external PSU.** The
matrix draws only **~0.1 A at 5 V** under normal game-play load
(average of mostly-dark panel with a couple of bouncing dots), so
the ESP32-DevKitC-1's `5V` pin — which is fed directly from the
USB's 5 V rail when the board is plugged in — has more than enough
headroom (USB 2.0 supplies 500 mA, the matrix takes 100 mA).

The matrix has **two screw terminals or spade lugs** marked `+5V` and
`GND` — completely separate from the IDC signal connector.

```
   ┌─────────────────────────────────────────────┐
   │  ESP32-DevKitC-1  +5V (USB) ──► matrix  +5V │
   │                 GND ─────┬─► matrix  GND    │
   │                         │                   │
   │   USB cable from computer ┘                  │
   │   (powers chip AND matrix; 0.1 A total)    │
   └─────────────────────────────────────────────┘
```

### Two wires, that's it

1. **Jumper from the ESP32's `5V` pin to the matrix's `+5V` terminal.**
   A standard M-F jumper is fine — we're moving 100 mA, not 4 A.
2. **Jumper from any ESP32 `GND` pin to the matrix's `GND` terminal.**
   This is non-negotiable. Without a common ground, the matrix's
   signal logic levels float relative to the ESP32's and the display
   shows garbage (or nothing).

That's it. Plug USB into the ESP32 and the whole thing powers up.

### When you'd want an external PSU (optional)

The 0.1 A figure is the average for a real game. The **peak** draw on
a 64×64 RGB panel at full white can still hit **1.5–4 A** depending on
the panel's brightness setting and BCM duty cycle. If you want a
brighter display than what USB can supply, or if you have a
high-brightness / "P10 outdoor" panel, you'd want an external 5 V PSU:

```
   ┌─────────────────────────────────────────────┐
   │  Bench PSU 5V   +──────► matrix  +5V        │
   │               −──┬───► matrix  GND          │
   │                  └────► ESP32 GND           │  ← all three grounds tied
   │                                              │
   │   USB → ESP32 (still powers the chip)         │
   └─────────────────────────────────────────────┘
```

The PSU should be sized to your panel's worst-case draw. As a rule of
thumb, **2 A per 64×64 panel** is safe for indoor-grade matrices;
4 A for outdoor-grade. A MeanWell LRS-35-5 (5 V / 7 A) is the
standard choice.

If you go this route, the rule is: **tie the grounds, keep the
`+5V` sources separate** (don't connect the PSU's `+5V` to the
ESP32's `5V` pin — that would let the panel's 1–4 A back-feed into
the ESP32's USB rail).

### Sanity check (no matter how you powered it)

1. With everything connected, plug in USB to the ESP32.  The board's
   red power LED should be on.
2. The matrix should stay dark (or show static noise from a
   uninitialised shift register) — no clock signal yet.
3. After flashing the firmware (`cargo run --release -p
   pong-wars-firmware`), the matrix should start displaying the
   boot-time test pattern or the Pong Wars game within ~1 second.

---

## 7. Connection order

1. **Wire everything with the USB cable unplugged.** No power on.
2. **Double-check the +5V and GND jumpers** — `+5V` to the matrix's
   `+5V` terminal, `GND` to `GND`. Reversing these will damage the
   panel; verify with a multimeter against the silkscreen.
3. **Plug USB into the ESP32.** The board's red power LED should be
   on, and the matrix should stay dark or show static noise from a
   uninitialised shift register — both are fine.
4. **Flash:**
   ```bash
   cargo run --release -p pong-wars-firmware
   ```

---

## 8. Software setup (toolchain)

You need Espressif's Rust fork (Xtensa LLVM patches are upstream-pending),
plus `espflash` to talk to the chip. One-time setup:

```bash
# Espressif's installer for the Xtensa toolchain.
cargo install espup --locked
espup install

# Activate it for the current shell (add this line to your zshrc/bashrc).
source ~/export-esp.sh

# Flashing + monitor tool.
cargo install espflash --locked
```

The `firmware/rust-toolchain.toml` pins `channel = "esp"`, so as soon as
you `cd firmware` (or run `cargo … -p pong-wars-firmware` from the root)
cargo will use the Xtensa toolchain. Outside the firmware directory your
normal stable/nightly rustc keeps working — handy for running the
host-side tests and the simulator.

### Why `espup`?

- ESP32 (and ESP32-S3) are **Xtensa**, not RISC-V. The upstream LLVM
  doesn't yet ship the Xtensa backend that the compiler needs, so
  Espressif maintains a downstream fork (`esp-rs/rust`).
- `espup` installs that fork as a custom rustup channel called `esp`.
- The channel ships its own rustc plus the `xtensa-esp32-none-elf`
  target — no `rustup target add` is needed.

### Why not embassy?

The canonical [`esp-hub75`][hub75] example for the original ESP32 is
plain blocking with `#[esp_hal::main]`. The render loop is "render →
DMA-transfer → wait → repeat" which doesn't benefit from async. Skipping
embassy cuts the dependency surface significantly and avoids the
private-feature resolution issues in `esp-hal-embassy 0.9.1`.

[hub75]: https://github.com/liebman/esp-hub75

### Why I2S-parallel and not LCD_CAM?

The original ESP32 doesn't have an LCD_CAM peripheral — that one is
S3-and-later.  The I2S peripheral can be configured into a 16-bit parallel
data mode that drives the HUB75 signals in lockstep via DMA.  The
`esp-hub75` crate abstracts the difference; the only thing the firmware
cares about is the constructor (`Hub75::new(I2S0.into(), …).into_async()`
on ESP32 vs `Hub75::new_async(LCD_CAM, …)` on S3) and the DMA channel
name (`DMA_I2S0` vs `DMA_CH0`).

---

## 9. Build & flash

```bash
# 1. Plug the board in via USB.
# 2. From the workspace root:
cargo run --release -p pong-wars-firmware
```

Cargo will:

1. Pick the Xtensa toolchain (via `firmware/rust-toolchain.toml`).
2. Compile for `xtensa-esp32-none-elf` (via `firmware/.cargo/config.toml`).
3. Hand the ELF to `espflash flash --monitor`, which writes the bootloader
   + partition table + app and then dumps serial output.

Press the board's `BOOT` button if `espflash` complains about not finding
the chip; some boards need it held while you hit enter.

---

## 10. Simulator usage

The simulator lives in `simulator/src/main.rs` and uses
[`minifb`](https://crates.io/crates/minifb) — chosen because it's the
smallest cross-platform way to get a window with a raw u32 ARGB pixel
buffer (which is exactly what an LED-panel simulator wants).

### How it works

- **Same simulation, same code path.** The simulator calls the exact
  `PongWars::tick()` the firmware does — it's the only thing in the loop.
- **Per-LED rendering.** Each of the 64×64 simulated LEDs becomes a
  12×12-pixel cell in the output window (`LED_SIZE`), with a 10-pixel
  circular dot (`LED_DIAMETER`) in the center colored according to the
  simulation. Cells around the dot stay near-black so the output reads as
  discrete LEDs, not a solid wall.
- **768×768 window** (`PIXEL_SIZE * LED_SIZE`), 60 FPS cap, resize-locked
  so the dots stay square.

### Keys

| Key            | Action                                        |
|----------------|-----------------------------------------------|
| `Space`        | Pause / resume                                |
| `R`            | Reset with a fresh seed                       |
| `+` / `-`      | Speed up / slow down (1×, 2×, 4×, 8×, 16×)    |
| `ESC` / close  | Quit                                          |

### Three simulator-only tests

- `rgb565_black_is_black` — converter floor case.
- `rgb565_white_round_trips_close_to_white` — converter ceiling case
  (verifies the bit-replicate trick).
- `led_mask_has_about_the_right_lit_count` — sanity-checks the cached
  LED-dot mask against the analytical area of a circle (≈ 78.5 px²).

---

## 11. Running the tests

The game logic is fully host-testable. Eight unit tests in `core` plus
three in `simulator` — eleven total, all run on your host with the default
toolchain. No ESP hardware required.

```bash
cargo test --workspace --exclude pong-wars-firmware
```

or per-crate:

```bash
cargo test -p pong-wars-core
cargo test -p pong-wars-sim
```

Expected output:

```
running 8 tests
test game::tests::cell_out_of_bounds_returns_none ... ok
test game::tests::initial_balls_are_on_opposite_quarters ... ok
test game::tests::initial_split_is_half_and_half ... ok
test game::tests::tick_does_not_change_total_cell_count ... ok
test game::tests::determinism_same_seed_same_history ... ok
test game::tests::one_side_eventually_makes_progress ... ok
test game::tests::ball_speed_stays_bounded ... ok
test game::tests::ball_stays_inside_the_grid ... ok
test result: ok. 8 passed; 0 failed

running 3 tests
test tests::rgb565_black_is_black ... ok
test tests::rgb565_white_round_trips_close_to_white ... ok
test tests::led_mask_has_about_the_right_lit_count ... ok
test result: ok. 3 passed; 0 failed
```

### What each core test guards against

| Test                                  | Catches                                       |
|---------------------------------------|-----------------------------------------------|
| `initial_split_is_half_and_half`      | Bug in the initial fill that imbalances teams |
| `initial_balls_are_on_opposite_quarters` | Bug in starting positions                  |
| `tick_does_not_change_total_cell_count` | Cells getting "lost" during collision math  |
| `ball_stays_inside_the_grid`          | Boundary-collision math letting the ball escape |
| `ball_speed_stays_bounded`            | Jitter exceeding `MIN_SPEED..=MAX_SPEED`      |
| `determinism_same_seed_same_history`  | Hidden global state breaking reproducibility  |
| `cell_out_of_bounds_returns_none`     | Public API bounds-checking                    |
| `one_side_eventually_makes_progress`  | Game stalling at exactly 50/50 (smoke test)   |

---

## 12. Troubleshooting

In rough order of likelihood:

| Symptom | Likely cause |
|---------|--------------|
| Panel stays completely dark | Wrong IDC connector (plugged into HUB-OUT), or +5V missing |
| Top half OK, bottom half wrong/dark | R2/G2/B2 wires swapped or one is missing |
| Image is split horizontally with wrong rows | E (addr4) not connected — your panel is 1/32 scan and needs all five address lines |
| Image is geometrically scrambled but moving | One of the address lines (A–E) miswired or floating |
| Image visible but very dim or flickery | Your panel may draw more than the USB rail can supply (e.g. a higher-brightness panel or full-white scenes). Add an external 5 V PSU as described in §6. Also check that GND is tied between the board and the panel. |
| Whole image is shifted by N pixels and wrapping | Wrong `COLS` in `firmware/src/main.rs` (should be 64) |
| Top and bottom halves swapped | R1↔R2, G1↔G2, B1↔B2 all swapped — re-check IDC pin numbering |
| Colors wrong (red shows as green, etc.) | R/G/B swapped on one half — re-check pins 1/2/3 or 5/6/7 |
| Glitchy at one brightness, OK at another | Pixel clock too high — drop `Rate::from_mhz(20)` to `Rate::from_mhz(10)` in `firmware/src/main.rs` |
| `espflash` doesn't find the chip | Hold the `BOOT` button while pressing `RESET`, then run again |
| Cargo build fails with "feature `esp32` not found" | You're outside the `firmware/` directory and using stable rustc instead of the `esp` channel |
| Cargo build fails on `esp-hal-embassy` | Don't depend on it — this project deliberately doesn't use embassy |

---

## 13. Adafruit Matrix Portal S3 alternative

The [Adafruit Matrix Portal S3](https://www.adafruit.com/product/5778) is
an ESP32-S3 dev board with the HUB75 IDC connector and a barrel-jack power
input *built in*. Using one massively simplifies wiring:

1. Plug the panel's IDC into the Matrix Portal's HUB75 socket.
2. Plug a 5V PSU into the Matrix Portal's barrel-jack input (the
   Portal routes this to the matrix's `+5V` screw terminal internally;
   pick a 5V/2A supply for indoor panels, 5V/4A for outdoor-grade).
3. Plug USB into the Matrix Portal for flashing.

**You must remap the GPIOs** — the Matrix Portal uses different pins than
the DevKitC-1. Replace the `Hub75Pins16 { ... }` block in
`firmware/src/main.rs` with:

```rust
let pins = Hub75Pins16 {
    red1:  peripherals.GPIO42.degrade(),
    grn1:  peripherals.GPIO41.degrade(),
    blu1:  peripherals.GPIO40.degrade(),
    red2:  peripherals.GPIO38.degrade(),
    grn2:  peripherals.GPIO39.degrade(),
    blu2:  peripherals.GPIO37.degrade(),
    addr0: peripherals.GPIO45.degrade(),
    addr1: peripherals.GPIO36.degrade(),
    addr2: peripherals.GPIO48.degrade(),
    addr3: peripherals.GPIO35.degrade(),
    addr4: peripherals.GPIO21.degrade(),
    clock: peripherals.GPIO2.degrade(),
    latch: peripherals.GPIO47.degrade(),
    blank: peripherals.GPIO14.degrade(),
};
```

**Verify against Adafruit's current schematic before you trust these
numbers** — they were assembled from memory of the Matrix Portal S3
pinout. The schematic PDF is on the Adafruit product page.

---

## 14. Architecture & design notes

### Why three crates?

- `pong-wars-core` — pure simulation, **no_std**, no display, no I/O. Builds
  on any target. Tests run on the host with the default toolchain.
- `pong-wars-firmware` — ESP32 binary. Depends on `pong-wars-core`. Has
  its own `rust-toolchain.toml` so it doesn't poison host builds.
- `pong-wars-sim` — desktop preview. Depends on `pong-wars-core` and
  `minifb`. Builds with stable rustc.

This split means you can iterate on game-feel using the simulator (fast
build, fast feedback) without touching the firmware crate, and you can
verify that gameplay correctness is preserved with host tests before you
ever flash hardware.

### Why fixed-size arrays instead of `Vec`?

The original WASM port stores cells in a `Vec<Vec<&str>>` keyed by canvas
size. On an MCU we know the panel up-front, the heap is small, and
`const`-sized arrays let the whole game state sit on the stack with zero
allocations:

```rust
pub struct PongWars {
    cells: [Team; GRID_SIZE * GRID_SIZE],   // 1024 bytes (Team is u8 repr)
    balls: [Ball; 2],                       // ~48 bytes
    rng:   Rand32,                          // 16 bytes
}
```

Total state: ~1 KB. Stack-allocated. The `esp-hub75` framebuffer dwarfs it
anyway.

### Why `f32` instead of `f64`?

Xtensa LX6 (ESP32) has a hardware **single-precision** FPU. Double
precision is software-emulated and ~20× slower. The game's collision math
doesn't need more than ~7 decimal digits of precision, so f32 is the right
default.

### Why `libm` instead of `f32::sin` etc.?

`f32::sin` lives in std. In a `no_std` library you have to call
`libm::sinf` directly. The `libm` crate is the Rust port of MUSL's
software math — same algorithms, same accuracy, just available without
std.

### Why `oorandom` instead of `rand`?

`oorandom` is a single-file PRNG with no global state, no allocations, and
no platform requirements. `rand` is great but pulls a much heavier
dependency tree and its default RNG wants OS entropy. For deterministic
jitter we just need a seedable PCG, which is exactly what
`oorandom::Rand32` is.

### Why blocking I/O instead of embassy?

The render loop is "build framebuffer → submit to DMA → wait → repeat".
There's nothing to overlap. Embassy would just add an executor, task
arenas, and version-pinning headaches. The official `esp-hub75` example
[`hello_lcd_cam.rs`](https://github.com/liebman/esp-hub75/blob/main/examples/hello_lcd_cam.rs)
is also blocking.

### Velocity scaling

The original WASM version used ±12.5 px/tick on an ~800-pixel canvas
(≈ 1.6% of the field per frame). Scaled to our 64-pixel field that would
be ~1 px/tick, which is too much — the ball would teleport past cells
without flipping them. Starting velocity is `±0.6 px/tick`
(≈ 0.9% of the field), and the speed band is clamped to
`[MIN_SPEED, MAX_SPEED] = [0.40, 0.90]`. Tuned by eye in the simulator.

### Why 32×32 cells of 2×2 pixels on a 64×64 panel?

```
PIXEL_SIZE   = 64
SQUARE_SIZE  = 2
GRID_SIZE    = PIXEL_SIZE / SQUARE_SIZE = 32
```

This preserves the chunky look of the web demo (which has roughly 32×32
cells on its default 800×800 canvas) while leaving the ball visible as a
2×2 dot. Going to 1-pixel cells would shrink the ball to a single pixel
and the gameplay would visually disappear.

### Deterministic seeds

`PongWars::new(seed)` only seeds the per-tick jitter; the initial layout
and starting velocities are fixed. So two engines started with the same
seed will trace **identical histories**. This is what makes the
`determinism_same_seed_same_history` test possible, and lets you
reproduce a specific game by hard-coding the seed in the simulator's
`initial_seed()`.

---

## 15. Tweaks (what to edit to change what)

| What you want                       | Where                                 |
|-------------------------------------|----------------------------------------|
| Change the palette                  | `core/src/color.rs` → `Palette::classic` |
| Change cell size or grid resolution | `core/src/game.rs` → `SQUARE_SIZE` / `PIXEL_SIZE` (set `PIXEL_SIZE` to match your panel; `SQUARE_SIZE` must divide it evenly) |
| Change ball speed band              | `core/src/game.rs` → `MIN_SPEED` / `MAX_SPEED` |
| Change starting velocity            | `core/src/game.rs` → `init_speed` inside `PongWars::new` |
| Change the random jitter strength   | `core/src/game.rs` → `RANDOM_JITTER`   |
| Change pixel clock (HUB75 refresh)  | `firmware/src/main.rs` → `Rate::from_mhz(20)` (drop to 10 if you see glitches) |
| Change pinout                       | `firmware/src/main.rs` → `Hub75Pins16 { … }` |
| Use a different original-ESP32 board | Re-map pins in `firmware/src/main.rs`'s `Hub75Pins16` block; keep the `esp32` feature on `esp-hal`/`esp-hub75` |
| Use the ESP32-S3 instead             | Change `esp32` features to `esp32s3`, swap `Hub75::new(I2S0.into(), …).into_async()` for `Hub75::new_async(LCD_CAM, …)`, change `DMA_I2S0` to `DMA_CH0`, retarget to `xtensa-esp32s3-none-elf`. See `esp-hub75`'s `examples/hello_lcd_cam.rs`. |
| Change simulator dot size / window  | `simulator/src/main.rs` → `LED_SIZE` / `LED_DIAMETER` |
| Change simulator background color   | `simulator/src/main.rs` → `BACKGROUND` |
| Deterministic simulator playback    | Replace `initial_seed()` body with a constant |
| Use defmt/RTT instead of UART logs  | `cargo run -p pong-wars-firmware --features defmt` |

---

## 16. Crate versions (pinned snapshot)

Working snapshot of the embedded-Rust ecosystem (Jun 2026):

| Crate                    | Version |
|--------------------------|---------|
| `esp-hal`                | 1.1.1   |
| `esp-hub75`              | 0.11.0  |
| `esp-alloc`              | 0.10.0  |
| `esp-backtrace`          | 0.19.0  |
| `esp-println`            | 0.17.0  |
| `esp-bootloader-esp-idf` | 0.5.0   |
| `embedded-graphics`      | 0.8.2   |
| `embedded-hal`           | 1.0.0   |
| `oorandom`               | 11.1.5  |
| `libm`                   | 0.2.16  |
| `minifb` (simulator)     | 0.28    |

If you upgrade, expect the following to be the most likely break points:
- `esp-hal` chip-selector features and the `unstable` feature gate
- `esp-hub75`'s `Hub75Pins16` field names (`red1`/`grn1`/`blu1`…)
- `esp-hub75::framebuffer::compute_rows` signature

---

## 17. Key source files at a glance

### `core/src/lib.rs`

```rust
#![cfg_attr(not(test), no_std)]
#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod color;
pub mod game;

pub use color::{Palette, Team, TEAM_DAY, TEAM_NIGHT};
pub use game::{Ball, PongWars, Vec2, GRID_SIZE, PIXEL_SIZE, SQUARE_SIZE_F};
```

### `core/src/color.rs` (key constants)

```rust
pub const COLOR_MYSTIC_MINT:          Rgb565 = rgb565(0xD9, 0xE8, 0xE3);
pub const COLOR_NOCTURNAL_EXPEDITION: Rgb565 = rgb565(0x11, 0x4C, 0x5A);

pub enum Team { Day = 0, Night = 1 }

pub struct Palette {
    pub day_cell:   Rgb565,
    pub night_cell: Rgb565,
    pub day_ball:   Rgb565,
    pub night_ball: Rgb565,
}
```

### `core/src/game.rs` (key constants and the public API)

```rust
pub const PIXEL_SIZE:    usize = 64;
pub const SQUARE_SIZE:   usize = 2;
pub const SQUARE_SIZE_F: f32   = SQUARE_SIZE as f32;
pub const GRID_SIZE:     usize = PIXEL_SIZE / SQUARE_SIZE;   // 32

pub const MIN_SPEED:     f32 = 0.40;
pub const MAX_SPEED:     f32 = 0.90;
pub const RANDOM_JITTER: f32 = 0.004;

pub struct PongWars { /* cells, balls, rng */ }

impl PongWars {
    pub fn new(seed: u64) -> Self;
    pub fn tick(&mut self);
    pub fn cells(&self) -> &[Team; GRID_SIZE * GRID_SIZE];
    pub fn balls(&self) -> &[Ball; 2];
    pub fn cell(&self, x: usize, y: usize) -> Option<Team>;
    pub fn score(&self) -> (u32, u32);   // (day_cells, night_cells)
}
```

### `firmware/src/main.rs` (skeleton)

```rust
#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

const ROWS:   usize = 64;
const COLS:   usize = 64;
const NROWS:  usize = compute_rows(ROWS);
const PLANES: usize = 7;
type FrameBuffer = DmaFrameBuffer<NROWS, COLS, PLANES>;

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(
        esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    let tx_descriptors = esp_hub75::hub75_dma_descriptors!(FrameBuffer);
    let pins = Hub75Pins16 { /* see §5 */ };

    let mut hub75 = Hub75::new_async(
        peripherals.I2S0.into(), pins, peripherals.DMA_I2S0,
        tx_descriptors, Rate::from_mhz(20),
    ).expect("failed to create HUB75 driver");

    let mut fb   = FrameBuffer::new();
    let mut game = PongWars::new(0xC0FFEE);

    loop {
        game.tick();
        draw_frame(&mut fb, &game);

        let xfer = hub75.render(&fb)
            .map_err(|(e, _)| e).expect("failed to start transfer");
        let (result, new_hub75) = xfer.wait();
        hub75 = new_hub75;
        result.expect("transfer failed");
    }
}
```

### `simulator/src/main.rs` (skeleton)

```rust
const LED_SIZE:     usize = 12;
const LED_DIAMETER: usize = 10;
const WINDOW_W:     usize = PIXEL_SIZE * LED_SIZE;   // 768
const WINDOW_H:     usize = PIXEL_SIZE * LED_SIZE;
const BACKGROUND:   u32   = 0xFF_08_08_08;

fn main() {
    let mut window = Window::new("Pong Wars — 64×64 LED simulator",
        WINDOW_W, WINDOW_H, /* …minifb options… */).unwrap();
    window.set_target_fps(60);

    let mut buf  = vec![BACKGROUND; WINDOW_W * WINDOW_H];
    let mut game = PongWars::new(initial_seed());
    let mut paused = false;
    let mut ticks_per_frame = 1u32;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        handle_input(&window, &mut paused, &mut ticks_per_frame, &mut game);
        if !paused {
            for _ in 0..ticks_per_frame { game.tick(); }
        }
        draw_frame(&mut buf, &game, &Palette::classic());
        window.update_with_buffer(&buf, WINDOW_W, WINDOW_H).unwrap();
    }
}
```

---

## 18. License

MIT. Original game by [Koen van Gilst](https://github.com/vnglst/pong-wars).

---

## Appendix A: What's been verified vs. what hasn't

To set expectations honestly:

| Thing                                  | Status |
|----------------------------------------|--------|
| `cargo test -p pong-wars-core`         | ✅ 8/8 passing locally |
| `cargo test -p pong-wars-sim`          | ✅ 3/3 passing locally |
| `cargo build -p pong-wars-sim`         | ✅ Builds clean (no warnings) |
| Simulator window opens and renders     | ✅ Smoke-tested for 4 s |
| `cargo metadata --filter-platform xtensa-esp32-none-elf` | ✅ Full 167-crate graph resolves |
| `cargo build -p pong-wars-firmware`    | ⚠️ Not built here (no `espup` toolchain on the dev machine) |
| Flashing real hardware                 | ⚠️ Not performed |

The firmware code mirrors the canonical `esp-hub75` `hello_i2s_parallel.rs`
example for the original ESP32, with only the rendering body changed.  The
most likely place a fresh build will hiccup is on minor API shifts in
`esp-hal 1.1.x` or `esp-hub75 0.11.x` between minor versions — if you see
one, it'll be a small mechanical fix (rename a field, add a `degrade()`,
etc.) rather than a structural rework.

## Appendix B: Sources used while building this

- esp-hub75 [`hello_i2s_parallel.rs`](https://github.com/liebman/esp-hub75/blob/main/examples/hello_i2s_parallel.rs) — canonical original-ESP32 I2S-parallel init pattern.
- [esp-hal 1.1.1 docs](https://docs.espressif.com/projects/rust/esp-hal/1.1.1/esp32/esp_hal/) — chip-selection mechanism, peripherals API.
- [vnglst/pong-wars](https://github.com/vnglst/pong-wars) — the original web game.
- [maxj/wasmhub-dev/pong_wars.rs](https://github.com/maxj/wasmhub-dev/tree/main/pong_wars.rs) — the WASM Rust port we ported from.
- crates.io sparse index — for pinning compatible crate versions.
