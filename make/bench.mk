.PHONY: bench bench-build bench-down

# Build images required for benchmarking
bench-build:
	docker buildx build --file crates/pavis/Dockerfile --tag pavis:local --load .
	cd bench && docker compose build bench-upstream

# Run case scripts (from bench/cases) for a single proxy
# Environment variables:
#   PROXY=<pavis|envoy|nginx|haproxy>  - Target proxy (default: pavis)
#   CASE="<case1> <case2> ..."         - Space-separated test cases (default: all)
#   DRY_RUN=1                          - Validate setup without running benchmarks
#
# Examples:
#   make bench                              # Run all tests with pavis
#   DRY_RUN=1 make bench                   # Quick validation
#   PROXY=envoy make bench                 # Test envoy
#   CASE="throughput_short_1x" make bench  # Single test case
bench:
	@PROXY=$${PROXY:-pavis} CASE="$${CASE:-}" bash bench/run.sh

# Generate summary CSV and markdown report from existing results
bench-report:
	@bash bench/scripts/summarize.sh
	@bash bench/scripts/report.sh

# Stop and cleanup the benchmark environment
bench-down:
	cd bench && docker compose down -v
