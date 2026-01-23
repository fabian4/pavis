.PHONY: e2e e2e-binary e2e-docker e2e-pavis e2e-pavis-binary e2e-pavis-docker e2e-relay e2e-relay-binary e2e-relay-docker e2e-integrated e2e-integrated-binary e2e-integrated-docker

# Run all E2E tests (defaults to binary mode)
e2e: e2e-binary

# Run all E2E tests in binary mode
e2e-binary: e2e-pavis-binary e2e-relay-binary e2e-integrated-binary

# Run all E2E tests in docker mode
e2e-docker: e2e-pavis-docker e2e-relay-docker e2e-integrated-docker

# Run Pavis E2E tests (defaults to binary mode)
e2e-pavis: e2e-pavis-binary

# Run Pavis E2E tests in binary mode (local proxy, dockerized backends)
e2e-pavis-binary:
	TEST_MODE=binary bash tests/run.sh pavis $(CASE)

# Run Pavis E2E tests in docker mode (all components dockerized)
e2e-pavis-docker:
	TEST_MODE=docker bash tests/run.sh pavis $(CASE)

# Run Relay E2E tests (defaults to binary mode)
e2e-relay: e2e-relay-binary

# Run Relay E2E tests in binary mode
e2e-relay-binary:
	TEST_MODE=binary bash tests/run.sh relay $(CASE)

# Run Relay E2E tests in docker mode
e2e-relay-docker:
	TEST_MODE=docker bash tests/run.sh relay $(CASE)

# Run Integrated E2E tests (defaults to binary mode)
e2e-integrated: e2e-integrated-binary

# Run Integrated E2E tests in binary mode
e2e-integrated-binary:
	TEST_MODE=binary bash tests/run.sh integrated $(CASE)

# Run Integrated E2E tests in docker mode
e2e-integrated-docker:
	TEST_MODE=docker bash tests/run.sh integrated $(CASE)