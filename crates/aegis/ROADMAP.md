# Aegis Roadmap 🛡️

**Role:** The Data Plane / Sidecar Proxy.
**Engine:** Cloudflare Pingora.
**Philosophy:** "Smart Bridge, Dumb Proxy" - No xDS parsing, only executable logic.

---

## 🚧 Phase 1: The Foundation (MVP)
**Goal:** A functional HTTP proxy handling traffic based on static configuration.

- [ ] **Core Setup**
    - [x] Initialize Rust crate with Pingora dependencies.
    - [x] Basic CLI setup (`clap`) to accept configuration paths.
    - [x] Setup `tracing` for structured logging.

- [ ] **Proxy Implementation (Pingora Traits)**
    - [x] Implement `ProxyHttp` trait.
    - [x] Implement `upstream_peer()`: Select upstream host based on static config.
    - [x] Implement `upstream_request_filter()`: Basic header forwarding.

- [ ] **Configuration (Static)**
    - [x] Define a temporary `yaml` or `json` config loader for development (before full Rune integration).
    - [ ] Support defining:
        - Listener port.
        - Upstream cluster (IP:Port).

- [ ] **Deployment**
    - [ ] Create a `Dockerfile` for Aegis.
    - [ ] `docker-compose` setup to test Aegis sitting in front of a dummy backend (e.g., `echo-server`).

---

## 🔗 Phase 2: Rune Integration (The Protocol)
**Goal:** Switch from static text config to dynamic `Rune` binary config.

- [ ] **Rune Loading**
    - [ ] Integrate `crate/rune`.
    - [ ] Implement `rkyv` zero-copy loading of `.rune` files.
    - [ ] Replace temporary config structs with `rune::Route` and `rune::Cluster`.

- [ ] **Hot Reloading**
    - [ ] Implement a file watcher or signal handler (`SIGHUP`).
    - [ ] Swap `Arc<RuneConfig>` atomically without dropping active connections.

---

## 🛡️ Phase 3: Resilience & Advanced Traffic
**Goal:** Reach feature parity with basic Envoy sidecar functionality.

- [ ] **Traffic Management**
    - [ ] Weighted Round-Robin Load Balancing.
    - [ ] Request Retries (configurable via Rune).
    - [ ] Timeouts (Connect/Read/Write).

- [ ] **Resilience**
    - [ ] Passive Health Checking (Outlier Detection).
    - [ ] Circuit Breaking (Fail fast when upstream is overloaded).

- [ ] **Security**
    - [ ] TLS Termination (Downstream).
    - [ ] Upstream TLS (mTLS with backend services).

---

## 🔭 Phase 4: Observability & Production Readiness
**Goal:** Metrics, tracing, and operational excellence.

- [ ] **Metrics**
    - [ ] Expose Prometheus endpoint (`/metrics`).
    - [ ] Track: Request rate, P99 latency, Upstream errors, Memory usage.

- [ ] **Tracing**
    - [ ] OpenTelemetry integration.
    - [ ] Propagate `traceparent` headers.

- [ ] **Optimization**
    - [ ] Profile memory usage (Target: <50MB).
    - [ ] Benchmark RPS vs. Envoy.
