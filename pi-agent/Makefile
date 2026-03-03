PORT       ?= 8765
PI_TARGET  ?= aarch64-unknown-linux-gnu
CROSS_TOOL ?= cross
PI_USER    ?= pi
PI_HOST    ?=

# Workspace target dir lives one level up from this crate
CROSS_BIN  := ../target/$(PI_TARGET)/release/pi-agent

.PHONY: build release test check clean run cross deploy help

# ── Local (host) targets ───────────────────────────────────────────────────────

build:
	cargo build

release:
	cargo build --release

test:
	cargo test

check:
	cargo check && cargo clippy -- -D warnings

clean:
	cargo clean

run:
	cargo run -- --port $(PORT)

# ── Cross-compilation ──────────────────────────────────────────────────────────

## Build a release binary for Raspberry Pi (aarch64-unknown-linux-gnu).
##
## Requires one of:
##   cross  (default) — Docker-based, install with:
##                       cargo install cross --git https://github.com/cross-rs/cross
##   cargo             — uses system linker from .cargo/config.toml; override with:
##                       make cross CROSS_TOOL=cargo
cross:
	rustup target add $(PI_TARGET)
	$(CROSS_TOOL) build --release --target $(PI_TARGET)

## Copy the cross-compiled binary to the Pi over SSH.
## Usage: make deploy PI_HOST=pi5.local [PI_USER=pi]
deploy: cross
	@test -n "$(PI_HOST)" || { echo "Error: set PI_HOST=hostname-or-ip  (e.g. make deploy PI_HOST=pi5.local)"; exit 1; }
	scp $(CROSS_BIN) $(PI_USER)@$(PI_HOST):/usr/local/bin/pi-agent
	@echo "Deployed to $(PI_USER)@$(PI_HOST):/usr/local/bin/pi-agent"

# ── Help ───────────────────────────────────────────────────────────────────────

help:
	@echo "Targets:"
	@echo "  build              Debug build for host"
	@echo "  release            Release build for host"
	@echo "  test               Run tests"
	@echo "  check              cargo check + clippy"
	@echo "  clean              Remove build artefacts"
	@echo "  run                cargo run (debug, port $(PORT))"
	@echo ""
	@echo "  cross              Cross-compile release binary for $(PI_TARGET)"
	@echo "                     CROSS_TOOL=$(CROSS_TOOL)  (override: CROSS_TOOL=cargo)"
	@echo "  deploy PI_HOST=…   cross + scp to Pi  (PI_USER=$(PI_USER))"
