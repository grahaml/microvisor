# Project One-Pager: Microvisor Control Plane

## The Problem
Running autonomous AI agents and headless browsers requires executing untrusted code. Traditional containerization shares the host machine's kernel, creating a massive attack surface. If an agent exploits a kernel vulnerability, it compromises the entire host. We need a sandboxed runtime that provides hardware-enforced isolation while keeping VM lifecycle latency low enough (p99 cold boot under 200 ms; snapshot resume under 10 ms) to feel like a container.

## The Solution: Microvisor
The **Microvisor** is a high-performance orchestration plane that treats a bare-metal Linux host as a programmable hardware multiplexer. It uses **Firecracker (KVM)** to provide each workload with its own dedicated kernel and memory space, but with the density and startup speed of a container.

### Core Architecture Pillars
*   **Rust Control Plane:** A statically compiled, non-blocking orchestrator (`tokio`) interacting directly with kernel `ioctl` boundaries.
*   **Hardware Isolation:** 1:1 mapping of guest vCPUs to physical host cores via cgroups v2 (`cpuset`), ensuring zero "noisy neighbor" interference.
*   **Zero-Shared-State Networking:** Elimination of host software bridges. Uses point-to-point TAP interfaces and **eBPF stateless NAT** for high-throughput, secure routing.
*   **Ephemeral Direct I/O:** Every VM boots from an immutable **Device Mapper Thin Provisioning** snapshot, bypassing the host page cache via `O_DIRECT`.

## The Goal: A Highly Observable "Mini-Cloud"
Instead of generic orchestrators like Kubernetes, the Microvisor is purpose-built for high-density, low-latency agent workloads. 

1.  **Deterministic Lifecycle:** Managed by an event-driven state machine that ensures atomic provisioning and cleanup.
2.  **Deep Observability:** Distributed tracing (OpenTelemetry) is integrated into the core. Every action is correlated by a 64-bit `session_id` linking user-space Rust logic to kernel-level KVM exits.
3.  **Dogfooding Implementation:** The Microvisor runs its own infrastructure (like the telemetry stack) inside managed system microVMs, keeping the host completely clean.

## Status
The architecture is codified in the **PRD** (`docs/001-prd.md`) and **TAD** (`docs/002-tad.md`), with the foundational decisions locked into ADRs 009–012 (multi-tenancy threat model, storage architecture, network model, crash-recovery contract). An SME review (`docs/003-expert-review.md`) hardened the spec set against technical errors and security gaps before implementation. Items deliberately deferred behind named re-evaluation triggers live in `docs/004-deferred-from-review.md`. First Rust orchestrator commit is the next step.
