# Pavis CLI (pavctl)

## 1. Crate Overview
`pavctl` is the command-line interface for the Pavis proxy system. Its primary responsibility is managing the lifecycle of configuration artifacts. It acts as the bridge between human-readable configuration formats (YAML/JSON) and the machine-optimized `.pvs` binary format required by the Pavis runtime.

Its capabilities include:
- Compiling high-level configurations into validated binary artifacts.
- Inspecting the structure and metadata of binary artifacts.
- converting binary artifacts back to human-readable formats.
- validating configuration files against the schema without artifact generation.

It explicitly does not:
- Run the proxy server or handle network traffic.
- Manage the runtime state of running proxy instances (currently).
- modify binary files in-place.

## 2. Features
- **Binary Compilation**: The `gen` command compiles YAML or JSON input into `.pvs` binaries, performing full semantic validation during the process.
- **Artifact Inspection**: The `view` command decodes binary files to display headers, logical configuration trees, and size statistics. It also supports raw hex dumping of the payload.
- **Reverse Conversion**: The `convert` command reconstructs YAML or JSON configurations from `.pvs` binaries, allowing for verification and debugging of compiled artifacts.
- **Dry-Run Validation**: The `check` command verifies the syntax and semantics of a configuration file against the Pavis schema without producing output.
- **Format Agnostic**: Automatically detects formats based on file extensions, supporting `.yaml`, `.yml`, and `.json`.

## 3. Module Breakdown

### `commands`
Contains the implementation logic for each CLI subcommand.
- `gen`: Handles file I/O and calls the codec to materialize binaries.
- `view`: Reads binary headers and payloads, using `format` to display them.
- `check`: Performs ingestion and materialization to test validity, discarding the result.
- `convert`: Loads a binary and re-serializes it to the requested text format.

### `format`
Provides logic for rendering internal data structures into human-readable strings.
- `format_header`: Displays protocol version, algorithm, and checksums.
- `format_config`: Renders the logical tree of listeners, upstreams, and routes.
- `format_stats`: Calculates and displays compression and element count statistics.

### `parse`
Abstracts the complexity of loading configuration from bytes. It bridges `pavis-ingest-api` and `pavis-codec-serde` to produce a valid `RuntimeConfig` from raw input.

### `main` (Binary Entry)
Defines the `clap` CLI structure (subcommands and arguments) and dispatches execution to the appropriate command module. It initializes logging/tracing.

## 4. Public API Surface

While primarily a binary, `pavctl` exposes a library API for use by other tools or tests:

### `parse_runtime_from_bytes`
`pub fn parse_runtime_from_bytes(format: SerdeFormat, bytes: &[u8]) -> Result<binary::RuntimeConfig>`
Decodes a configuration object from a byte slice, applying schema validation.

### Formatting Functions
- `format_config`: Returns a string representation of the configuration tree.
- `format_header`: Returns a formatted string of the PVS header.
- `format_stats`: Returns a summary of artifact statistics.

## 5. Configuration and Runtime Behavior

### File Extensions
The CLI relies on file extensions to determine input/output formats:
- Input: `.yaml`, `.yml`, `.json` for text; `.pvs` for binaries.
- Output: `.pvs` for binaries; `.yaml`, `.json` for text.

### Subcommands
- `gen <input> [output]`: Compiles configuration.
- `view <input> [-x]`: Inspects a binary. `-x` enables hex dump.
- `check <input>`: Validates configuration.
- `convert <input> [output]`: Converts binary to text.

## 6. Error Handling and Invariants

### Validation
All operations involving configuration input (gen, check) strictly enforce the Pavis schema. Invalid configurations result in an immediate error and process termination.

### Extension Invariants
The tool refuses to process files with unknown extensions to avoid ambiguity. It requires explicit format declaration or standard extensions.

### Safe Parsing
The `parse` module ensures that only valid, schema-compliant configurations are successfully loaded. It relies on the codec layer to catch type mismatches or missing required fields.

## 7. Non-Goals and Explicit Limitations
- **Partial Updates**: `pavctl` operates on entire configuration files. It does not support patching or partial updates of existing binaries.
- **Runtime Control**: This crate does not contain logic to communicate with a running Pavis instance (no xDS or admin API client logic is currently implemented in this crate).
- **In-Memory Modification**: There is no interactive mode to edit configurations; it is a batch processing tool.