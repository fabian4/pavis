# Code-Level Architectural Invariants

This document defines enforceable, code-level guardrails for Pavis. Every invariant uses **MUST / MUST NOT** language and is intended for code review, crate design, and refactors.

## 1) Dependency Invariants

**Runtime**
- `pavis` (runtime) **MUST** depend only on `pavis-core` and `pavis-pvs`.
- `pavis` **MUST NOT** depend on ingest/codec/relay/governor crates or DTO types.
- `pavis` **MUST NOT** link to networking/IO stacks from ingest/relay.

**Codec**
- `pavis-codec-*` **MUST** depend only on `pavis-core` and DTO schemas.
- `pavis-codec-*` **MUST NOT** depend on I/O, networking, async runtimes, or env access.
- `pavis-codec-*` **MUST NOT** depend on relay or governor crates.

**Ingest**
- `pavis-ingest-*` **MUST** depend on transport libraries and DTO schemas. 
- `pavis-ingest-*` **MUST NOT** depend on relay or governor crates.
- `pavis-ingest-*` **MUST NOT** depend on RuntimeConfig construction or pavis-core canonical semantic validation.

**Relay**
- `pavis-relay` **MUST** depend on `pavis-core` and `pavis-pvs`.
- `pavis-relay` **MUST** treat RuntimeConfig as opaque, validated data and MUST NOT re-interpret or re-validate its semantics.
- `pavis-relay` **MUST NOT** depend on upstream protocol DTOs (xDS, CRD, YAML, JSON).
- `pavis-relay` **MUST NOT** include ingest/codec logic.

**Governor**
- `pavis-governor` **MUST** sit above relay and **MUST NOT** be on runtime hot paths .When present, pavis-governor MUST sit above relay
  and MUST NOT be on runtime hot paths.
- `pavis-governor` **MUST NOT** be linked into `pavis` or `pavis-pvs`.

**Enforcement**
- Cargo workspace layout **MUST** keep crates in separate dependency tiers.
- `cargo deny` (or equivalent) **MUST** block disallowed dependency edges.
- Feature gating **MUST** prevent optional protocol types from leaking into relay/runtime.
- Visibility rules (`pub(crate)`, private modules) **MUST** limit cross-crate access to internal constructors.

## 2) Type Boundary Invariants

- DTO types **MUST NOT** cross into runtime or relay public APIs.
- `RuntimeConfig` **MUST** be the only configuration type the runtime accepts.
- Relay APIs **MUST** accept only **Approved Plans** or **PVS Artifacts**, never DTOs.
- Codec APIs **MUST** be pure data transformations.

```rust
// ✅ codec boundary
fn dto_to_runtime(dto: XdsDto) -> RuntimeConfig;

// ✅ relay boundary
fn publish(artifact: PvsArtifact) -> Result<()>;
fn execute(plan: ApprovedPlan) -> Result<PvsArtifact>;

// ❌ forbidden
fn publish(dto: XdsDto);              // DTO crosses boundary
fn runtime_load(dto: XdsDto);         // runtime sees DTO
```

## 3) Construction & Ownership Invariants

**RuntimeConfig**
- `RuntimeConfig` **MUST** be constructed by codec pipelines only.
- Ingest **MUST NOT** construct `RuntimeConfig`.
- Relay **MUST** accept only validated `RuntimeConfig` or approved artifacts.

**PVS Artifacts**
- `pavis-pvs` **MUST** be the only crate that writes `.pvs` artifacts.
- Codecs **MUST NOT** serialize `.pvs` directly.
- Relay **MUST** call into `pavis-pvs` for artifact emission.

**Distribution State**
- Relay **MUST** be the single owner of distribution state (versions, checksums, caches).
- Other crates **MUST NOT** mutate relay’s distribution state.

**Patterns**
- Constructors for artifacts **MUST** be private or module-scoped.
- Builders for artifacts **MUST** live in `pavis-pvs`.
- Approved plans **MUST** be represented by a sealed type exposed only by governor.

## 4) Execution-Time Safety Invariants (Last Resort)

These are **safety nets**, not primary enforcement.

- Relay **MUST** reject multiple active sources at execution time.
- Relay **MUST** refuse unapproved plans.
- `pavis-pvs` **MUST** fail fast on magic/version/checksum mismatch.
- Runtime **MUST** abort on `.pvs` integrity failures.

## 5) Non-Goals (Anti-Invariants)

These constraints **MUST NOT** be enforced at runtime or in wrong layers:

- Runtime **MUST NOT** implement business policy or governance logic.
- Relay **MUST NOT** embed protocol-specific parsing or DTO handling.
- Codec **MUST NOT** make rollout or governance decisions.
- Ingest **MUST NOT** perform canonical semantic validation.
