# Configuration Alignment Plan (Frozen Purity)

This section records the alignment plan for enforcing the Frozen Data Plane model in the configuration pipeline.

### Short-term (Alignment & Safety)
1.  **Explicit Pipeline Boundary**: Enforce check → compile → materialize in `pavis-codec-api`. (Status: Completed)
2.  **Remove Semantic Defaults from Parsing**: Ensure `#[serde(default)]` does not inject business logic. (Status: Completed)
3.  **Isolate Structural Completion**: Separate shape normalization from semantic defaulting inside codec `compile`.

### Medium-term (Structural Clarity)
4.  **Constrain codec-api**: Ensure it enforces the boundary and core validation, with no semantic defaults.
5.  **Enforce RuntimeConfig Finality**: Runtime must reject configurations that haven't passed core validation.

### Long-term (Governor-readiness)
6.  **Harden Relay**: Ensure Relay treats `.pvs` blobs as opaque artifacts without inspection.
