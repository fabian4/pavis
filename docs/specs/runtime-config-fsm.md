# Runtime Config Fetch/Reload State Machine (Normative)

## 1) Overview

This document defines the runtime-side configuration fetch/reload state machine for Pavis. It is strictly compatible with the Relay Distribution Protocol Specification (Normative) and is directly implementable. The runtime behaves as a single-threaded event-driven FSM that consumes only four semantic response classes (NewArtifact, NoUpdate, TransientUnavailable, NeedResync) plus timers and cancellation. The runtime MUST NOT issue concurrent fetch requests and MUST treat checksum/ETag as the sole identity.

## 2) State Model

### States (exact names)
1) **Idle**  
   - Fields: none  
   - Meaning: no in-flight fetch; ready to schedule a new fetch.

2) **Fetching**  
   - Fields:
     - `mode`: Conditional | Unconditional
     - `wait_ms`: u64  
   - Meaning: exactly one in-flight fetch exists.

3) **Verifying**  
   - Fields:
     - `artifact_bytes`
     - `etag`
     - `observed_version` (optional, informational only)  
   - Meaning: artifact is being verified.

4) **Applying**  
   - Fields:
     - `artifact_bytes`
     - `etag`
     - `observed_version` (optional, informational only)  
   - Meaning: artifact is being applied and persisted.

5) **BackoffSleeping**  
   - Fields: none  
   - Meaning: a backoff timer is active.

6) **Stopped**  
   - Fields: none  
   - Meaning: FSM terminated; no further actions.

### State Invariants
- Exactly one state is active at a time.
- Fetching is the only state with an in-flight network request.
- Verifying and Applying are mutually exclusive.
- BackoffSleeping MUST only be entered after TransientUnavailable.
- Observed version MUST be informational only and MUST NOT affect decisions.

## 3) Event Model

Events are the only inputs to the FSM. Allowed events:

- **Start**: FSM startup trigger.
- **Response(NewArtifact, payload)**: semantic class NewArtifact with artifact bytes + ETag + optional version.
- **Response(NoUpdate)**: semantic class NoUpdate.
- **Response(TransientUnavailable)**: semantic class TransientUnavailable.
- **Response(NeedResync)**: semantic class NeedResync.
- **VerifyOk**: verification succeeded.
- **VerifyFail**: verification failed.
- **ApplyOk**: apply succeeded.
- **ApplyFail**: apply failed.
- **TimerFired**: scheduled timer fired.
- **Shutdown**: cancellation/termination signal.

Events MUST correspond only to the four semantic response classes plus timer/cancellation primitives.

## 4) Context / Persistent Runtime Memory

The runtime MUST persist the following across ticks:

- `last_applied_etag` (optional string): ETag of last applied artifact.
- `last_rejected_etag` (optional string): ETag of last failed artifact.
- `last_rejected_until` (monotonic time): expiry for `last_rejected_etag`.
- `backoff_attempt` (u32): counts consecutive TransientUnavailable responses.
- `local_lkg_path` (path): persistent local LKG location.

Rejected-ETag TTL:
- `last_rejected_etag` MUST expire after **10 minutes**. After expiry, it MUST be cleared.

## 5) Effect Model (Side Effects)

The FSM may request only these effects:

- **FetchConditional(wait_ms, if_none_match_etag)**
- **FetchUnconditional(wait_ms)**
- **Verify(artifact_bytes, etag)**
- **Apply(artifact_bytes, etag)**
- **ScheduleTimer(duration)**
- **EmitMetrics/Logs(event_name, fields)** (optional but normative minimum defined in §12)

## 6) Transition Rules (Normative)

A complete transition table is defined below. “Effects” may include multiple items in order. Every event is handled in every state.

### Constants
- `WAIT_MS = 30000`
- `REJECT_TTL = 10 minutes`

### From Idle
- Start → Idle. Effects:
  - Immediately initiate FetchUnconditional(wait_ms=WAIT_MS).
- TimerFired → Fetching(Unconditional, wait_ms=WAIT_MS). Effects: FetchUnconditional(WAIT_MS).
- Shutdown → Stopped. Effects: none.
- Response/NewArtifact/NoUpdate/TransientUnavailable/NeedResync/VerifyOk/VerifyFail/ApplyOk/ApplyFail → Idle (no-op). Effects: none.

### From Fetching
- Response(NewArtifact) → Verifying unless rejected-ETag skip applies (see §6.1). Effects: Verify or none.
- Response(NoUpdate) → Fetching. Effects: FetchConditional or FetchUnconditional (immediate next long-poll, no backoff).
- Response(NeedResync) → Fetching. Effects:
  - Clear `last_applied_etag`, `last_rejected_etag`, `last_rejected_until`, `backoff_attempt`.
  - FetchUnconditional(wait_ms=WAIT_MS).
- Response(TransientUnavailable) → BackoffSleeping. Effects: ScheduleTimer(backoff_delay).
- Shutdown → Stopped. Effects: cancel in-flight request.
- VerifyOk/VerifyFail/ApplyOk/ApplyFail/TimerFired → Fetching (no-op). Effects: none.

### From Verifying
- VerifyOk → Applying if dedup passes; otherwise Fetching with immediate next long-poll. Effects:
  - If `etag == last_applied_etag`, skip Apply and immediately FetchConditional/FetchUnconditional.
  - Else Apply.
