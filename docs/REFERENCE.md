# Pavis Runtime Configuration Reference & Usage Manual

This manual provides a detailed reference for the Pavis data plane configuration. It is derived directly from the codebase implementation.

---

## Top-Level Structure

The configuration is typically provided in YAML or JSON format and maps to the `SerdeConfig` structure.

- **listeners** (list of object, optional): Definition of entry points for the proxy.
- **telemetry** (object, optional): Global observability and telemetry settings.
- **upstreams** (list of object, optional): Backend clusters and endpoint definitions.
- **routes** (list of object, optional): Virtual host and routing rules.

---

## 1. Listeners

Defines how Pavis listens for incoming connections.

### Field Tree: `listeners`

- **name** (string, required): Unique identifier for the listener.
- **address** (string, required): Socket address to bind to (e.g., `"0.0.0.0:8080"`).
- **workers** (integer, optional): Number of worker threads.
    - **Allowed values**: `1` to `65535`.
    - **Default**: `auto` (detected based on CPU cores).
    - **Validation**: Must be greater than 0.
- **tls** (object, optional): TLS configuration for the listener.
    - **cert_path** (string, required if `tls` is set): Path to the PEM-encoded certificate file.
    - **key_path** (string, required if `tls` is set): Path to the PEM-encoded private key file.
    - **client_auth** (object, optional): mTLS configuration.
        - **Allowed variants**:
            - `disabled`: (Default) No client authentication.
            - `optional`: Client certificate is requested but not required.
                - **ca_path** (string, required): Path to CA bundle for verifying client certs.
            - `required`: Valid client certificate is mandatory.
                - **ca_path** (string, required): Path to CA bundle for verifying client certs.

#### Validation & Backend Constraints
- Duplicate listener names are not allowed.
- **Backend**: Peer certificate extraction for Rustls mode is currently noted as a TODO in the codebase.

#### Example
```yaml
listeners:
  - name: "http-gateway"
    address: "0.0.0.0:8080"
    workers: 4
  - name: "https-gateway"
    address: "0.0.0.0:8443"
    tls:
      cert_path: "/etc/pavis/certs/server.crt"
      key_path: "/etc/pavis/certs/server.key"
      client_auth:
        required:
          ca_path: "/etc/pavis/certs/client-ca.crt"
```

---

## 2. Telemetry

Global observability settings.

### Field Tree: `telemetry`

- **level** (string, optional): Global logging level.
    - **Allowed values**: `error`, `warn`, `info`, `debug`, `trace`.
    - **Default**: `info`.
- **pingora** (string, optional): Logging level for the underlying Pingora framework.
    - **Allowed values**: `error`, `warn`, `info`, `debug`, `trace`.
    - **Default**: `info`.
- **service_name** (string, optional): Name of the service for logs and traces.
    - **Default**: `"pavis"`.
- **metrics** (string, optional): Socket address for the Prometheus metrics exporter (e.g., `"0.0.0.0:9090"`).
    - **Alias**: `prometheus_addr`.
    - **Default**: Disabled.
- **access_log** (string, optional): Access logging policy.
    - **Allowed values**: `disabled`, `stdout`, or a file path (e.g., `"/var/log/pavis/access.log"`).
    - **Default**: `stdout`.
- **tracing** (object, optional): Distributed tracing configuration.
    - **provider** (string, optional): Tracing provider.
        - **Allowed values**: `otlp`, `jaeger`, `zipkin`.
        - **Default**: `otlp`.
    - **sampling** (integer, optional): Sampling rate in percentage.
        - **Allowed values**: `0` to `100`.
        - **Default**: `100`.
    - **endpoint** (string, optional): Collector endpoint URL.
        - **Default**: `"http://localhost:4317"`.

#### Example
```yaml
telemetry:
  level: "debug"
  service_name: "pavis-prod"
  metrics: "0.0.0.0:9091"
  access_log: "/var/log/pavis/access.log"
  tracing:
    provider: "otlp"
    sampling: 50
    endpoint: "http://otel-collector:4317"
```

---

## 3. Upstreams

Defines backend clusters.

### Field Tree: `upstreams`

- **name** (string, required): Unique identifier for the upstream.
- **id** (integer, optional): Numeric ID for the upstream.
    - **Default**: Auto-assigned starting from 1.
