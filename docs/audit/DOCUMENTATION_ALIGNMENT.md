# Documentation Alignment Audit

_Date:_ 2026-01-28

## Summary
- Core specs (README, ARCHITECTURE, roadmap, features) are aligned with the frozen data plane thesis.
- Several operational and benchmarking guides still speak in production/adoption language, undermining the new positioning.
- No files actively contradict the thesis, but stale references and runbooks need rewriting or removal to avoid confusion.

## Documents to Delete
| File | Reason |
| --- | --- |
| `CLAUDE.md` | Contains only the text "AGENTS.md"; redundant with the true instructions file. |
| `GEMINI.md` | Same as above; duplicate pointer that drifts from the actual source of truth. |

## Documents to Rewrite
| File | Issue | Target State |
| --- | --- | --- |
| `AGENTS.md` | Workflow checklist references `docs/project/roadmap.md` and `docs/project/features.md`, which no longer exist. | Update links to `docs/roadmap/roadmap.md` and `docs/roadmap/features.md`; keep normative. |
| `CONTRIBUTING.md` | Points readers to `docs/FEATURES.md`, now obsolete. | Redirect to the canonical roadmap/features docs; normative. |
| `bench/README.md` | Uses an invalid relative path (`./docs/…`) and frames benchmarks as product readiness. | Trim to an index that points to `../docs/benchmarks/*.md`; informational. |
| `docs/operations/runtime.md` | Production runbook (systemd/Kubernetes) implying supported deployment models. | Recast as an operational evidence note focusing on environment checks and reload experiments; informational. |
| `docs/operations/relay.md` | Same production-ops framing, including health checks and backup plans. | Rewrite as a relay-behavior memo tied to long-poll/LKG semantics; informational. |
| `docs/operations/recovery.md` | Reads like an SRE guide (systemctl commands, resource tuning). | Convert into a failure-semantics narrative aligned with the thesis; informational. |
| `docs/benchmarks/methodology.md` | Claims “production-ready” goals and productization benchmarks. | Rewrite intro/conclusion to emphasize capability proof and semantic closure; normative methodology. |
| `docs/benchmarks/running.md` | Presents CI/workstation flows as deployment guidance. | Reframe as reproduction instructions for thesis benchmarks only; informational. |
| `docs/roadmap/plans/xds/implementation.md` | Implies active commitment to full xDS compatibility. | Annotate as design-only exploratory adapter; informational. |
| `docs/roadmap/plans/xds/test_plan.md` | Describes an “authoritative test strategy” without noting the optional status. | Mark as dormant blueprint gated behind optional adapter work; informational. |
| `docs/roadmap/coverage.md` | Static, undated coverage snapshot; likely stale and misleading. | Replace with regeneration instructions plus timestamp template; informational. |

## Documents to Keep As-Is
| File | Reason |
| --- | --- |
| `docs/specs/*.md` (runtime-config-fsm, relay-protocol, pvs-format) | Normative specs already aligned with frozen data plane guarantees. |
| `docs/design.md` | Captures the design constraints consistent with the thesis. |
| `docs/configuration/{reference,guide}.md` | Canonical schema/recipe references with no runtime inference claims. |
| `docs/audit/MAINTAINABILITY_SCAN.md`, `docs/audit/MAINTAINABILITY_PLAN.md` | Recent thesis-aligned audits. |
| `tests/README.md`, `tests/suites/DESIGN_*.md` | Test rationale documents that confirm the frozen data plane contract. |
| Crate README files (`crates/pavis*`, `pavis-relay`, `pavctl`, `pavis-core`, etc.) | Explain responsibilities/boundaries in engineering terms. |
| `docs/api/*.md`, `docs/operations/metrics.md` | Normative API/telemetry descriptions without product claims. |
| Policy/meta files (`CODE_OF_CONDUCT.md`, `SECURITY.md`, `LICENSE`) | Repository administration documents unaffected by the repositioning. |

## Optional Improvements
- After rewriting the operations guides, add cross-links from `docs/api/*` and `docs/design.md` to the new “operational evidence” docs.
- Add a short banner at the top of every `docs/roadmap/plans/*` file to mark whether it is a terminal gate or optional exploration.
- Provide a tiny README inside `docs/benchmarks/` that points to methodology + running docs once rewritten, improving navigation.
- Introduce a "Doc Manifest" (e.g., `docs/README.md`) that labels each document as normative vs. informational so contributors know where to add updates.
