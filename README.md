# Microvisor

> **AI Agents:** Please read [AGENTS.md](./AGENTS.md) and [GEMINI.md](./GEMINI.md) for core security mandates and architectural patterns before making changes.

Microvisor is a high-density, ultra-low latency private cloud orchestration plane. It treats standard Linux hosts as programmable hardware multiplexers, providing hardware-enforced isolation with "Layer 1" performance for ephemeral workloads.

## 🎯 Project Vision
The goal is to deliver a bare-metal orchestrator that leverages Firecracker, KVM, and native Linux kernel primitives to achieve deterministic performance and strict multi-tenant isolation.

### Target Latencies
- **Cold Boot:** < 200 ms
- **Snapshot Resume:** < 10 ms
- **vCPU KVM_RUN Re-entry:** < 5 µs
- **eBPF NAT Overhead:** < 200 ns

## 🏗️ Core Architecture
- **Hypervisor:** Firecracker (KVM) launched via the `jailer`.
- **Control Plane:** Asynchronous Rust daemon built on `tokio`, interacting directly with `/dev/kvm` and `ioctl` boundaries.
- **Compute Isolation:** 1:1 mapping of guest vCPUs to physical physical cores using cgroups v2 (`cpuset`).
- **Networking:** Zero-shared-state routing via Point-to-Point TAPs and stateless eBPF NAT rewrites (`tc` hooks). No host bridges.
- **Storage:** Device Mapper Thin Provisioning (`dm-thin`) for instant CoW snapshots, opened with `O_DIRECT` and submitted via `io_uring`.

## 📂 Project Structure
- `control-plane/`: The Rust orchestrator implementation.
    - `specs/`: Rust-specific technical specifications (Authoritative).
    - `src/`: Core logic for VMM management, storage, networking, and observability.
- `docs/`: Strategic project documentation.
    - `001-prd.md`: Product Requirements & Operator Stories.
    - `002-tad.md`: Technical Architecture & Kernel Primitives.
    - `003-expert-review.md`: SME architectural audit findings.
- `constraints/`: Formal security and resource-fencing rules.
- `ADRs/`: Architecture Decision Records capturing key design trade-offs.
- `captains-log/`: Session-by-session history of technical decisions and progress.
- `scripts/`: Retired bash prototypes (`hermes`, `steel`) retained for reference.
- `diagrams/`: Mermaid-based architecture and state machine visualizations.
- `tests/`: End-to-end and integration tests (e.g., browser verification).

## 🛠️ Tech Stack
- **Language:** Rust
- **Runtime:** `tokio`
- **Virtualization:** Firecracker, KVM
- **Kernel Primitives:** eBPF (`libbpf-rs`), cgroups v2, Device Mapper, `io_uring`, NUMA.
- **Guest OS:** Modular (Debian, Alpine, Wolfi).

## 📖 Getting Started
1. Review the [Product Requirements (PRD)](./docs/001-prd.md) for the "Why".
2. Read the [Technical Architecture (TAD)](./docs/002-tad.md) for the "How".
3. Consult the [Control Plane Specs](./control-plane/specs/) for implementation details.
4. Check [AGENTS.md](./AGENTS.md) if you are an AI assistant.
