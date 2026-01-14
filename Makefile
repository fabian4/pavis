# ==============================================================================
# Pavis Workspace Makefile
# ==============================================================================
# This Makefile uses a modular approach. Specific task implementations are
# located in the make/ directory.

# Default target
.PHONY: all
all: build

# Include sub-modules
include make/build.mk
include make/test.mk
include make/e2e.mk
include make/bench.mk
include make/clean.mk
include make/docs.mk

# High-level Orchestration
.PHONY: ci ci-local help

# CI pipeline (format check, lint, shellcheck, unit tests)
ci: fmt-check lint shellcheck test

# Local CI pipeline (format, lint, shellcheck, unit tests)
ci-local: fmt lint shellcheck test

# Show available commands
help:
	@echo "Pavis Build System"
	@echo ""
	@echo "Build Commands:"
	@echo "  build              - Build debug workspace"
	@echo "  binary-build       - Build release binaries (CRATE=workspace|pavis|relay|... [BIN=name])"
	@echo "  docker-build       - Build docker image (IMAGE=pavis|relay|bench-upstream|envoy-xds-server, MODE=local|ci)"
	@echo "  docker-images      - Build all docker images (MODE=local|ci)"
	@echo "  coverage-report    - Generate coverage markdown at ./docs/coverage.md"
	@echo "  run-pavis          - Run proxy engine"
	@echo "  run-relay          - Run relay service"
	@echo "  run-upstream       - Run pavis-upstream fixture (requires TLS_CERT_FILE/TLS_KEY_FILE)"
	@echo "  fmt                - Format code"
	@echo "  lint               - Run clippy"
	@echo "  shellcheck         - Run shellcheck on bash scripts"
	@echo ""
	@echo "Test Commands:"
	@echo "  test               - Run unit tests"
	@echo "  e2e                - Run end-to-end tests"
	@echo "  coverage-html      - Generate coverage report"
	@echo ""
	@echo "Benchmarking:"
	@echo "  Standalone Mode (Docker Compose):"
	@echo "    bench-standalone       - Run benchmarks (PROXY=pavis|envoy|nginx|haproxy CASE=\"...\" DRY_RUN=1)"
	@echo "    bench-standalone-all   - Run benchmarks for all proxies sequentially"
	@echo "    bench-standalone-build - Build benchmark images for standalone mode"
	@echo "    bench-standalone-down  - Stop and cleanup benchmark containers"
	@echo ""
	@echo "  System Mode (Kubernetes):"
	@echo "    bench-system       - Run system mode benchmarks (PROXY=pavis|envoy|linkerd CASE=\"...\")"
	@echo "    bench-system-all   - Run system mode benchmarks for all proxies"
	@echo "    bench-system-build - Build Docker images for system mode"
	@echo "    bench-system-down  - Cleanup system mode environment (delete kind cluster)"
	@echo ""
	@echo "  Shared:"
	@echo "    bench-report       - Aggregate results and generate markdown report"
	@echo ""
	@echo "  Aliases (backward compatibility):"
	@echo "    bench              - Alias for bench-standalone"
	@echo "    bench-all          - Alias for bench-standalone-all"
	@echo "    bench-build        - Alias for bench-standalone-build"
	@echo "    bench-down         - Alias for bench-standalone-down"
	@echo ""
	@echo "Maintenance:"
	@echo "  clean              - Cleanup build artifacts and generated files"
	@echo "  clean-docker       - Remove Docker build cache and containers"
	@echo "  clean-all          - Remove all artifacts, cache, and Docker resources"
	@echo "  docs               - Generate documentation"
	@echo "  ci                 - Run full CI suite"
