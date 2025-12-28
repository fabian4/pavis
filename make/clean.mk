.PHONY: clean

# Remove all build artifacts and generated files
clean:
	cargo clean
	rm -rf crates/pavis-e2e/config/generated_*
	rm -rf bench/output
