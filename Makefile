.PHONY: all build test fmt lint clean run-aegis run-raven help e2e e2e-down

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

# CI pipeline (format check, test, lint)
ci: fmt-check test lint

# Run E2E Environment (Docker Compose)
e2e:
	cd e2e && docker-compose up --build -d
	@echo "Environment started. Running tests..."
	./e2e/test.sh

# Stop E2E Environment
e2e-down:
	cd e2e && docker-compose down

# Run Aegis (Engine)
run-aegis:
	RUST_LOG=debug cargo run -p aegis -- --config crates/aegis/config.yaml

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
