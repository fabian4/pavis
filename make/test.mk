.PHONY: test coverage coverage-html

# Run unit and doc tests for all crates (excluding E2E)
test:
	cargo test --workspace --exclude pavis-e2e

# Run code coverage summary (requires cargo-llvm-cov)
coverage:
	cargo llvm-cov --workspace --exclude pavis-e2e

# Generate HTML code coverage report
coverage-html:
	cargo llvm-cov --workspace --exclude pavis-e2e --exclude-files 'crates/pavis-e2e/*' --exclude-files 'crates/*/tests/*' --html