- VerifyFail → Fetching. Effects:
  - Set `last_rejected_etag = etag`, `last_rejected_until = now + REJECT_TTL`.
  - Immediately FetchConditional/FetchUnconditional.
- Shutdown → Stopped. Effects: none.
- Response/TimerFired/ApplyOk/ApplyFail → Verifying (no-op). Effects: none.

### From Applying
- ApplyOk → Fetching. Effects:
  - Set `last_applied_etag = etag`.
  - If `last_rejected_etag == etag`, clear it.
  - Immediately FetchConditional/FetchUnconditional.
- ApplyFail → Fetching. Effects:
  - Set `last_rejected_etag = etag`, `last_rejected_until = now + REJECT_TTL`.
  - Immediately FetchConditional/FetchUnconditional.
- Shutdown → Stopped. Effects: none.
- Response/VerifyOk/VerifyFail/TimerFired → Applying (no-op). Effects: none.

### From BackoffSleeping
- TimerFired → Fetching. Effects: FetchConditional/FetchUnconditional.
- Shutdown → Stopped. Effects: none.
- Response/VerifyOk/VerifyFail/ApplyOk/ApplyFail → BackoffSleeping (no-op). Effects: none.

### From Stopped
- Any event → Stopped. Effects: none.

#### 6.1 Rejected-ETag Skip Rule (Normative)
If a Response(NewArtifact) arrives with `etag == last_rejected_etag` and `now < last_rejected_until`, the runtime MUST skip Verify and Apply and return to normal polling immediately (no backoff).

## 7) Fetch Loop Requirements

- The runtime MUST NOT issue concurrent fetch requests to the Fetch Endpoint.
- Normal polling schedule is defined as a continuous long-poll cycle using `wait_ms = 30000`, issuing the next long-poll immediately after the previous request completes, without additional backoff.
- Conditional fetch MUST use `If-None-Match = last_applied_etag` if present; otherwise unconditional.
- After NoUpdate, the runtime MUST immediately start the next long-poll (normal polling schedule).
- The initial relay fetch MUST be unconditional with `wait_ms = 30000`.

## 8) Backoff Rules (Normative)

Backoff applies ONLY to TransientUnavailable.

### Parameters (fixed)
- Base delay: 250 ms  
- Factor: 2.0  
- Cap: 5,000 ms  
- Jitter: ±10% uniform

### Computation
`delay = min(cap, base * factor^backoff_attempt)`, then apply jitter.

### Reset Conditions
- On NewArtifact, NoUpdate, or NeedResync: reset `backoff_attempt = 0`.
- On TransientUnavailable: increment `backoff_attempt` by 1.

### Interaction with Scheduling
- NoUpdate MUST NOT trigger backoff.
- NeedResync MUST NOT be backoff-gated and MUST cause immediate unconditional fetch.

## 9) Deduplication and Verification Rules

Order is strict and mandatory:

1) Verify  
2) Dedup  
3) Apply  

Rules:
- Verification MUST include:
  - checksum verification against the ETag
  - artifact format validation
  - schema/version compatibility validation
- Runtime MUST NOT apply any artifact that fails verification.
- Runtime MUST treat checksum/ETag as the only identity and MUST NOT use version for identity or deduplication.
- Runtime MUST NOT re-apply an artifact with the same checksum.

## 10) Local Runtime LKG Semantics (Normative)

- On startup, the runtime MUST attempt to load a local LKG from `local_lkg_path` before emitting Start to the FSM.
- The local LKG MUST be verified using the same verification rules before use.
- If valid, the runtime MAY apply it immediately before the first fetch.
- The runtime MUST begin relay polling immediately after startup regardless of local LKG.
- The runtime MUST persist the newly applied artifact as local LKG as part of Apply.
- Local LKG usage MUST NOT delay or replace Relay fetching.

## 11) Error Mapping

The runtime MUST map HTTP and network outcomes into exactly four semantic classes:

- **NewArtifact**:
  - HTTP 200 with artifact body and ETag
- **NoUpdate**:
  - HTTP 204
  - HTTP 304
- **NeedResync**:
  - HTTP 410
- **TransientUnavailable**:
  - Any 5xx response
  - Network errors (timeouts, connection failures, DNS failures)
  - Any other non-200/204/304/410 response

The runtime MUST implement behavior exclusively in terms of these four semantic classes and MUST NOT infer semantics from raw HTTP status codes beyond this mapping.

## 12) Metrics and Observability (Normative Minimum)

Runtime MUST emit at least:

- Counter: `runtime_fetch_total{class=NewArtifact|NoUpdate|TransientUnavailable|NeedResync}`
- Counter: `runtime_verify_total{result=ok|fail}`
- Counter: `runtime_apply_total{result=ok|fail}`
- Gauge: `runtime_last_applied_version` (informational only)
- Gauge: `runtime_last_applied_etag_present` (0/1)
- Gauge: `runtime_backoff_attempt`

## 13) Concurrency and Cancellation

- The FSM MUST be single-threaded with a single event consumer.
- All side effects MUST be serialized by the FSM event loop.
- On Shutdown, any in-flight fetch MUST be canceled and the FSM MUST enter Stopped.

## 14) Security Considerations

- Artifact integrity MUST be verified prior to apply.
- ETag comparison MUST be a byte-for-byte string equality comparison.
- The runtime MUST reject any artifact whose checksum does not match the ETag.
- The runtime MUST treat any verification failure as non-fatal and continue serving the previous LKG.
