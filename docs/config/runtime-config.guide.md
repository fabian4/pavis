# Pavis Runtime Configuration Guide

This guide provides scenario-based instructions and recipes for configuring the Pavis data plane. It is based on the [Field-Tree Reference](./runtime-config.reference.md).

## Overview

Configuration follows a linear flow:
1. **YAML/JSON**: High-level intent defined by the user.
2. **Codec Layer**: Parses the source, applies defaults, and performs shape-completion.
3. **Validation**: Enforces semantic rules (e.g., path normalization, upstream referencing).
4. **Runtime Artifact**: An immutable, optimized binary representation used by the proxy.

---

## Quickstart

The smallest valid configuration that routes all traffic to a local backend:

```yaml
listeners:
  - name: "default"
    address: "0.0.0.0:8080"

upstreams:
  - name: "local-service"
    endpoints:
      - address: "127.0.0.1"
        port: 8081

routes:
  - host: "*"
    paths:
      - destinations:
          - upstream: "local-service"
            weight: 1
```

---

## Core Concepts

### Listeners
Listeners define the "front door" of the proxy. You can define multiple listeners (e.g., one for HTTP and one for HTTPS).
- [Reference: `listeners[]`](./runtime-config.reference.md#listeners)

### Routes & Matching
Pavis uses a "First Match Wins" strategy. 
- **Hosts**: Exact matching is tried first, then wildcard hosts (`*`).
- **Paths**: Defined per host. Matchers can be `!prefix`, `!exact`, or `!regex`.
- [Reference: `routes[]`](./runtime-config.reference.md#routes)

### Actions
Every path must have exactly one action. Actions are "flattened" at the route level:
- **Forwarding**: Just provide `destinations`.
- **Redirecting**: Provide `status` and `location`.
- **Direct Response**: Provide `status` and `body`.

### Rewrites
You can modify the request before it reaches the upstream:
- **Path Rewrite**: Replaces the matched prefix with a new one.
- **Host Rewrite**: Overwrites the `Host` header sent to the backend.
- [Reference: `routes[].paths[].rewrite`](./runtime-config.reference.md#routes)

### Upstreams
Upstreams are clusters of backend endpoints. They handle load balancing, protocol selection (H1/H2), and connection pooling.
- [Reference: `upstreams[]`](./runtime-config.reference.md#upstreams)

---

## TLS & Identity

### Inbound TLS Termination
Applied at the listener level. Requires `cert_path` and `key_path`.

### Inbound mTLS (Client Auth)
> **Backend Constraint**: Currently, while `client_auth` can be configured as `required` or `optional`, peer certificate extraction for Rustls mode is not fully wired in the runtime (marked as TODO).

### Outbound TLS (Origination)
Enable TLS for upstream connections by adding a `tls: {}` block to an upstream.
- **SNI**: Defaults to `auto`. If `verify=full`, you must use DNS discovery or a route host rewrite.
- [Reference: `upstreams[].tls`](./runtime-config.reference.md#upstreams)

### Outbound mTLS
Provide a `cert` block within the upstream `tls` configuration.

---

## Cookbook (Recipes)

### Basic HTTP Routing
Route traffic based on path prefixes.

**Config Snippet**:
```yaml
routes:
  - host: "api.example.com"
    paths:
      - matcher: !prefix
          path: "/v1"
        destinations:
          - upstream: "v1-backend"
            weight: 1
```
**Expected Behavior**: Requests to `api.example.com/v1/users` are sent to `v1-backend`.
**Common Errors**: `PathNotNormalized` if path doesn't start with `/`.

### Weighted Traffic Shift
Split traffic between two versions of a service.

**Config Snippet**:
```yaml
paths:
  - matcher: !prefix
      path: "/"
    destinations:
      - upstream: "v1"
        weight: 90
      - upstream: "v2"
        weight: 10
```
**Expected Behavior**: 90% of requests go to `v1`, 10% to `v2`.

### TLS Origination with SNI
Connect to a secure backend that requires a specific SNI.

**Config Snippet**:
```yaml
upstreams:
  - name: "secure-backend"
    tls:
      sni_mode: "name"
      sni: "backend.service.local"
    endpoints:
      - address: "10.0.0.5"
        port: 443
```
**Expected Behavior**: Pavis connects via TLS and sends `backend.service.local` in the SNI extension.

### Header Manipulation
Inject a tracking ID into requests and remove a sensitive header from responses.

**Config Snippet**:
```yaml
paths:
  - matcher: !prefix
      path: "/"
    request_headers:
      set_headers:
        - ["X-Pavis-Proxied", "true"]
    response_headers:
      remove_headers: ["Server"]
    destinations:
      - upstream: "backend"
        weight: 1
```

### Path Rewrite
Strip a version prefix before forwarding to a backend that doesn't expect it.

**Config Snippet**:
```yaml
paths:
  - matcher: !prefix
      path: "/service-a"
    rewrite:
      path: "/"
    destinations:
      - upstream: "backend-a"
        weight: 1
```
**Expected Behavior**: A request to `/service-a/health` is sent to the backend as `/health`.
