# Runtime Configuration Reference

> **Status:** Reference
> **Role:** The canonical definition of the "Fully Explicit" Runtime Configuration.

This document describes the `RuntimeConfig` structure consumed by the Pavis runtime. 
It represents the **fully materialized** state after the Codec layer has processed user input and applied all policy defaults.

## Configuration File Structure

The following YAML reference corresponds strictly to the `pavis_core::RuntimeConfig` struct.

```yaml
# ------------------------------------------------------------------------------
# PAVIS RUNTIME CONFIGURATION (Fully Explicit Reference)
# ------------------------------------------------------------------------------
# This configuration represents the "Fully Materialized" state consumed by the
# Pavis runtime. It is the result of the Codec layer processing user input and
# applying all policy defaults.
#
# NOTE: This format corresponds strictly to the `pavis_core::RuntimeConfig` struct.
# All policy decisions (timeouts, algorithms) are explicit here.
# ------------------------------------------------------------------------------

# ------------------------------------------------------------------------------
# LISTENERS
# Define entry points where Pavis accepts incoming connections.
# ------------------------------------------------------------------------------
listeners:
  - name: "public-https"                # [Required] Unique identifier for logs/metrics.
    listen_addr: "0.0.0.0:443"          # [Required] Bind address (IP:Port).
    
    # [Optional] Thread pool size for this listener.
    # If null, the Runtime uses a system heuristic (e.g., 1 thread per core).
    # Explicit values ensure deterministic resource usage across environments.
    worker_threads: 4

    # [Optional] TLS termination settings.
    # If null, the listener operates in cleartext (TCP/HTTP).
    # If present, TLS is enforced.
    tls:
      enabled: true                     # [Required] Master toggle for TLS on this listener.
      cert_path: "/etc/pavis/cert.pem"  # [Required if enabled] Path to server certificate.
      key_path: "/etc/pavis/key.pem"    # [Required if enabled] Path to private key.

# ------------------------------------------------------------------------------
# TELEMETRY
# Observability settings (Logs, Metrics, Tracing).
# ------------------------------------------------------------------------------
telemetry:
  # [Optional] Global logging verbosity.
  # Codec Default: "info"
  # Runtime Behavior: If null, defaults to "info". Explicit value preferred for determinism.
  level: "info"

  # [Optional] Pingora engine internal logging verbosity.
  # Often set deeper (e.g., "debug") for network troubleshooting, or same as level.
  pingora: "warn"

  # [Optional] Service identifier for distributed tracing and metrics tags.
  # Should match the deployment name.
  service_name: "pavis-gateway-prod"

  # [Optional] Address to expose Prometheus metrics.
  # If null, the metrics server is not started.
  prometheus_addr: "0.0.0.0:9090"

  # [Required] Access log destination.
  # Options: "Disabled", "Stdout", or { "File": "/path/to/log" }.
  # Codec Default: "Stdout".
  access_log: "Stdout"

  # [Optional] Distributed tracing configuration.
  # If null, tracing is disabled.
  tracing:
    enabled: true                       # [Required] Master toggle.
    provider: "otlp"                    # [Required] Tracing backend (currently only "otlp").
    sampling_rate: 0.1                  # [Required] 0.0 to 1.0 (10% sampling).

# ------------------------------------------------------------------------------
# UPSTREAMS
# Backend service definitions (Clusters).
# ------------------------------------------------------------------------------
upstreams:
  # UPSTREAM 1: HTTP/1.1 App Service
  - name: "app-service-v1"              # [Required] ID referenced by routes.
    
    # [Required] How endpoints are resolved.
    # "static"      -> Fixed IP list (config driven).
    # "strict-dns"  -> DNS A records, TTL respected.
    # "logical-dns" -> DNS resolved lazily at connection time.
    discovery_type: "static"

    # [Required] Load balancing algorithm.
    # "round-robin" -> Cyclic iteration.
    # "random"      -> Stateless random selection (Codec Default).
    load_balancer: "round-robin"

    # [Required] Upstream protocol.
    # "h1"   -> HTTP/1.1 (Codec Default).
    # "h2"   -> HTTP/2.
    # "auto" -> ALPN negotiation.
    http_version: "h1"

    # [Required] Connection pool sizing and timeouts.
    connection_pool:
      idle_timeout_secs: 60             # Keep-alive duration for idle connections.
      connection_timeout_secs: 5        # Max time to wait for TCP handshake.

    # [Optional] Upstream TLS settings (for backend encryption).
    # If null, connects via plaintext.
    tls: null

    # [Optional] Static list of endpoints (if discovery_type is static).
    endpoints:
      - address: { "ip": "10.0.1.10:8080" } # [Required] Target address.
        weight: 1                           # [Required] Load balancing weight (default 1).
      - address: { "ip": "10.0.1.11:8080" }
        weight: 1

  # UPSTREAM 2: HTTP/2 gRPC Service with TLS
  - name: "grpc-core"
    discovery_type: "logical-dns"
    load_balancer: "random"
    http_version: "h2"
    connection_pool:
      idle_timeout_secs: 120
      connection_timeout_secs: 2
    
    tls:
      enabled: true                     # [Required] Enable TLS to upstream.
      verify_hostname: true             # [Required] Verify cert Common Name matches host.
      verify_cert: true                 # [Required] Validate cert trust chain.
      sni: "grpc.internal.svc"          # [Optional] SNI header to send.

    endpoints:
      - address: { "dns": ["grpc.internal.svc", 9000] }
        weight: 5

# ------------------------------------------------------------------------------
# ROUTES
# Mapping incoming requests to Upstreams.
# ------------------------------------------------------------------------------
routes:
  # VIRTUAL HOST 1: Public API
  - host: "api.example.com"             # [Required] "Host" header match. "*" matches any.
    paths:
      
      # ROUTE 1: Exact match for login
      - match_type: "exact"             # [Required] "prefix", "exact", or "regex".
        path: "/v1/login"               # [Required] URI pattern.
        
        # [Optional] Request processing timeout.
        # If null, defaults to system/global limit.
        # Explicit values preferred for SLA enforcement.
        timeout_ms: 2000

        # [Optional] Retry policy for transient failures.
        retry_policy:
          attempts: 2                   # Total tries (1 initial + 2 retries).
          per_try_timeout_ms: 500       # Limit per individual attempt.
          retry_on: ["5xx", "connect-failure"] # Conditions triggering retry.

        destinations:
          - upstream: "app-service-v1"  # [Required] Target upstream name.
            weight: 1                   # [Required] Traffic split weight.

      # ROUTE 2: Prefix match for general API
      - match_type: "prefix"
        path: "/v1/"
        timeout_ms: 5000
        retry_policy: null              # No retries for general endpoints.
        
        # [Optional] Header manipulation.
        request_headers:
          actions:
            - key: "x-proxy-id"
              value: "pavis-gateway"
              action: "set"             # "set", "append", "add_if_absent", "remove"

        destinations:
          - upstream: "app-service-v1"
            weight: 1

  # VIRTUAL HOST 2: gRPC Subdomain
  - host: "grpc.example.com"
    paths:
      - match_type: "prefix"
        path: "/"
        timeout_ms: 10000
        
        # [Optional] Rewrite logic.
        rewrite:
          path_prefix_rewrite: "/"      # Strip prefix if needed.
          host_rewrite_literal: "grpc.internal.svc" # Change Host header upstream.

        destinations:
          - upstream: "grpc-core"
            weight: 1
```
