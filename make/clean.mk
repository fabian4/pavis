.PHONY: clean clean-docker clean-all

# Remove build artifacts and generated files
clean:
	cargo clean
	rm -rf bench/output
	rm -rf target/
	find . -type f -name "*.pvs" -delete

# Remove Docker build cache and containers
clean-docker:
	docker system prune -af --volumes

# Remove all artifacts, cache, and Docker resources
clean-all: clean clean-docker
	@echo "All build artifacts, cache, and Docker resources cleaned"
