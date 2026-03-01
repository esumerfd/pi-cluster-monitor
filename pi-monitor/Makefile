BIN     := pi-monitor
CARGO   := cargo

.PHONY: build release test check clean run help

build:
	$(CARGO) build

release:
	$(CARGO) build --release

test:
	$(CARGO) test

check:
	$(CARGO) check && $(CARGO) clippy -- -D warnings

clean:
	$(CARGO) clean

run:
	$(CARGO) run

help:
	@echo "Targets:"
	@echo "  build    Debug build (default)"
	@echo "  release  Optimised release build"
	@echo "  test     Run tests"
	@echo "  check    cargo check + clippy"
	@echo "  clean    Remove build artefacts"
	@echo "  run      cargo run (debug)"
