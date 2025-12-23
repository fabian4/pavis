BUILDER ?= builder

.PHONY: all build test fmt lint clean run-pavis run-pavis-xds help e2e e2e-down

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
ci: fmt-check lint test

# Build release binary
binary-build:
	cargo build --release --workspace

# Build Docker image with local cache
docker-build-local:
	DOCKER_BUILDKIT=1 docker buildx build \
		--builder $(BUILDER) \
		--file crates/pavis/Dockerfile \
		--tag pavis:local \
		--cache-from=type=local,src=.buildx-cache \
		--cache-to=type=local,dest=.buildx-cache,mode=max \
		--load \
		.

# Build Docker image with GitHub Actions cache
docker-build-ci:
	docker buildx build \
		--file crates/pavis/Dockerfile \
		--tag pavis:ci \
		--cache-from=type=gha \
		--cache-to=type=gha,mode=max \
		--load \
		.

# Run E2E Tests (Binary Mode - Default)
e2e: e2e-binary

# Run E2E Tests (Binary Mode: Local Pavis + Docker Backends)
e2e-binary:
	TEST_MODE=binary bash ./tests/e2e.sh

# Run E2E Tests (Docker Mode: All Containers)
e2e-docker:
	TEST_MODE=docker bash ./tests/e2e.sh

# Stop E2E Environment
e2e-down:
	cd tests && docker compose down

# Run Pavis (Engine)
run-pavis:
	RUST_LOG=debug cargo run -p pavis -- --config crates/pavis/config.yaml

# Run Pavis xDS (Controller)
run-pavis-xds:
	cargo run -p pavis-xds

# Clean build artifacts
clean:
	cargo clean

# Show help
help:
	@echo "Available targets:"
	@echo "  build              - Build all crates in the workspace"
	@echo "  test               - Run tests for all crates"
	@echo "  fmt                - Format code using cargo fmt"
	@echo "  lint               - Lint code using cargo clippy"
	@echo "  ci                 - Run all CI checks (fmt, test, lint)"
	@echo "  binary-build       - Build release binary"
	@echo "  docker-build-local - Build Docker image (Local cache)"
	@echo "  docker-build-ci    - Build Docker image (GHA cache)"
	@echo "  e2e                - Run E2E tests (Default: Binary)"
	@echo "  e2e-binary         - Run E2E tests with local binary and Docker backends"
	@echo "  e2e-docker         - Run E2E tests fully containerized"
	@echo "  e2e-down           - Stop E2E environment"
	@echo "  run-pavis          - Run the Pavis application"
	@echo "  run-pavis-xds      - Run the Pavis xDS application"
	@echo "  clean              - Clean build artifacts"