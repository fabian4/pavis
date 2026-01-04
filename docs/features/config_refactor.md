# Config Refactor Plan (RuntimeConfig)

This refactor is complete and aligned with the canonical schema in `docs/reference/CONFIGURATION.md`.

## Status
- Done: `pavis-core` runtime types, validation, serde mappings, and dependent crates updated.
- Done: serde codec DTOs/conversions updated to match new names and YAML keys.
- Done: runtime/tools/tests updated to compile against the new schema.

## Final Schema Alignment (key points)
- Timeouts: `Timeout`, `ConnectTimeout`, `IdleTimeout`, `TryTimeout`, `Duration`.
- Telemetry: `Telemetry`, `Metrics`, `AccessLogPolicy`, `TracingPolicy`, `TracingProvider`.
- Upstreams: `LoadBalancer`, `HttpVersion`, `Pool { idle, connect, max }`, `ConnectionLimit`, `TlsPolicy`, `TlsVerify`, `SniName`.
- Routing: `PathMatch`, `RetryPolicy`, `RetryFlags`, `HeadersPolicy`, `Headers`, `Rewrite`, `Destination`.
- Names/types: `Hostname`, `Host`, `Path`, `ServiceName`, `UpstreamName`, `UpstreamId`, `ListenerName`, `Port`, `Weight`, `SampleRate`.

## Validation and Tooling
- `make fmt`, `make lint`, and `make build` validate the refactor.
