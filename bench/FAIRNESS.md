# Proxy Configuration Fairness Checklist

**Purpose**: This document ensures all proxies in the benchmark are configured with equivalent semantics to enable fair performance comparison.

---

## Configuration Equivalence Table

| Semantic Behavior                  | Pavis                                      | Envoy                                      | Nginx                                      | HAProxy                                    |
|------------------------------------|--------------------------------------------|--------------------------------------------|--------------------------------------------|--------------------------------------------|
| **Workers/Threads**                | Runtime-detected (2 expected)              | `--concurrency 2`                          | `worker_processes 2`                       | `nbthread 2`                               |
| **Worker CPU Affinity**            | Runtime (OS scheduler)                     | Runtime (OS scheduler)                     | Runtime (OS scheduler)                     | `cpu-map 1 0`, `cpu-map 2 1`               |
| **Downstream Keepalive Enabled**   | ✅ Enabled (default)                       | ✅ Enabled (default)                       | ✅ `keepalive_timeout 65`                  | ✅ Enabled (HTTP mode default)             |
| **Downstream Keepalive Timeout**   | 30s (assumed default)                      | 3600s (route timeout, can be overridden)   | `keepalive_timeout 65`                     | `timeout client 30s`                       |
| **Downstream Keepalive Requests**  | Unlimited (assumed)                        | Unlimited (default)                        | `keepalive_requests 10000`                 | Unlimited (default)                        |
| **Upstream Keepalive Enabled**     | ✅ Connection pool (default)               | ✅ Connection pool (cluster)               | ✅ `keepalive 100` (upstream)              | ✅ Enabled (default)                       |
| **Upstream Connection Pool Size**  | Runtime-managed                            | Cluster config (circuit breaker)           | `keepalive 100` (persistent pool)          | No explicit limit                          |
| **HTTP Version (Downstream)**      | HTTP/1.1                                   | HTTP/1.1                                   | HTTP/1.1 (default)                         | HTTP/1.1 (HTTP mode)                       |
| **HTTP Version (Upstream)**        | HTTP/1.1                                   | HTTP/1.1                                   | `proxy_http_version 1.1`                   | HTTP/1.1 (HTTP mode)                       |
| **Connection Header (Upstream)**   | `Connection: keep-alive` (implicit)        | Managed by cluster                         | `proxy_set_header Connection ""`           | Managed by backend config                  |
| **Max Concurrent Connections**     | No explicit limit (OS-limited)             | No explicit limit                          | `worker_connections 10000` (per worker)    | `maxconn 20000` (global)                   |
| **Idle Timeout (Upstream)**        | 30s (assumed)                              | Connection pool idle timeout               | Persistent (with keepalive)                | `timeout server 30s`                       |
| **Connect Timeout (Upstream)**     | Default (5s assumed)                       | Default                                    | Default                                    | `timeout connect 5s`                       |
| **Logging**                        | ⛔ Disabled for benchmark                  | ⛔ `/dev/null`                             | ⛔ `access_log off; error_log /dev/null`   | ⛔ `no log`                                |
| **TCP Optimizations**              | OS defaults                                | OS defaults                                | `tcp_nopush on; tcp_nodelay on`            | OS defaults                                |
| **File Operations**                | N/A (reverse proxy)                        | N/A (reverse proxy)                        | `sendfile on`                              | N/A (reverse proxy)                        |
| **Event Model**                    | Async (Rust tokio)                         | Event-driven (C++ libevent)                | `use epoll; multi_accept on`               | Event-driven (C epoll)                     |
| **Worker Connections Limit**       | OS ulimit (`ulimit -n`)                    | OS ulimit                                  | `worker_connections 10000`                 | `maxconn 20000`                            |
| **Request Buffer Size**            | Default                                    | Default                                    | Default                                    | `tune.bufsize 16384`                       |
| **Max Accept Batch**               | Default                                    | Default                                    | `multi_accept on`                          | `tune.maxaccept 100`                       |

---

## Detailed Configuration Analysis

### 1. Worker/Thread Count

**Equivalence**: All proxies configured with 2 workers/threads to match baseline resource profile (2 CPUs).

**Implementations**:
- **Pavis**: Automatically detects available CPUs (expects 2 from container limit)
- **Envoy**: `--concurrency 2` command-line flag
- **Nginx**: `worker_processes 2` in nginx.conf
- **HAProxy**: `nbthread 2` in global section

**Verification**:
```bash
# Check Pavis worker count (inspect logs)
docker logs bench-pavis 2>&1 | grep -i "worker"

# Check Envoy worker count
docker exec bench-envoy ps aux | grep envoy

# Check Nginx worker count
docker exec bench-nginx ps aux | grep "nginx: worker"

# Check HAProxy thread count
docker exec bench-haproxy haproxy -vv | grep -i thread
```

---

### 2. Keepalive Configuration

**Downstream (Client → Proxy)**:

