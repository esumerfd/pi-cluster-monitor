PI_TARGET  ?= aarch64-unknown-linux-gnu
CROSS_TOOL ?= cross
PORT       ?= 8765

.PHONY: build release test check clean run run-agent cross-agent help

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

# ── pi-agent (cross-compilation for Raspberry Pi) ─────────────────────────────

cross-agent:
	rustup target add $(PI_TARGET)
	$(CROSS_TOOL) build --release --target $(PI_TARGET) -p pi-agent

help:
	@echo "Workspace targets (both crates):"
	@echo "  build              Debug build of pi-monitor + pi-agent"
	@echo "  release            Release build of pi-monitor + pi-agent"
	@echo "  test               Run all tests"
	@echo "  check              cargo check + clippy (all crates)"
	@echo "  clean              Remove build artefacts"
	@echo ""
	@echo "Run targets:"
	@echo "  run                Run pi-monitor (debug)"
	@echo "  run-agent          Run pi-agent (debug, port $(PORT))"
	@echo ""
	@echo "pi-agent targets (Raspberry Pi):"
	@echo "  cross-agent        Cross-compile pi-agent for $(PI_TARGET)"
	@echo "                     CROSS_TOOL=$(CROSS_TOOL)  (override: CROSS_TOOL=cargo)"
