.PHONY: build binary-build docker-build run-pavis run-relay fmt fmt-check lint coverage-report

BUILDER ?= builder
ROOT_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST)))/..)
IMAGE ?= pavis
MODE ?= local
CRATE ?= workspace

# Build all crates in the workspace (debug mode)
build:
	cargo build --workspace

# Build release binaries (CRATE=workspace|pavis|pavis-relay|...)
binary-build:
	@set -e; \
	if [ "$(CRATE)" = "workspace" ]; then \
		cargo build --release --workspace; \
	else \
		CRATE_NAME="$(CRATE)"; \
		if [ "$$CRATE_NAME" = "relay" ]; then \
			CRATE_NAME="pavis-relay"; \
		fi; \
		cargo build --release -p $$CRATE_NAME; \
	fi

# Build Docker image (IMAGE=pavis|relay, MODE=local|ci)
docker-build:
	@set -e; \
	if [ "$(IMAGE)" = "pavis" ]; then \
		DOCKERFILE=crates/pavis/Dockerfile; \
		TAG=pavis:local; \
	elif [ "$(IMAGE)" = "relay" ]; then \
		DOCKERFILE=crates/pavis-relay/Dockerfile; \
		TAG=pavis-relay:local; \
	else \
		echo "Unsupported IMAGE=$(IMAGE) (use pavis or relay)"; \
		exit 2; \
	fi; \
	if [ "$(MODE)" = "local" ]; then \
		DOCKER_BUILDKIT=1 docker buildx build \
			--builder $(BUILDER) \
			--file $$DOCKERFILE \
			--tag $$TAG \
			--cache-from=type=local,src=.buildx-cache \
			--cache-to=type=local,dest=.buildx-cache,mode=max \
			--load \
			.; \
	elif [ "$(MODE)" = "ci" ]; then \
		docker buildx build \
			--file $$DOCKERFILE \
			--tag $$TAG \
			--cache-from=type=gha \
			--cache-to=type=gha,mode=max \
			--load \
			.; \
	else \
		echo "Unsupported MODE=$(MODE) (use local or ci)"; \
		exit 2; \
	fi

# Run the Pavis engine with debug logging
run-pavis:
	RUST_LOG=debug cargo run -p pavis -- --config $(ROOT_DIR)/crates/pavis/config.yaml

# Run the Pavis relay with the example config
run-relay:
	RUST_LOG=debug cargo run -p pavis-relay -- --config $(ROOT_DIR)/crates/pavis-relay/relay.yaml

audit:
	cargo audit

udeps:
	cargo +nightly udeps --workspace --all-targets

# Format all code in the workspace
fmt:
	cargo fmt

# Check if all code is formatted
fmt-check:
	cargo fmt -- --check

# Lint all code using Clippy
lint:
	cargo clippy --workspace --all-targets --all-features -- -D warnings

# Generate coverage markdown (requires cargo-tarpaulin + grcov)
coverage-report:
	cargo tarpaulin -e pavis-e2e --workspace --all-features --exclude-files 'crates/pavis-e2e/*' --exclude-files 'crates/*/tests/*' --exclude-files "crates/**/*tests.rs" --out Lcov
	grcov lcov.info --source-dir . --output-type markdown --ignore 'crates/pavis-e2e/*' --ignore 'crates/*/tests/*' --ignore 'crates/**/*tests.rs' --output-path ./docs/coverage.md
