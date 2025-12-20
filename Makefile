.PHONY: all build test fmt lint clean run-aegis run-raven help

# Default target
all: build

# Build all crates in the workspace
build:
	cargo build --workspace

# Run tests for all crates
test:
	cargo test --workspace

# Format code
fmt:
	cargo fmt

# Check formatting
fmt-check:
	cargo fmt -- --check

# Lint code
lint:
	cargo clippy --workspace -- -D warnings

# Run Aegis (Engine)
run-aegis:
	RUST_LOG=info cargo run -p aegis -- --config crates/aegis/config.yaml

# Run Raven (Controller)
run-raven:
	cargo run -p raven

# Clean build artifacts
clean:
	cargo clean

# Show help
help:
	@echo "Available targets:"
	@echo "  build       - Build all crates in the workspace"
	@echo "  test        - Run tests for all crates"
	@echo "  fmt         - Format code using cargo fmt"
	@echo "  lint        - Lint code using cargo clippy"
	@echo "  run-aegis   - Run the Aegis application"
	@echo "  run-raven   - Run the Raven application"
	@echo "  clean       - Clean build artifacts"
