.PHONY: e2e e2e-binary e2e-docker e2e-down

# Run E2E tests (defaults to binary mode)
e2e: e2e-binary

# Run E2E tests in binary mode (local proxy, dockerized backends)
e2e-binary:
	TEST_MODE=binary bash ./scripts/e2e.sh

# Run E2E tests in docker mode (all components dockerized)
e2e-docker:
	TEST_MODE=docker bash ./scripts/e2e.sh

# Stop and cleanup the E2E environment
e2e-down:
	cd crates/pavis-e2e/config && docker compose down
