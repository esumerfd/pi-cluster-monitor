PI_TARGET  ?= aarch64-unknown-linux-gnu
CROSS_TOOL ?= cross
PORT       ?= 8765

.PHONY: build release test check clean run run-agent release-pi setup help

# ── workspace ─────────────────────────────────────────────────────────────────

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

# ── run ───────────────────────────────────────────────────────────────────────

run:
	cargo run -p pi-monitor

run-agent:
	cargo run -p pi-agent -- --port $(PORT)

# ── setup ─────────────────────────────────────────────────────────────────────

setup:
	cargo install cross --git https://github.com/cross-rs/cross
	rustup target add $(PI_TARGET)

# ── release builds ────────────────────────────────────────────────────────────

release-pi:
	rustup target add $(PI_TARGET)
	DOCKER_HOST=unix:///var/run/docker.sock $(CROSS_TOOL) build --release --target $(PI_TARGET) -p pi-agent

help:
	@echo "Workspace targets (both crates):"
	@echo "  build              Debug build (host arch)"
	@echo "  release            Release build (host arch)"
	@echo "  test               Run all tests"
	@echo "  check              cargo check + clippy (all crates)"
	@echo "  clean              Remove build artefacts"
	@echo ""
	@echo "Run targets:"
	@echo "  run                Run pi-monitor (debug)"
	@echo "  run-agent          Run pi-agent (debug, port $(PORT))"
	@echo ""
	@echo "Release targets:"
	@echo "  release            target/release/          (host arch)"
	@echo "  release-pi         target/$(PI_TARGET)/release/  (Raspberry Pi)"
	@echo "                     CROSS_TOOL=$(CROSS_TOOL)  (override: CROSS_TOOL=cargo)"
	@echo ""
	@echo "Setup:"
	@echo "  setup              Install cross + rustup target for $(PI_TARGET)"
