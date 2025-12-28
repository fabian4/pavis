.PHONY: test test-integration test-cli coverage coverage-html

# Run unit and doc tests for all crates (excluding E2E)
test:
	cargo test --workspace --exclude pavis-e2e

# Run integration tests
test-integration:
	cargo test --test integration

# Run CLI tests (requires binary build first)
test-cli:
	cargo build -p pavis
	cargo test --test cli

# Run code coverage summary (requires cargo-llvm-cov)
coverage:
	cargo llvm-cov --workspace --exclude pavis-e2e

# Generate HTML code coverage report
coverage-html:
	cargo llvm-cov --workspace --exclude pavis-e2e --html
