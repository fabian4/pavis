# Relay Protocol Specification

> **Status:** Implementation Specification
> **Role:** Defines the distribution protocol mechanism between Relay and Runtime.

## 1. Relay Distribution Protocol (The State Machine)

The Relay ensures configuration propagation via HTTP Long-Polling.

**API Reference:** See [docs/reference/API_RELAY.md](../reference/API_RELAY.md) for the HTTP contract.

### 1.1 Server Side State Machine
The server uses `tokio::sync::Notify` to handle concurrent waiters without thread exhaustion.

**Logic:**
1.  Parse `X-Pavis-Artifact-Version` from request.
2.  Compare with Relay's `current_version`.
3.  **If** `client_ver != current_ver`: Immediate response (200 OK + File).
4.  **If** `client_ver == current_ver`:
    *   Register interest in `Notify` handle.
    *   Await `Notify` OR `Timeout`.
    *   **On Notify:** Response (200 OK + File).
    *   **On Timeout:** Response (204 No Content).

```rust
async fn handle_poll(req: Request, state: State) -> Response {
    let client_ver = req.header("X-Pavis-Artifact-Version").parse::<u64>();
    let current_ver = state.artifact_version().await;

    if client_ver != current_ver {
        return send_file(current_ver);
    }

    // [PERF] Park the task using Tokio waker. 0 CPU usage.
    let notified = state.notifier().notified();
    match timeout(Duration::from_millis(wait_ms), notified).await {
        Ok(_) => send_file(state.artifact_version().await),
        Err(_) => Response::builder().status(204).body(Empty) // 204 No Content
    }
}
```