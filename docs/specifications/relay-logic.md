# Relay Distribution Protocol (Logic)

The Relay distributes **Frozen Artifacts** via HTTP Long-Polling. It does not inspect the artifact content, serving only as a distribution mechanism.

## 1. State Machine (Server)

The server uses `tokio::sync::Notify` to handle concurrent waiters without thread exhaustion.

1.  Parse `X-Pavis-Artifact-Version` from request.
2.  Compare with Relay's `current_version`.
3.  **If** `client_ver != current_ver`: Immediate response (200 OK + File).
4.  **If** `client_ver == current_ver`:
    *   Register interest in `Notify` handle.
    *   Await `Notify` OR `Timeout`.
    *   **On Notify:** Response (200 OK + File).
    *   **On Timeout:** Response (204 No Content).
