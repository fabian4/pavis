.PHONY: benchmark benchmark-build benchmark-pre-run benchmark-single benchmark-down

# Build images required for benchmarking
benchmark-build:
	docker buildx build --file crates/pavis/Dockerfile --tag pavis:bench --load .

# Cleanup previous benchmark results
benchmark-pre-run:
	rm -rf bench/output
	mkdir -p bench/output

# Run full benchmark matrix for all proxies
benchmark: benchmark-build benchmark-pre-run
	cd bench && BENCHMARK_TARGET=pavis bash scripts/run.sh
	cd bench && BENCHMARK_TARGET=envoy bash scripts/run.sh
	cd bench && BENCHMARK_TARGET=nginx bash scripts/run.sh
	cd bench && BENCHMARK_TARGET=haproxy bash scripts/run.sh
	RESULTS_DIR=bench/output bash bench/scripts/csv.sh
	RESULTS_DIR=bench/output bash bench/scripts/summary.sh

# Run benchmark for a specific proxy (e.g., PROXY=envoy make benchmark-single)
benchmark-single: benchmark-build
	cd bench && BENCHMARK_TARGET=$${PROXY:-pavis} bash scripts/run.sh
	RESULTS_DIR=bench/output bash bench/scripts/csv.sh
	RESULTS_DIR=bench/output bash bench/scripts/summary.sh

# Stop and cleanup the benchmark environment
benchmark-down:
	cd bench && docker compose down -v
