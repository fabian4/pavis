.PHONY: bench-all-build
.PHONY: bench-standalone bench-standalone-build bench-standalone-down
.PHONY: bench-system bench-system-build bench-system-down bench-report

# ============================================================================
# Standalone Mode (Docker Compose) Targets
# ============================================================================

# Build images required for standalone mode benchmarking
bench-standalone-build:
	$(MAKE) binary-build CRATE=pavctl
	$(MAKE) binary-build CRATE=pavis-benchkit BIN=bench-loadgen
	$(MAKE) docker-build IMAGE=pavis MODE=$(MODE)
	$(MAKE) docker-build IMAGE=bench-upstream MODE=$(MODE)

# Run case scripts (from bench/cases/standalone) for Pavis in standalone mode
# Environment variables:
#   PROXY=<pavis>                      - Target proxy (default: pavis)
#   CASE="<case1> <case2> ..."         - Space-separated test cases (default: all)
#   DRY_RUN=1                          - Validate setup without running benchmarks
#
# Examples:
#   make bench-standalone                              # Run all tests with pavis
#   DRY_RUN=1 make bench-standalone                   # Quick validation
#   CASE="throughput_short_1x" make bench-standalone  # Single test case
bench-standalone:
	@MODE=standalone PROXY=$${PROXY:-pavis} CASE="$${CASE:-}" bash bench/run.sh

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

# Run system mode benchmarks for Pavis
# Environment variables:
#   PROXY=<pavis>               - Target proxy (default: pavis)
#   CASE="<case1> <case2> ..."   - Space-separated test cases (default: system mode cases)
#   DRY_RUN=1                    - Validate setup without running benchmarks
#
# Examples:
#   make bench-system                                    # Run system tests with pavis
#   CASE="stress_recovery" make bench-system             # Single system test
bench-system:
	@MODE=system PROXY=$${PROXY:-pavis} BENCH_PROFILE=$${BENCH_PROFILE:-workstation} CASE="$${CASE:-}" bash bench/run.sh

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
