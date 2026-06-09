# pong_wars_led

[Pong Wars](https://github.com/vnglst/pong-wars) on an **ESP32** (the
original, Xtensa LX6) driving a **64×64 HUB75 LED matrix**, written
entirely in Rust.

![Pong Wars running on the ESP32 + 64x64 HUB75 panel](docs/demo.gif)

Two balls — one "day", one "night" — bounce around a 32×32 logical grid,
flipping cells to their own team's color whenever they touch an enemy cell
and ricocheting off in the process. The simulation is a faithful port of
[`wasmhub-dev/pong_wars.rs`][wasm-port] (which itself ports the
original HTML5 canvas demo), with arithmetic switched from `f64` to `f32`
and the heap-allocated grid replaced by a stack-allocated fixed array so
it fits comfortably in the chip's ~320 KB SRAM.

[wasm-port]: https://github.com/wasmhub-dev/tree/main/pong_wars.rs

## Quick preview (no hardware)

```bash
cargo run --release -p pong-wars-sim
```

Opens a 768×768 window showing the simulated 64×64 LED panel — each LED as
a circular dot on a near-black background. Space pauses, R resets,
+/- changes speed, ESC quits. Full controls in [`GUIDE.md`](GUIDE.md).

## Layout

```
pong_wars_led/
├── core/        no_std game library (host-testable, target-neutral)
├── simulator/   desktop preview window (stable rustc, any OS)
└── firmware/    ESP32 binary — I2S parallel HUB75 driver, blocking render
                 loop.  Builds for `xtensa-esp32-none-elf` via the `esp`
                 toolchain.  Default pin map is for the ESP32-DevKitC-1.
```

## Hardware

A bare **ESP32-DevKitC-1** (with the WROOM-32 module) plus a bare
**64×64 HUB75** matrix, or any equivalent. Detailed wiring is in
[`GUIDE.md`](GUIDE.md#5-signal-wiring-matches-the-firmware-defaults) but
the short version is:

| HUB75 | GPIO | HUB75 | GPIO |
| ----- | ---- | ----- | ---- |
| R1    | 16   | A     | 15   |
| G1    | 4    | B     | 13   |
| B1    | 17   | C     | 12   |
| R2    | 18   | D     | 14   |
| G2    | 5    | E     | 2    |
| B2    | 19   | LAT   | 26   |
| CLK   | 27   | OE    | 25   |

Power: the matrix draws only ~0.1 A at 5 V, so you can power it straight
from the ESP32-DevKitC-1's `5V` pin (which is fed from USB). **No
external PSU needed.** If your panel is a higher-brightness model or
you plan to push it to full white, see "External PSU (optional)" in
the guide.

## Build & flash

```bash
# One-time toolchain (Xtensa fork of rustc via espup).
cargo install espup --locked && espup install
source ~/export-esp.sh             # add to your shell rc
cargo install espflash --locked

# Each time:
cargo run --release -p pong-wars-firmware
```

`espflash` will erase the chip, write the bootloader + partition table +
app, and open a serial monitor.

## Tests

```bash
cargo test                          # all 11 tests (8 core + 3 sim)
cargo test -p pong-wars-core        # game-logic tests only
```

## See also

- [`GUIDE.md`](GUIDE.md) — full reference: wiring details, PSU sizing,
  connection order, troubleshooting, architecture notes, tweaks, code
  skeletons.
- [vnglst/pong-wars](https://github.com/vnglst/pong-wars) — the original
  web game.
- [liebman/esp-hub75](https://github.com/liebman/esp-hub75) — the HUB75
  driver crate this project uses.

## License

MIT. Original game by [Koen van Gilst](https://github.com/vnglst/pong-wars).
