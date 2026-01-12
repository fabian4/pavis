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

# CI pipeline (format check, lint, unit tests)
ci: fmt-check lint test

# Local CI pipeline (format, lint, unit tests)
ci-local: fmt lint test

# Show available commands
help:
	@echo "Pavis Build System"
	@echo ""
	@echo "Build Commands:"
	@echo "  build              - Build debug workspace"
	@echo "  binary-build       - Build release binaries (CRATE=workspace|pavis|relay|...)"
	@echo "  docker-build       - Build docker image (IMAGE=pavis|relay|upstream|mock-upstream|mock-relay, MODE=local|ci)"
	@echo "  docker-images      - Build all docker images (MODE=local|ci)"
	@echo "  coverage-report    - Generate coverage markdown at ./docs/coverage.md"
	@echo "  run-pavis          - Run proxy engine"
	@echo "  run-relay          - Run relay service"
	@echo "  run-upstream       - Run pavis-upstream fixture (requires TLS_CERT_FILE/TLS_KEY_FILE)"
	@echo "  fmt                - Format code"
	@echo "  lint               - Run clippy"
	@echo ""
	@echo "Test Commands:"
	@echo "  test               - Run unit tests"
	@echo "  e2e                - Run end-to-end tests"
	@echo "  coverage-html      - Generate coverage report"
	@echo ""
	@echo "Benchmarking:"
	@echo "  bench              - Run benchmarks (PROXY=pavis|envoy|nginx|haproxy CASE=\"...\" DRY_RUN=1)"
	@echo "  bench-report       - Aggregate results and generate markdown report"
	@echo "  bench-build        - Build benchmark images (pavis:local, bench-upstream:local)"
	@echo "  bench-down         - Stop and cleanup benchmark containers"
	@echo ""
	@echo "Maintenance:"
	@echo "  clean              - Cleanup build artifacts and generated files"
	@echo "  clean-docker       - Remove Docker build cache and containers"
	@echo "  clean-all          - Remove all artifacts, cache, and Docker resources"
	@echo "  docs               - Generate documentation"
	@echo "  ci                 - Run full CI suite"
