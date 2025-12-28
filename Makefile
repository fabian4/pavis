# ==============================================================================
# Pavis Workspace Makefile
# ==============================================================================
# This Makefile uses a modular approach. Specific task implementations are
# located in the make/ directory.

# Parallel execution support
MAKEFLAGS += -j$(shell nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 1)

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

# CI pipeline (format check, lint, unit tests, integration tests)
ci: fmt-check lint test test-integration

# Local CI pipeline (format, lint, unit tests)
ci-local: fmt lint test

# Show available commands
help:
	@echo "Pavis Build System"
	@echo ""
	@echo "Build Commands:"
	@echo "  build              - Build debug workspace"
	@echo "  binary-build       - Build release workspace"
	@echo "  docker-build-local - Build local docker image"
	@echo "  run-pavis          - Run proxy engine"
	@echo "  fmt                - Format code"
	@echo "  lint               - Run clippy"
	@echo ""
	@echo "Test Commands:"
	@echo "  test               - Run unit tests"
	@echo "  test-integration   - Run integration tests"
	@echo "  test-cli           - Run CLI binary tests"
	@echo "  e2e                - Run end-to-end tests"
	@echo "  coverage-html      - Generate coverage report"
	@echo ""
	@echo "Benchmarking:"
	@echo "  benchmark          - Run full performance matrix"
	@echo "  benchmark-single   - Run specific proxy (PROXY=envoy)"
	@echo ""
	@echo "Maintenance:"
	@echo "  clean              - Cleanup artifacts"
	@echo "  docs               - Generate documentation"
	@echo "  ci                 - Run full CI suite"