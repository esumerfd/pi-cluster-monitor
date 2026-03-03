MONITOR_DIR := pi-monitor
AGENT_DIR   := pi-agent
MAKE        := $(MAKE) -C

# Cross-compilation variables (forwarded to pi-agent/Makefile)
PI_TARGET  ?= aarch64-unknown-linux-gnu
CROSS_TOOL ?= cross
PI_USER    ?= pi
PI_HOST    ?=

.PHONY: build release test check clean run \
        build-agent release-agent run-agent \
        cross-agent deploy-agent \
        build-all release-all test-all \
        help

# ── pi-monitor ────────────────────────────────────────────────────────────────

build:
	$(MAKE) $(MONITOR_DIR) build

release:
	$(MAKE) $(MONITOR_DIR) release

test:
	$(MAKE) $(MONITOR_DIR) test

check:
	$(MAKE) $(MONITOR_DIR) check

clean:
	$(MAKE) $(MONITOR_DIR) clean
	$(MAKE) $(AGENT_DIR) clean

run:
	$(MAKE) $(MONITOR_DIR) run

# ── pi-agent (host) ───────────────────────────────────────────────────────────

build-agent:
	$(MAKE) $(AGENT_DIR) build

release-agent:
	$(MAKE) $(AGENT_DIR) release

run-agent:
	$(MAKE) $(AGENT_DIR) run

# ── pi-agent (cross-compilation for Raspberry Pi) ─────────────────────────────

cross-agent:
	$(MAKE) $(AGENT_DIR) cross PI_TARGET=$(PI_TARGET) CROSS_TOOL=$(CROSS_TOOL)

deploy-agent:
	@test -n "$(PI_HOST)" || { echo "Error: set PI_HOST=hostname-or-ip  (e.g. make deploy-agent PI_HOST=pi5.local)"; exit 1; }
	$(MAKE) $(AGENT_DIR) deploy PI_TARGET=$(PI_TARGET) CROSS_TOOL=$(CROSS_TOOL) PI_USER=$(PI_USER) PI_HOST=$(PI_HOST)

# ── workspace ─────────────────────────────────────────────────────────────────

build-all:
	cargo build

release-all:
	cargo build --release

test-all:
	cargo test

help:
	@echo "pi-monitor targets:"
	@echo "  build              Debug build of pi-monitor"
	@echo "  release            Release build of pi-monitor"
	@echo "  test               Run pi-monitor tests"
	@echo "  check              cargo check + clippy for pi-monitor"
	@echo "  clean              Remove build artefacts (both crates)"
	@echo "  run                cargo run pi-monitor (debug)"
	@echo ""
	@echo "pi-agent targets (host):"
	@echo "  build-agent        Debug build for host"
	@echo "  release-agent      Release build for host"
	@echo "  run-agent          cargo run pi-agent (debug, port 8765)"
	@echo ""
	@echo "pi-agent targets (Raspberry Pi):"
	@echo "  cross-agent        Cross-compile for $(PI_TARGET)"
	@echo "                     CROSS_TOOL=$(CROSS_TOOL)  (override: CROSS_TOOL=cargo)"
	@echo "  deploy-agent PI_HOST=…  cross + scp to Pi (PI_USER=$(PI_USER))"
	@echo ""
	@echo "Workspace targets:"
	@echo "  build-all          Build both crates"
	@echo "  release-all        Release build of both crates"
	@echo "  test-all           Run all workspace tests"
