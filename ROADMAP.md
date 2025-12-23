# 🗺️ Asgard Project Roadmap

**Goal:** Build a high-performance, crash-safe service mesh data plane.
**Architecture:** Decoupled Control Plane (Raven) → Binary Protocol (Rune) → Polling Sidecar (Aegis).

---

## ✅ Phase 1: The Aegis Foundation (MVP)
**Status:** In Progress / Completed
**Goal:** A functional HTTP proxy handling traffic based on static local configuration.

- [x] **Core Engine (Aegis)**
  - [x] Initialize Cloudflare Pingora crate.
  - [x] Implement `ProxyHttp` trait.
  - [x] Basic CLI setup.
- [x] **Traffic Logic**
  - [x] Static Upstream Selection (IP:Port).
  - [x] Basic Load Balancing (Round Robin).
- [x] **Infrastructure**
  - [x] Dockerfile & Optimized Build.
  - [x] `docker-compose` environment.

---

## 🚧 Phase 2: The Rune Protocol (Zero-Copy & Safety)
**Status:** **Next Priority**
**Goal:** Define the binary interface and build the static compiler.

- [ ] **Protocol Definition (`crates/rune`)**
  - [ ] Define `ProxyConfig` root struct.
  - [ ] **[CRITICAL]** Define `RuneHeader` with **Magic Bytes** (`0x41 0x53 0x47 0x44` - "ASGD") and `Version` (u32).
  - [ ] Implement `rkyv` derivation for zero-copy deserialization.
  - [ ] Implement `check_bytes` validation (prevent segfaults on version mismatch).

- [ ] **Raven Compiler (Static CLI)**
  - [ ] Implement `raven compile -i config.yaml -o config.rune`.
  - [ ] Map YAML structs (Serde) → Rune Structs (Rkyv).
  - [ ] Validate logic (e.g., ensure Route references existing Cluster).

- [ ] **Aegis Integration (Local File)**
  - [ ] Replace YAML loader with `mmap` + `rkyv` loader.
  - [ ] Add startup check: Verify Magic Bytes & Version before loading.

---

## 🔄 Phase 3: The "Long Poll" Pipeline
**Status:** Planned
**Goal:** Establish the dynamic communication channel (Raven Server → Aegis Client).

- [ ] **Raven Server (`crates/raven`)**
  - [ ] Setup HTTP Server (Hyper/Axum).
  - [ ] Implement `GET /v1/config` endpoint.
  - [ ] **Long Polling Logic:**
    - [ ] Accept `X-Rune-Version` header from client.
    - [ ] If client is up-to-date, **hold connection** (sleep) for 60s.
    - [ ] If update occurs (or timeout), send binary response.

- [ ] **Aegis Client (`crates/aegis`)**
  - [ ] Implement Background Config Thread.
  - [ ] **Long Polling Loop:**
    - [ ] Request config with current version.
    - [ ] On 200 OK: Atomic Swap (`ArcSwap`) of config pointer.
    - [ ] On 304/Timeout: Loop again immediately.
  - [ ] **[CRITICAL]** Add Observability: `aegis_config_version` and `last_reload_timestamp` metrics.

---

## 🌉 Phase 4: The Bridge (xDS Integration)
**Status:** Planned
**Goal:** Connect Raven to the outside world (Istio).

- [ ] **xDS Client (Raven)**
  - [ ] Implement gRPC Client (Tonic) connecting to `istiod`.
  - [ ] Subscribe to `LDS` (Listeners) and `EDS` (Endpoints).
  - [ ] **Translation Engine:**
    - [ ] Convert xDS Protobuf → Rune Internal Structs.
    - [ ] Trigger the "Update Notification" to wake up long-polling Aegis clients.

- [ ] **Optimization**
  - [ ] Implement "Delta" logic: Only re-compile Rune if xDS actually changes relevant fields.

---

## 🛡️ Phase 5: Production Parity (The "Vital 20%")
**Status:** Planned
**Goal:** Add the resilience features required for a real Service Mesh.

- [ ] **Resilience**
  - [ ] Smart Retries (Retry on 503/Connect Failure).
  - [ ] Circuit Breaking (Max Pending Requests).
  - [ ] Timeouts.

- [ ] **Security**
  - [ ] mTLS implementation (using `rustls`).
  - [ ] Certificate rotation reloading.

- [ ] **Telemetry**
  - [ ] Distributed Tracing (OpenTelemetry Headers).
  - [ ] Prometheus Exporter (Request Rates / Latency buckets).