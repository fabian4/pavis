.PHONY: build binary-build docker-build run-pavis run-relay run-upstream fmt fmt-check lint shellcheck coverage-report

BUILDER ?= builder
ROOT_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST)))/..)
IMAGE ?= pavis
MODE ?= local
CRATE ?= workspace
HTTP_PORT ?= 8080
HTTPS_PORT ?= 8443
INSTANCE_ID ?= pavis-upstream

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

# Build Docker image (IMAGE=pavis|relay|mock-upstream|mock-relay|bench-upstream, MODE=local|ci)
docker-build:
	@set -e; \
	BUILD_ARGS=""; \
	if [ "$(IMAGE)" = "pavis" ]; then \
		DOCKERFILE=crates/pavis/Dockerfile; \
		TAG=pavis:local; \
	elif [ "$(IMAGE)" = "relay" ]; then \
		DOCKERFILE=crates/pavis-relay/Dockerfile; \
		TAG=pavis-relay:local; \
	elif [ "$(IMAGE)" = "mock-upstream" ]; then \
		DOCKERFILE=crates/pavis-testkit/Dockerfile; \
		TAG=pavis-mock-upstream:local; \
		BUILD_ARGS="--build-arg BINARY=pavis-mock-upstream"; \
	elif [ "$(IMAGE)" = "mock-relay" ]; then \
		DOCKERFILE=crates/pavis-testkit/Dockerfile; \
		TAG=pavis-mock-relay:local; \
		BUILD_ARGS="--build-arg BINARY=pavis-mock-relay"; \
	elif [ "$(IMAGE)" = "bench-upstream" ]; then \
		DOCKERFILE=crates/pavis-benchkit/Dockerfile; \
		TAG=pavis-bench-upstream:local; \
	else \
		echo "Unsupported IMAGE=$(IMAGE) (use pavis, relay, mock-upstream, mock-relay, or bench-upstream)"; \
		exit 2; \
	fi; \
	if [ "$(MODE)" = "local" ]; then \
		DOCKER_BUILDKIT=1 docker buildx build \
			--builder $(BUILDER) \
			--file $$DOCKERFILE \
			--tag $$TAG \
			$$BUILD_ARGS \
			--cache-from=type=local,src=.buildx-cache \
			--cache-to=type=local,dest=.buildx-cache,mode=max \
			--load \
			.; \
	elif [ "$(MODE)" = "ci" ]; then \
		docker buildx build \
			--file $$DOCKERFILE \
			--tag $$TAG \
			$$BUILD_ARGS \
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

# Run the pavis-upstream fixture (requires TLS_CERT_FILE/TLS_KEY_FILE env vars)
run-upstream:
	@:${TLS_CERT_FILE:?Set TLS_CERT_FILE=/absolute/path/to/upstream.crt}
	@:${TLS_KEY_FILE:?Set TLS_KEY_FILE=/absolute/path/to/upstream.key}
	RUST_LOG=debug \
		TLS_CERT_FILE=$(TLS_CERT_FILE) \
		TLS_KEY_FILE=$(TLS_KEY_FILE) \
		HTTP_PORT=$(HTTP_PORT) \
		HTTPS_PORT=$(HTTPS_PORT) \
		INSTANCE_ID=$(INSTANCE_ID) \
		cargo run -p pavis-upstream

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
	cargo clippy --workspace --all-targets --all-features -- -D warnings -W clippy::all

# Run shellcheck on bash scripts (requires shellcheck)
shellcheck:
	@sh -c 'if command -v rg >/dev/null 2>&1; then \
		files=$$(rg --files -g "*.sh"); \
	else \
		files=$$(find . -type f -name "*.sh" \
			-not -path "./target/*" \
			-not -path "./.git/*"); \
	fi; \
	if [ -z "$$files" ]; then \
		echo "No shell scripts found."; \
		exit 0; \
	fi; \
	shellcheck -x -S warning $$files'
