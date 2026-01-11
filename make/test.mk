.PHONY: test coverage coverage-html

# Run unit and doc tests for all crates (excluding E2E)
test:
	cargo test --workspace

# Run code coverage summary (requires cargo-llvm-cov)
coverage:
	cargo llvm-cov --workspace

# Generate HTML code coverage report
coverage-html:
	cargo llvm-cov --workspace --exclude-files 'crates/*/tests/*' --html

# Generate coverage markdown (requires cargo-tarpaulin + grcov)
coverage-report:
	cargo tarpaulin --workspace --all-features --exclude-files 'crates/*/tests/*' --exclude-files "crates/**/*tests.rs" --out Lcov
	grcov lcov.info --source-dir . --output-type markdown --ignore 'crates/*/tests/*' --ignore 'crates/**/*tests.rs' --output-path ./docs/coverage.md