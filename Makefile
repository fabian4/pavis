BUILDER ?= builder

.PHONY: all build test fmt lint clean run-pavis run-pavis-xds help e2e e2e-down benchmark benchmark-pavis benchmark-envoy benchmark-down

# Default target
all: build

# Build all crates in the workspace
build:
	cargo build --workspace

# Run tests for all crates (excluding E2E which requires binary)
test:
	cargo test --workspace --exclude pavis-e2e

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
	TEST_MODE=binary bash ./scripts/e2e.sh

# Run E2E Tests (Docker Mode: All Containers)
e2e-docker:
	TEST_MODE=docker bash ./scripts/e2e.sh

# Stop E2E Environment
e2e-down:
	cd crates/pavis-e2e/config && docker compose down

# Run Pavis (Engine)
run-pavis:
	RUST_LOG=debug cargo run -p pavis -- --config crates/pavis/config.yaml

# Run Pavis xDS (Controller)
run-pavis-xds:
	cargo run -p pavis-xds

# Clean build artifacts
clean:
	cargo clean

# ========================================
# Benchmark Targets
# ========================================

# Build benchmark images
benchmark-build:
	docker buildx build --file crates/pavis/Dockerfile --tag pavis:bench --load .

# Clean results before run
benchmark-pre-run:
	rm -rf bench/output
	mkdir -p bench/output

# Run full benchmark matrix (44 runs = 11 per proxy × 4 proxies)
benchmark: benchmark-build
	cd bench && BENCHMARK_TARGET=pavis bash scripts/run.sh
	cd bench && BENCHMARK_TARGET=envoy bash scripts/run.sh
	cd bench && BENCHMARK_TARGET=nginx bash scripts/run.sh
	cd bench && BENCHMARK_TARGET=haproxy bash scripts/run.sh
	RESULTS_DIR=bench/output bash bench/scripts/csv.sh
	RESULTS_DIR=bench/output bash bench/scripts/summary.sh

# Run benchmark for a single proxy (use: PROXY=envoy make benchmark-single)
benchmark-single: benchmark-build
	cd bench && BENCHMARK_TARGET=$${PROXY:-pavis} bash scripts/run.sh
	RESULTS_DIR=bench/output bash bench/scripts/csv.sh
	RESULTS_DIR=bench/output bash bench/scripts/summary.sh

# Stop benchmark environment
benchmark-down:
	cd bench && docker compose down -v

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
	@echo "  e2e-binary         - Run E2E tests using scripts/e2e.sh in Binary"
	@echo "  e2e-docker         - Run E2E tests using scripts/e2e.sh in Docker"
	@echo "  e2e-down           - Stop E2E environment"
	@echo ""
	@echo "Benchmark targets:"
	@echo "  benchmark          - Run full benchmark (44 runs, all proxies)"
	@echo "  benchmark-single   - Run single proxy (PROXY=envoy make benchmark-single)"
	@echo "  benchmark-down     - Stop benchmark environment"
	@echo ""
	@echo "Other targets:"
	@echo "  run-pavis          - Run the Pavis application"
	@echo "  run-pavis-xds      - Run the Pavis xDS application"
	@echo "  clean              - Clean build artifacts"