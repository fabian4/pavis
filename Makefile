BUILDER ?= builder

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

# Build release binary
binary-build:
	cargo build --release --workspace

# Build Docker image with local cache
docker-build-local:
	DOCKER_BUILDKIT=1 docker buildx build \
		--builder $(BUILDER) \
		--file crates/aegis/Dockerfile \
		--tag aegis:local \
		--cache-from=type=local,src=.buildx-cache \
		--cache-to=type=local,dest=.buildx-cache,mode=max \
		--load \
		.

# Build Docker image with GitHub Actions cache
docker-build-ci:
	docker buildx build \
		--file crates/aegis/Dockerfile \
		--tag aegis:ci \
		--cache-from=type=gha \
		--cache-to=type=gha,mode=max \
		--load \
		.

# Run E2E Tests (Binary Mode - Default)
e2e: e2e-binary

# Run E2E Tests (Binary Mode: Local Aegis + Docker Backends)
e2e-binary:
	TEST_MODE=binary bash ./crates/aegis/tests/e2e.sh

# Run E2E Tests (Docker Mode: All Containers)
e2e-docker:
	TEST_MODE=docker bash ./crates/aegis/tests/e2e.sh

# Stop E2E Environment
e2e-down:
	cd crates/aegis/tests && docker compose down

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
	@echo "  run-aegis          - Run the Aegis application"
	@echo "  run-raven          - Run the Raven application"
	@echo "  clean              - Clean build artifacts"
