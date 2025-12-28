.PHONY: docs

# Generate API documentation for all crates
docs:
	cargo doc --workspace --no-deps