- **discovery** (string, optional): Endpoint discovery mechanism.
    - **Alias**: `discovery_type`.
    - **Allowed values**:
        - `static`: (Default) Endpoints are fixed IP addresses.
        - `logical`: DNS-based discovery with connection-time resolution.
        - `strict`: DNS-based discovery with periodic background resolution.
            - **ttl** (integer, required for `strict`): DNS cache TTL in seconds.
- **balancer** (string, optional): Load balancing algorithm.
    - **Aliases**: `load_balancer`, `lb`.
    - **Allowed values**: `round-robin`, `random`, `least-request`.
    - **Default**: `random`.
- **protocol** (string, optional): Protocol for upstream connections.
    - **Aliases**: `http_version`, `http`.
    - **Allowed values**: `h1`, `h2`, `h2h1`.
    - **Default**: `h1`.
- **pool** (object, optional): Connection pool settings.
    - **Alias**: `connection_pool`.
    - **idle** (duration, optional): Idle timeout for pooled connections (e.g., `"60s"`, `"1m"`).
        - **Default**: `"60s"`.
    - **connect** (duration, optional): Connection establishment timeout.
        - **Default**: `"5s"`.
    - **max** (integer, optional): Maximum number of concurrent connections.
        - **Default**: `0` (unlimited).
- **tls** (object, optional): TLS configuration for backend connections.
    - **enabled** (boolean, optional): Enable TLS for upstream.
        - **Default**: `true` if the `tls` object is present.
    - **verify_cert** (boolean, optional): Verify backend certificate chain.
        - **Default**: `true`.
    - **verify_hostname** (boolean, optional): Verify backend certificate hostname.
        - **Default**: `true`.
    - **sni** (string, optional): Explicit SNI hostname to send.
    - **sni_mode** (string, optional): SNI selection logic.
        - **Allowed values**:
            - `auto`: (Default) Use endpoint hostname or request host.
            - `name`: Use explicit value in `sni` field.
            - `disabled`: Do not send SNI.
    - **ca_bundle_path** (string, optional): Path to CA bundle for verifying backend certs.
        - **Alias**: `ca_bundle`.
        - **Constraint**: **NOT CURRENTLY USED** by the Pingora Rustls connector.
    - **cert** (object, optional): Client certificate for mTLS to backend.
        - **cert_path** (string, required): Path to PEM certificate.
        - **key_path** (string, required): Path to PEM private key.
        - **chain_path** (string, optional): Path to additional certificate chain.
        - **chain_mode** (string, optional): How to handle the certificate chain.
            - **Allowed values**: `none`, `embedded`, `file`.
            - **Default**: `none`.
- **endpoints** (list of object, required): List of backend endpoints.
    - **address** (string, required): IP address or hostname.
        - **Aliases**: `addr`, `ip`.
    - **port** (integer, required): Destination port.
    - **weight** (integer, optional): Load balancing weight (1-65535).
        - **Default**: `1`.
- **circuit_breaker** (object, optional): Per-upstream circuit breaker.
    - **max_connections** (integer, required): Max in-flight upstream requests.
    - **max_pending_requests** (integer, required): Max queued requests waiting for capacity.
- **outlier_detection** (object, optional): Passive ejection on consecutive failures.
    - **consecutive_errors** (integer, required): Consecutive failures before ejection.
    - **eject_duration** (duration, required): How long to eject the endpoint.
- **health_check** (object, optional): Active health checks.
    - **path** (string, required): Probe path (must start with `/`).
    - **interval** (duration, required): Probe interval.
    - **timeout** (duration, optional): Probe timeout.
        - **Default**: Equal to `interval`.
    - **healthy_threshold** (integer, optional): Must be `1` (other values are rejected).
    - **unhealthy_threshold** (integer, optional): Must be `1` (other values are rejected).

#### Validation & Backend Constraints
- **Validation**: If `verify_cert` and `verify_hostname` are both true (`verify=full`), `sni_mode` cannot be `disabled`.
- **Validation**: `sni_mode: auto` with `verify=full` requires either DNS-based endpoints or a route-level host rewrite to provide a valid hostname for verification.
- **Backend**: `ca_bundle_path` is currently ignored due to Pingora Rustls connector limitations.

