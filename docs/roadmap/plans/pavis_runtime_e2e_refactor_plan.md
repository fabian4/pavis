Overview
Refactor and extend the Pavis Runtime (Data Plane) E2E suite to increase signal density, reduce redundancy, improve concurrency and negative-path coverage, and standardize evidence capture, while keeping tests deterministic and implementation-feasible.

Scope Clarification (Critical)
- All new or merged cases MUST live in the same directory as their predecessor cases (e.g. the directory that currently contains 20_reload_contract_core, 40_traffic_routing_semantics, etc.).
- Naming MUST follow existing conventions in the repository:
  - If current cases are NN_name.sh, new cases MUST also be NN_name.sh.
  - If cases are registry-driven (not filename-driven), update only the registry; filenames may remain but must not be executed.
- Do not invent new directory layouts.

Goals
- Merge/restructure cases to reduce duplication and strengthen causality in single-case flows.
- Add high-value concurrency and failure-path cases with deterministic assertions.
- Standardize evidence capture across fetch / validate / apply / serve phases.
- Keep CI summaries compact, deterministic, and debuggable.

Non-Goals
- No changes to runtime behavior or protocol semantics.
- No probabilistic pass/fail logic (100/0 only).
- No performance benchmarking beyond coarse reload leak sentinels.
- No refactors unrelated to test determinism or observability.

Evidence Model (Two-Tier, Explicit)
All cases MUST emit evidence in four phases. Evidence sources are tiered to avoid blocking on runtime changes.

Tier 1 (MUST, no runtime changes):
- Fetch:
  - Relay-served artifact version (etag/version header or relay response metadata).
- Apply:
  - Admin stats version field OR equivalent observable version indicator.
- Serve:
  - HTTP response status.
  - Response body or header containing version marker and/or upstream instance_id.
- LKG proof:
  - Admin stats version unchanged AND served traffic remains on previous version.

Tier 2 (SHOULD, minimal observability additions allowed):
- Validate:
  - Stable log tag OR metric counter indicating validation result with reason class:
    - parse
    - version
    - semantic
- Apply transition:
  - Stable log tag “applied version X”.

Rule:
- Tier 1 evidence is REQUIRED for gating.
- Tier 2 evidence is OPTIONAL initially; if missing, tests MUST fall back to Tier 1 proofs.
- Any Tier 2 addition MUST be isolated in its own PR and limited strictly to observability (logs/metrics only).

Proposed Case Restructure
Final case list with mapping (numbers indicative; follow existing numbering scheme):

- reload_contract_core (new):
  Merge of 20_reload_norestart + 21_reload_zero_option_impact (implemented as 20_reload_contract_core).

- traffic_routing_semantics (existing 40):
  Single start; internal Phase A / Phase B assertions.

- lkg (existing 30):
  Extended with semantic validation rejection branch.

- traffic_weighted (existing 41):
  Clarify intent only.

- obs_consistency (new):
  Merge of 70_obs_metrics + 80_obs_cross_consistency (implemented as 70_obs_consistency).

- obs_access_log (existing 71):
  Stabilize log waiting logic.

- operational_admin_api (existing 90):
  Relax uptime assertions.

- reload_storm (new 22).

- reload_keepalive_atomic (new 23).

- atomic_mid_request (new 24).

- lkg_relay_unavailable (new 32).

- semantic_validation_matrix (new 33).

- operational_reload_resource_sanity (new 92, WARN/optional).

Case Merge Plan
- reload_contract_core:
  - Create new case file alongside 20/21.
  - Remove 20/21 from suite registry first; keep files temporarily.
  - Delete 20/21 files only after new case is green.
  - Update only the authoritative case index (suite README or registry file).

- obs_consistency:
  - Create new case file alongside 70/80.
  - Remove 70/80 from registry, then delete files.
  - Update authoritative index only.

- traffic_routing_semantics:
  - Keep filename.
  - Split assertions internally:
    Phase A: match precedence, regex fallback.
    Phase B: headers, actions, rewrites.
  - No restart between phases.

Case Adjustments (Deterministic)
- reload_contract_core:
  Assertions:
  - Zero failed requests during burst.
  - Atomicity:
    - Use barrier: wait until apply of V2 confirmed (admin stats or equivalent),
      THEN assert that all subsequent sampled requests are V2 only.
  - Removal:
    - V1 has X-Pavis-Version header.
    - After apply barrier, header MUST be absent.
  - SUT id unchanged; process alive.

- lkg:
  Add semantic rejection branch:
  - Publish config with route referencing non-existent upstream.
  Assertions:
  - Apply does NOT occur (admin stats version unchanged).
  - Served traffic remains on LKG.
  - If Tier 2 signal exists, surface reject reason = semantic.
  - Reject reason is SHOULD, not MUST.

- traffic_weighted:
  Update comments/output:
  - Explicitly state this test proves elimination of state carry-over after reload (100/0 flips).
  - Not a probabilistic weight correctness test.

