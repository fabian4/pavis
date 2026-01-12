# Proxy Configuration & Fairness Comparison

**Purpose**: This document ensures all proxies in the benchmark (Pavis, Envoy, Nginx, HAProxy) are configured with equivalent semantics to enable fair performance comparison.

We strictly adhere to a **"Fairness Standard"** where proxies are unthrottled and given equal access to available resources (CPU/RAM/Connections) within the container limits.

---

## 1. Configuration Equivalence Table

The following table maps the semantic behaviors across all tested proxies.

| Semantic Behavior                  | Pavis                                      | Envoy                                      | Nginx                                      | HAProxy                                    |
|------------------------------------|--------------------------------------------|--------------------------------------------|--------------------------------------------|--------------------------------------------|
| **Workers/Threads**                | Runtime-detected (2 expected)              | `--concurrency 2`                          | `worker_processes 2`                       | `nbthread 2`                               |
| **Worker CPU Affinity**            | Runtime (OS scheduler)                     | Runtime (OS scheduler)                     | Runtime (OS scheduler)                     | `cpu-map 1 0`, `cpu-map 2 1`               |
| **Downstream Keepalive Enabled**   | ✅ Enabled (default)                       | ✅ Enabled (default)                       | ✅ `keepalive_timeout 65`                  | ✅ Enabled (HTTP mode default)             |
| **Downstream Keepalive Timeout**   | 30s (assumed default)                      | 3600s (route timeout, can be overridden)   | `keepalive_timeout 65`                     | `timeout client 30s`                       |
| **Downstream Keepalive Requests**  | Unlimited (assumed)                        | Unlimited (default)                        | `keepalive_requests 10000`                 | Unlimited (default)                        |
| **Upstream Keepalive Enabled**     | ✅ Connection pool (default)               | ✅ Connection pool (cluster)               | ✅ `keepalive 1000` (upstream)             | ✅ Enabled (default)                       |
| **Upstream Connection Pool Size**  | Runtime-managed                            | Cluster config (circuit breaker)           | `keepalive 1000` (persistent pool)         | No explicit limit                          |
| **HTTP Version (Downstream)**      | HTTP/1.1                                   | HTTP/1.1                                   | HTTP/1.1 (default)                         | HTTP/1.1 (HTTP mode)                       |
| **HTTP Version (Upstream)**        | HTTP/1.1                                   | HTTP/1.1                                   | `proxy_http_version 1.1`                   | HTTP/1.1 (HTTP mode)                       |
| **Connection Header (Upstream)**   | `Connection: keep-alive` (implicit)        | Managed by cluster                         | `proxy_set_header Connection ""`           | Managed by backend config                  |
| **Max Concurrent Connections**     | No explicit limit (OS-limited)             | No explicit limit                          | `worker_connections 65535` (per worker)    | `maxconn 20000` (global)                   |
| **Idle Timeout (Upstream)**        | 30s (assumed)                              | Connection pool idle timeout               | Persistent (with keepalive)                | `timeout server 30s`                       |
| **Connect Timeout (Upstream)**     | Default (5s assumed)                       | Default                                    | Default                                    | `timeout connect 5s`                       |
| **Logging**                        | ⛔ Disabled for benchmark                  | ⛔ `/dev/null`                             | ⛔ `access_log off; error_log /dev/null`   | ⛔ `no log`                                |
| **TCP Optimizations**              | OS defaults                                | OS defaults                                | `tcp_nopush on; tcp_nodelay on`            | OS defaults                                |
| **Event Model**                    | Async (Rust tokio)                         | Event-driven (C++ libevent)                | `use epoll; multi_accept on`               | Event-driven (C epoll)                     |
| **Worker Connections Limit**       | OS ulimit (`ulimit -n`)                    | OS ulimit                                  | `worker_connections 65535`                 | `maxconn 20000`                            |

---

## 2. Detailed Configuration Analysis

### Worker/Thread Count
**Equivalence**: All proxies are configured with **2 workers/threads** to match the baseline resource profile (2 CPUs).
- **Pavis**: Automatically detects available CPUs (container limit).
- **Envoy**: `--concurrency 2` flag.
- **Nginx**: `worker_processes 2`.
- **HAProxy**: `nbthread 2`.

### Keepalive Configuration
**Downstream (Client → Proxy)**:
- All proxies support persistent connections.
- Timeouts vary slightly (30s - 3600s) but are sufficient for the 30s benchmark duration.

**Upstream (Proxy → Backend)**:
- **Pavis**: Runtime-managed connection pool.
- **Envoy**: Cluster circuit breaker config.
- **Nginx**: `upstream { keepalive 1000; }` (Increased from 100 to prevent bottlenecks).
- **HAProxy**: Unlimited server connections.

### Logging Overhead
**Requirement**: All proxies must disable access logging to eliminate Disk I/O overhead.
- **Pavis**: Logging disabled by default in benchmark mode.
- **Envoy**: `/dev/null`.
- **Nginx**: `access_log off`.
- **HAProxy**: `no log`.

### Nginx-Specific Optimizations
To ensure Nginx is not unfairly penalized:
- **Connections**: `worker_connections` increased to `65535`.
- **TCP**: `tcp_nopush on` and `tcp_nodelay on` are enabled (standard best practice).
- **Event Model**: `use epoll` and `multi_accept on` are enabled.

---

## 3. Validation Checklist

Before running benchmarks, verify the following:

- [ ] All proxies use 2 workers/threads.
- [ ] Downstream & Upstream keepalive is enabled.
- [ ] HTTP/1.1 is used for all connections.
- [ ] Logging is disabled.
- [ ] CPU pinning is active (`cpuset_cpus` in docker-compose).
- [ ] **Host `ulimit -n` is ≥ 65535** (Crucial for high concurrency tests).
- [ ] CPU governor is set to `performance`.

---

## 4. Reporting Fairness Violations

If you identify a configuration mismatch that affects fairness (e.g., one proxy has an unfair advantage or handicap):

1. **Document the discrepancy**: Which setting is different?
2. **Assess impact**: Does it materially affect RPS or Latency?
3. **Open an Issue**: https://github.com/fabian4/pavis/issues
