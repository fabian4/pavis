# Pavis - A Lightweight Service Mesh Sidecar

[![Crates.io](https://img.shields.io/crates/v/pavis.svg)](https://crates.io/crates/pavis)
[![License](https://img.shields.io/crates/l/pavis.svg)](./LICENSE)
## 🎯 Project Goal
Build a lightweight, crash-safe, and memory-efficient service mesh sidecar to replace Envoy.
**Core Philosophy:** Decoupled Architecture ("Smart Bridge, Dumb Proxy").

## 🚀 Why this Split?
Standard sidecars (Envoy) do **too much**. They have to parse massive Protobuf configs, handle xDS streams, and manage complex internal state.
*   **Pavis xDS** takes the burden of complexity.
*   **Pavis** stays simple, fast, and dumb.

## 📊 Comparison

| Feature | Envoy | Pavis (Goal) |
| :--- | :--- | :--- |
| **Language** | C++ | Rust (Safe) |
| **Memory** | 100MB+ | ~20MB |
| **Config Load** | Heavy (Protobuf parsing overhead) | Instant (Zero-copy `Pavis Core` loading) |
| **Architecture** | Monolithic Sidecar | Decoupled (Controller + Proxy) |

## 🛠 Tech Stack
*   **Language:** Rust
*   **Proxy Engine:** [Cloudflare Pingora](https://github.com/cloudflare/pingora)
*   **Control Plane Communication:** `tonic` (gRPC)
*   **Serialization:** `rkyv` (Zero-Copy)

## 🏃 Quick Start

### Prerequisites
*   Rust 1.75+
*   `cmake` (for Pingora)
*   Docker & Docker Compose (for E2E tests)