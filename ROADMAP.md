# 🗺️ Pavilion Project Roadmap

**Goal:** Build a high-performance, crash-safe service mesh data plane.
**Architecture:** Decoupled Control Plane (Pavis xDS) → Binary Protocol (Pavis Core) → Polling Sidecar (Pavis).

---

## ✅ Phase 1: The Pavis Foundation (MVP)
**Status:** In Progress / Completed
**Goal:** A functional HTTP proxy handling traffic based on static local configuration.

- [x] **Core Engine (Pavis)**
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

## 🚧 Phase 2: The Pavis Core Protocol (Zero-Copy & Safety)
**Status:** **Next Priority**
**Goal:** Define the binary interface and build the static compiler.

- [ ] **Protocol Definition (`crates/pavis-core`)**
  - [ ] Define `ProxyConfig` root struct.
  - [ ] **[CRITICAL]** Define `PavisHeader` with **Magic Bytes** (`0x50 0x41 0x56 0x53` - "PAVS") and `Version` (u32).
  - [ ] Implement `rkyv` derivation for zero-copy deserialization.
  - [ ] Implement `check_bytes` validation (prevent segfaults on version mismatch).

- [ ] **Pavis xDS Compiler (Static CLI)**
  - [ ] Implement `pavis-xds compile -i config.yaml -o config.pavis`.
  - [ ] Map YAML structs (Serde) → Pavis Core Structs (Rkyv).
  - [ ] Validate logic (e.g., ensure Route references existing Cluster).

- [ ] **Pavis Integration (Local File)**
  - [ ] Replace YAML loader with `mmap` + `rkyv` loader.
  - [ ] Add startup check: Verify Magic Bytes & Version before loading.

---

## 🔄 Phase 3: The "Long Poll" Pipeline
**Status:** Planned
**Goal:** Establish the dynamic communication channel (Pavis xDS Server → Pavis Client).

- [ ] **Pavis xDS Server (`crates/pavis-xds`)**
  - [ ] Setup HTTP Server (Hyper/Axum).
  - [ ] Implement `GET /v1/config` endpoint.
  - [ ] **Long Polling Logic:**
    - [ ] Accept `X-Pavis-Version` header from client.
    - [ ] If client is up-to-date, **hold connection** (sleep) for 60s.
    - [ ] If update occurs (or timeout), send binary response.

- [ ] **Pavis Client (`crates/pavis`)**
  - [ ] Implement Background Config Thread.
  - [ ] **Long Polling Loop:**
    - [ ] Request config with current version.
    - [ ] On 200 OK: Atomic Swap (`ArcSwap`) of config pointer.
    - [ ] On 304/Timeout: Loop again immediately.
  - [ ] **[CRITICAL]** Add Observability: `pavis_config_version` and `last_reload_timestamp` metrics.

---

## 🌉 Phase 4: The Bridge (xDS Integration)
**Status:** Planned
**Goal:** Connect Pavis xDS to the outside world (Istio).

- [ ] **xDS Client (Pavis xDS)**
  - [ ] Implement gRPC Client (Tonic) connecting to `istiod`.
  - [ ] Subscribe to `LDS` (Listeners) and `EDS` (Endpoints).
  - [ ] **Translation Engine:**
    - [ ] Convert xDS Protobuf → Pavis Core Internal Structs.
    - [ ] Trigger the "Update Notification" to wake up long-polling Pavis clients.

- [ ] **Optimization**
  - [ ] Implement "Delta" logic: Only re-compile Pavis Core if xDS actually changes relevant fields.

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
