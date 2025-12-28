# pavctl

`pavctl` is the primary command-line interface for managing Pavis. It handles the lifecycle of `.pvs` binary configuration files, provides observability into the data plane, and orchestrates runtime updates.

## Core Functionality

### 1. PVS Binary File Operations
Generate, parse, and validate the optimized binary protocol.

- **gen**: Compile high-level configurations (like YAML) into `.pvs` binary files.
  ```bash
  pavctl gen <input_file> [output_file]
  ```
- **view**: View the logical configuration tree and protocol metadata.
  ```bash
  pavctl view [-x] <pvs_file>
  ```
- **check**: Verify configuration integrity and semantics.
  ```bash
  pavctl check <input_file>
  ```
- **convert**: Reconstruct source configuration from a binary file.
  ```bash
  pavctl convert <pvs_file> [output_file]
  ```

### 2. Runtime Interaction (Planned)
Interface with the active proxy system via the xDS bridge.

- **Apply**: Push a `.pvs` configuration to active proxy instances.
- **Status**: Monitor health and active configuration versions.
- **Logs**: Stream proxy logs for troubleshooting.

### 3. Configuration Management (Planned)
- **Rollback**: Revert to a previous configuration version.
- **Simulate**: Predict routing outcomes without affecting traffic.
- **Visualize**: Render the logical configuration structure.

## Integration
`pavctl` is designed to work with the **Split Data Plane** architecture, ensuring that heavy parsing and validation happen at the control level (the CLI or xDS Bridge), keeping the proxy runtime lightweight and fast.
