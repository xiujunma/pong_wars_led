# Top-level convenience targets for the pong_wars_led workspace.
#
# Background: this is a mixed workspace — `core` and `simulator` build on the
# host with stable Rust, while `firmware` needs the Espressif `esp` toolchain
# (Xtensa).  Because cargo aliases can't override built-in subcommands like
# `test`, we provide Make targets instead.

.PHONY: help test check sim run-sim flash clean

# Default target — show a quick menu.
help:
	@echo "Common targets:"
	@echo "  make test       - run the 11 host tests (8 core + 3 sim), skip firmware"
	@echo "  make check      - cargo check on core + sim"
	@echo "  make sim        - launch the desktop simulator"
	@echo "  make flash      - build + flash the firmware onto the ESP32"
	@echo "  make clean      - remove the workspace target/ directory"

# Host tests: explicitly exclude the firmware crate because it pulls in
# esp-hal / esp-hub75 which need the Xtensa toolchain.
test:
	cargo test --workspace --exclude pong-wars-firmware

# Same exclusion for `cargo check`.
check:
	cargo check --workspace --exclude pong-wars-firmware

# Launch the desktop simulator (stable rustc, any host OS).
sim run-sim:
	cargo run --release -p pong-wars-sim

# Build + flash the firmware.  Requires:
#   1. `cargo install espup --locked && espup install`
#   2. `source ~/export-esp.sh` in this shell
#   3. The ESP32 board to be connected via USB (CP2102 driver on macOS/Linux)
# Override the port with:  make flash PORT=/dev/cu.usbserial-XXXX
PORT ?= /dev/cu.usbserial-0001
flash:
	cd firmware && ESPFLASH_PORT=$(PORT) cargo run --release

# Wipe build artifacts.  Safe to re-run; doesn't touch the .git directory.
clean:
	cargo clean
	rm -rf firmware/target core/target simulator/target