| Proxy   | Enabled | Timeout | Max Requests | Config                           |
|---------|---------|---------|--------------|----------------------------------|
| Pavis   | ✅      | 30s     | Unlimited    | Default                          |
| Envoy   | ✅      | 3600s   | Unlimited    | Default (route timeout override) |
| Nginx   | ✅      | 65s     | 10000        | `keepalive_timeout 65; keepalive_requests 10000` |
| HAProxy | ✅      | 30s     | Unlimited    | `timeout client 30s`             |

**Upstream (Proxy → Backend)**:

| Proxy   | Enabled | Pool Size | Config                              |
|---------|---------|-----------|-------------------------------------|
| Pavis   | ✅      | Runtime   | Default connection pool             |
| Envoy   | ✅      | Cluster   | Cluster config (circuit breaker)    |
| Nginx   | ✅      | 100       | `upstream { keepalive 100; }`       |
| HAProxy | ✅      | Unlimited | `server backend1 backend:80 check`  |

**Fairness Assessment**: Minor timeout differences exist (30s vs 65s vs 3600s), but all support persistent connections. For 30s benchmark durations, these differences are negligible.

---

### 3. HTTP Version Alignment

**Requirement**: All proxies must use HTTP/1.1 for both downstream and upstream connections.

**Implementations**:
- **Pavis**: HTTP/1.1 default
- **Envoy**: HTTP/1.1 default
- **Nginx**: `proxy_http_version 1.1; proxy_set_header Connection "";`
- **HAProxy**: HTTP/1.1 default in HTTP mode

**Verification**:
```bash
# Capture upstream request from backend
docker exec bench-backend tcpdump -A -s 0 'tcp port 80' | grep "HTTP/1"
```

---

### 4. TCP & Event Optimizations

**Nginx-Specific Optimizations**:
- `sendfile on`: Zero-copy file transmission (not applicable for proxying)
- `tcp_nopush on`: Batch small packets (Nagle's algorithm)
- `tcp_nodelay on`: Disable Nagle for low latency
- `multi_accept on`: Accept multiple connections per event
- `use epoll`: Linux epoll event model

**Fairness Assessment**:
- Nginx has explicit TCP tuning; other proxies rely on OS defaults
- These optimizations are standard best practices and do not constitute unfair advantage
- All proxies can use epoll on Linux (event model abstracted by runtime)

**Mitigation**: None required. TCP_NODELAY and similar flags are performance best practices.

---

### 5. Logging Overhead

**Requirement**: All proxies must disable access logging to eliminate I/O overhead.

**Implementations**:
- **Pavis**: Logging disabled by default in benchmark mode
- **Envoy**: `/dev/null` logging
- **Nginx**: `access_log off; error_log /dev/null;`
- **HAProxy**: `no log`

**Verification**:
```bash
# Check for log files in containers
docker exec bench-pavis ls /var/log 2>/dev/null
docker exec bench-envoy ls /var/log 2>/dev/null
docker exec bench-nginx ls /var/log 2>/dev/null
docker exec bench-haproxy ls /var/log 2>/dev/null
```

---

## Known Differences (Non-Critical)

### Acceptable Differences

1. **Worker CPU Affinity**: HAProxy explicitly pins workers to CPUs (`cpu-map`); others rely on OS scheduler + cpuset
   - **Impact**: Minimal with cgroup cpuset pinning in docker-compose.yaml
   - **Mitigation**: All containers use `cpuset_cpus` for CPU isolation

2. **Upstream Connection Pooling**: Nginx has fixed pool size (100); others dynamically manage
   - **Impact**: Negligible for workloads with 100-500 connections
   - **Mitigation**: Benchmark connections ≤ 100 for most tests (except concurrency)

3. **Buffer Sizes**: HAProxy explicitly sets `tune.bufsize 16384`; others use defaults
   - **Impact**: Minimal for small payloads (httpbin/minimal responses < 1KB)
   - **Mitigation**: None required for current workloads

---

## Validation Checklist

Before running benchmarks, verify:

- [ ] All proxies use 2 workers/threads
- [ ] Downstream keepalive enabled for all proxies
- [ ] Upstream keepalive enabled for all proxies
- [ ] HTTP/1.1 used for all connections
- [ ] Logging disabled for all proxies
- [ ] CPU pinning configured in docker-compose.yaml
- [ ] Backend pinned to CPU 0
- [ ] Proxies pinned to CPUs 1-2 (or CPU 1 for cpu-limited)
- [ ] ulimit -n ≥ 10000 on host
- [ ] CPU governor set to `performance` (recommended)

---

## Configuration Files Reference

Full proxy configurations available in:
- `bench/config/pavis.yaml`
- `bench/config/envoy.yaml`
- `bench/config/nginx.conf`
- `bench/config/haproxy.cfg`

---

## Reporting Fairness Violations

If you identify a configuration mismatch that affects fairness:

1. Document the discrepancy (semantic behavior + proxies affected)
2. Assess performance impact (estimate % difference)
3. Propose mitigation (config change or benchmark limitation)
4. Open an issue: https://github.com/fabian4/pavis/issues

---

**End of Fairness Checklist**
