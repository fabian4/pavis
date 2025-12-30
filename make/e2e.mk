.PHONY: e2e e2e-binary e2e-docker e2e-pavis e2e-pavis-binary e2e-pavis-docker e2e-relay e2e-relay-binary e2e-relay-docker e2e-down

# Run all E2E tests (defaults to binary mode)
e2e: e2e-binary

# Run all E2E tests in binary mode
e2e-binary: e2e-pavis-binary e2e-relay-binary

# Run all E2E tests in docker mode
e2e-docker: e2e-pavis-docker e2e-relay-docker

# Run Pavis E2E tests (defaults to binary mode)
e2e-pavis: e2e-pavis-binary

# Run Pavis E2E tests in binary mode (local proxy, dockerized backends)
e2e-pavis-binary:
	TEST_MODE=binary bash ./crates/pavis-e2e/scripts/e2e-pavis.sh

# Run Pavis E2E tests in docker mode (all components dockerized)
e2e-pavis-docker:
	TEST_MODE=docker bash ./crates/pavis-e2e/scripts/e2e-pavis.sh

# Run Relay E2E tests (defaults to binary mode)
e2e-relay: e2e-relay-binary

# Run Relay E2E tests in binary mode
e2e-relay-binary:
	TEST_MODE=binary bash ./crates/pavis-e2e/scripts/e2e-relay.sh

# Run Relay E2E tests in docker mode
e2e-relay-docker:
	TEST_MODE=docker bash ./crates/pavis-e2e/scripts/e2e-relay.sh

# Stop and cleanup the E2E environment
e2e-down:
	cd crates/pavis-e2e/config && docker compose down