- obs_access_log:
  - Replace sleep with bounded exponential backoff.
  - Cap total wait time.
  - On failure, print:
    - Tail last N lines of log.
    - Current admin stats version.
    - SUT id.

- operational_admin_api:
  - Uptime assertion: uptime_seconds(t2) > uptime_seconds(t1) only.

New Cases (Adjusted for Determinism)

reload_storm:
- Scenario:
  - Sustained traffic at fixed concurrency.
  - Publish V1 → V10 rapidly.
  - Each version has explicit version marker (header or upstream instance).
- Assertions:
  - Zero request failures.
  - Phase-based monotonicity:
    - After apply(Vn) barrier, next K requests MUST all be Vn.
  - No rollback after barrier.
- Determinism:
  - Explicit apply barrier per version.
  - Fixed concurrency, fixed cadence.

reload_keepalive_atomic:
- Scenario:
  - Single keep-alive client.
  - Sequential requests across reload.
- Assertions:
  - Connection not dropped.
  - After apply barrier, no old version observed.
  - Each request internally consistent.
- Determinism:
  - Single client, serialized requests.

atomic_mid_request:
- Scenario:
  - One slow upstream request in flight.
  - Reload triggered mid-response.
- Assertions:
  - In-flight request completes.
  - Response reflects exactly one version.
- Determinism:
  - Fixed delay upstream, single request.

lkg_relay_unavailable:
- Scenario:
  - Break relay (timeout/unreachable).
  - Restore relay later.
- Assertions:
  - Traffic continues serving LKG.
  - No crash-loop.
  - After restore, new version eventually applies.
- Evidence:
  - Fetch failures optional (SHOULD).
- Determinism:
  - Explicit outage window and restore point.

semantic_validation_matrix:
- Scenario:
  - Sequential publishes of invalid configs:
    - Missing upstream reference.
    - Duplicate listener port.
    - Missing policy reference.
    - Illegal rewrite target.
    - Missing RBAC policy.
- Assertions:
  - Each publish rejected.
  - LKG continues serving.
  - Reject reason surfaced if Tier 2 signal exists.
- Determinism:
  - Fixed configs, fixed order.

operational_reload_resource_sanity (WARN/optional):
- Scenario:
  - N reloads with no traffic mutation.
- Assertions:
  - Resource indicators do not grow unboundedly.
  - Prefer coarse checks:
    - Admin stats counts.
    - Connection counts.
- Gating:
  - WARN only unless stable threshold proven.
- Determinism:
  - Fixed N, fixed cadence.

Execution Strategy
- All steps are small, mergeable PRs.
- Never mix runtime behavior changes with test logic changes.
- Observability additions (if any) are isolated and optional.
- Preserve single-start patterns where possible.

Risks and Mitigations
- Missing reject reason signals:
  - Mitigation: fall back to Tier 1 LKG proofs.
- Flaky timing:
  - Mitigation: apply barriers + bounded waits.
- Runtime increase:
  - Mitigation: cap reload counts; mark heavy cases optional.

Deliverables
- Updated case set with merged and new cases.
- Deterministic assertions with explicit barriers.
- Standardized evidence output (fetch/apply/serve/LKG).
- Updated authoritative suite index.

Status
- Completed: Step 1, Step 2, Step 3, Step 4, Step 5, Step 6, Step 7, Step 8, Step 9, Step 10, Step 11, Step 12, Step 13, Step 14, Step 15, Step 16

Step-by-Step Plan
1. Define evidence helpers (apply barrier, version sampling) reusable by cases.
   Exit: helpers used by at least one existing case.

2. Implement reload_contract_core; remove 20/21 from registry.
   Exit: new case green; old cases not executed.

3. Delete legacy 20/21 files; update authoritative index.
   Exit: no references remain.

4. Refactor 40_traffic_routing_semantics into Phase A/B.
   Exit: single start, grouped output.

5. Implement obs_consistency; remove 70/80 from registry.
   Exit: metrics/log/trace consistency proven.

6. Relax uptime assertion in operational_admin_api.
   Exit: case green with new rule.

7. Extend lkg with semantic rejection branch (Tier 1 proofs).
   Exit: reject + LKG continuity proven.

8. Clarify traffic_weighted intent (comments/output only).
   Exit: no behavior change.

9. Stabilize obs_access_log with bounded backoff and diagnostics.
   Exit: bounded waits, clear failure output.

10. Add reload_storm with apply barriers.
    Exit: zero failures, monotonic per-phase.

11. Add reload_keepalive_atomic.
    Exit: connection preserved, atomicity proven.

12. Add atomic_mid_request.
    Exit: in-flight atomicity proven.

13. Add lkg_relay_unavailable.
    Exit: LKG continuity and recovery proven.

14. Add semantic_validation_matrix.
    Exit: all invalid configs rejected deterministically.

15. Add operational_reload_resource_sanity (WARN lane).
    Exit: diagnostics emitted, no CI flake.

16. Optional: add Tier 2 observability signals if missing (separate PR).
    Exit: reject/apply reasons surfaced without changing semantics.