#### Example
```yaml
upstreams:
  - name: "api-backend"
    discovery: "logical"
    balancer: "round-robin"
    protocol: "h2"
    pool:
      idle: "30s"
      max: 100
    tls:
      sni_mode: "auto"
    endpoints:
      - address: "api.internal.local"
        port: 443
        weight: 10
```

---

## 4. Routes

Defines how requests are routed to upstreams.

### Field Tree: `routes`

- **host** (string, required): Virtual host domain (e.g., `"example.com"`, `"*"`).
- **paths** (list of object, required): Routing rules for this host.
    - **matcher** (object, optional): Path matching logic.
        - **Default**: Prefix match on `"/"`.
        - **Allowed variants**:
            - `!prefix`: Matches paths starting with the value.
                - **path** (string, required): Path prefix.
            - `!exact`: Matches the path exactly.
                - **path** (string, required): Exact path.
            - `!regex`: Matches the path against a regular expression.
                - **path** (string, required): Regex pattern.
    - **timeout** (duration, optional): Request timeout (e.g., `"30s"`).
        - **Default**: Disabled (no timeout).
    - **retry** (object, optional): Retry policy (enforced by the runtime).
        - **attempts** (integer, required): Number of retry attempts.
        - **retry_on** (list of string, required): Conditions to trigger retry.
            - **Allowed values**: `5xx`, `connect_failure`, `reset`, `refused`.
        - **per_try_timeout** (duration, required): Timeout for each individual attempt.
    - **request_headers** / **response_headers** (object, optional): Header manipulations.
        - **set_headers** (list of `[name, value]`, optional): Set or overwrite headers.
        - **append_headers** (list of `[name, value]`, optional): Append to existing header values.
        - **add_headers** (list of `[name, value]`, optional): Add headers (may create duplicates).
        - **remove_headers** (list of string, optional): Remove specified headers.
    - **rewrite** (object, optional): Path or Host rewrites.
        - **path** (string, optional): New path prefix to replace the matched prefix.
        - **host** (string, optional): New Host header value.
    - **principal** (object, optional): Authorization requirements.
        - **Allowed variants**:
            - `any`: (Default) No specific principal required.
            - `authenticated`: Requires a specific SPIFFE ID.
                - **spiffe** (string, required): SPIFFE ID.
            - `prefix`: Requires a SPIFFE ID with a specific prefix.
                - **prefix** (string, required): SPIFFE ID prefix.
    - **Action** (one of the following, required):
        - **forward** (object): Forward to an upstream.
            - **destinations** (list of object, required):
                - **upstream** (string, required): Name of the upstream.
                - **weight** (integer, required): Relative weight for this destination.
        - **redirect** (object): Respond with a redirect.
            - **status** (integer, required): HTTP status code (e.g., `301`, `302`).
            - **location** (string, required): Redirect target URL.
        - **direct** (object): Respond directly with a body.
            - **status** (integer, required): HTTP status code.
            - **body** (string, required): Response body content.

#### Validation & Backend Constraints
- **Validation**: Rewrite is **not supported** with `!regex` matchers and will cause a validation error.
- **Validation**: Forward actions must have at least one destination.
- **Validation**: Paths must be normalized (start with `/`, no trailing slashes unless it is `/`).
- **Validation**: Regex patterns are limited to 2048 characters.

#### Example
```yaml
routes:
  - host: "app.example.com"
    paths:
      - matcher: !prefix
          path: "/api/v1"
        rewrite:
          path: "/v1"
        forward:
          destinations:
            - upstream: "api-backend"
              weight: 100
      - matcher: !exact
          path: "/health"
        direct:
          status: 200
          body: "OK"
      - matcher: !prefix
          path: "/legacy"
        redirect:
          status: 301
          location: "https://new-app.example.com/legacy"
```

---

## Validation & Backend Constraints Summary

1. **Duration Fields**: Use humantime format (e.g., `"500ms"`, `"1s"`, `"2m"`, `"1h"`).
2. **Duplicate Detection**: Unique names are strictly enforced for listeners, upstreams, and virtual host domains.
3. **mTLS**: Upstream `ca_bundle_path` is currently ignored by the runtime's Rustls implementation.
4. **Header Names**: Must be valid HTTP header names (no spaces or control characters).
5. **Regex Limits**: Regular expressions are validated at configuration load time; complex or overly long regexes will be rejected.
6. **Normalization**: All path matchers (except regex) must be normalized.
