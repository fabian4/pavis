.PHONY: bench-all-build
.PHONY: bench-standalone bench-standalone-build bench-standalone-down bench-standalone-all
.PHONY: bench-system bench-system-build bench-system-down bench-system-all bench-report

# ============================================================================
# Standalone Mode (Docker Compose) Targets
# ============================================================================

# Build images required for standalone mode benchmarking
bench-standalone-build:
	$(MAKE) binary-build CRATE=pavctl
	$(MAKE) binary-build CRATE=pavis-benchkit BIN=bench-loadgen
	$(MAKE) docker-build IMAGE=pavis MODE=$(MODE)
	$(MAKE) docker-build IMAGE=bench-upstream MODE=$(MODE)

# Run case scripts (from bench/cases/standalone) for a single proxy in standalone mode
# Environment variables:
#   PROXY=<pavis|envoy|nginx|haproxy>  - Target proxy (default: pavis)
#   CASE="<case1> <case2> ..."         - Space-separated test cases (default: all)
#   DRY_RUN=1                          - Validate setup without running benchmarks
#
# Examples:
#   make bench-standalone                              # Run all tests with pavis
#   DRY_RUN=1 make bench-standalone                   # Quick validation
#   PROXY=envoy make bench-standalone                 # Test envoy
#   CASE="throughput_short_1x" make bench-standalone  # Single test case
bench-standalone:
	@MODE=standalone PROXY=$${PROXY:-pavis} CASE="$${CASE:-}" bash bench/run.sh

# Run benchmarks for all proxies sequentially (standalone mode)
bench-standalone-all:
	@for proxy in pavis envoy nginx haproxy; do \
		MODE=standalone PROXY="$$proxy" BENCH_PROFILE="$${BENCH_PROFILE:-workstation}" CASE="$${CASE:-}" bash bench/run.sh; \
	done

# Stop and cleanup the benchmark environment (standalone mode)
bench-standalone-down:
	cd bench && docker compose down -v

# ============================================================================
# Backward Compatibility Aliases
# ============================================================================

bench-all-build:
	$(MAKE) binary-build CRATE=pavctl
	$(MAKE) docker-build IMAGE=relay MODE=$(MODE)
	$(MAKE) docker-build IMAGE=pavis MODE=$(MODE)
	$(MAKE) docker-build IMAGE=bench-upstream MODE=$(MODE)
	$(MAKE) binary-build CRATE=pavis-benchkit BIN=bench-loadgen
	@if [ "$${BENCH_PROFILE:-workstation}" != "github" ]; then \
		echo "Building envoy xDS server image..."; \
		$(MAKE) docker-build IMAGE=envoy-xds-server MODE=$(MODE); \
	else \
		echo "Skipping envoy xDS server image for BENCH_PROFILE=github"; \
	fi

# ============================================================================
# System Mode (Kubernetes) Targets
# ============================================================================

# Build images required for system mode benchmarking
bench-system-build:
	@echo "Building Docker images for system mode..."
	$(MAKE) binary-build CRATE=pavctl
	$(MAKE) docker-build IMAGE=pavis MODE=$(MODE)
	$(MAKE) docker-build IMAGE=relay MODE=$(MODE)
	$(MAKE) docker-build IMAGE=bench-upstream MODE=$(MODE)
	$(MAKE) binary-build CRATE=pavis-benchkit BIN=bench-loadgen
	@if [ "$${BENCH_PROFILE:-workstation}" != "github" ]; then \
		echo "Building envoy xDS server image..."; \
		$(MAKE) docker-build IMAGE=envoy-xds-server MODE=$(MODE); \
	else \
		echo "Skipping envoy xDS server image for BENCH_PROFILE=github"; \
	fi

# Run system mode benchmarks for a single proxy
# Environment variables:
#   PROXY=<pavis|envoy|linkerd>  - Target proxy (default: pavis)
#   CASE="<case1> <case2> ..."   - Space-separated test cases (default: system mode cases)
#   DRY_RUN=1                    - Validate setup without running benchmarks
#
# Examples:
#   make bench-system                                    # Run system tests with pavis
#   PROXY=linkerd make bench-system                      # Test linkerd
#   PROXY=envoy CASE="stress_recovery" make bench-system # Single system test
bench-system:
	@MODE=system PROXY=$${PROXY:-pavis} BENCH_PROFILE=$${BENCH_PROFILE:-workstation} CASE="$${CASE:-}" bash bench/run.sh

# Run system mode benchmarks for all supported proxies
bench-system-all:
	@for proxy in pavis envoy linkerd; do \
		MODE=system PROXY="$$proxy" BENCH_PROFILE=workstation CASE="$${CASE:-}" bash bench/run.sh; \
	done

# Cleanup system mode environment (delete kind cluster)
bench-system-down:
	@BENCH_CLEANUP_FORCE=true bash bench/scripts/cleanup.sh || true
	@kind delete cluster --name pavis-bench 2>/dev/null || true

# ============================================================================
# Shared Targets
# ============================================================================

# Generate summary CSV and markdown report from existing results
bench-report:
	@bash bench/scripts/report.sh
