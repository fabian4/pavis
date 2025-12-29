.PHONY: build binary-build docker-build-local docker-build-ci run-pavis run-relay fmt fmt-check lint

BUILDER ?= builder
ROOT_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST)))/..)

# Build all crates in the workspace (debug mode)
build:
	cargo build --workspace

# Build all crates in the workspace (release mode)
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

# Run the Pavis engine with debug logging
run-pavis:
	RUST_LOG=debug cargo run -p pavis -- --config $(ROOT_DIR)/crates/pavis/config.yaml

# Run the Pavis relay with the example config
run-relay:
	RUST_LOG=debug cargo run -p pavis-relay -- --config $(ROOT_DIR)/crates/pavis-relay/relay.yaml

# Run the Pavis xDS controller
# Format all code in the workspace
fmt:
	cargo fmt

# Check if all code is formatted
fmt-check:
	cargo fmt -- --check

# Lint all code using Clippy
lint:
	cargo clippy --workspace -- -D warnings
