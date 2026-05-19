# AI Agent Instructions: Microvisor

You are an AI engineer assisting in the development of a high-density, ultra-low latency private cloud orchestration plane ("Microvisor"). This project uses Rust, Firecracker, and native Linux kernel primitives to treat the host as a programmable hardware multiplexer.

## 🎯 Project Vision
The goal is to deeply understand and implement a bare-metal orchestrator that provides hardware-enforced isolation with "Layer 1" performance. The bash prototype phase is complete; the project is now moving to the Rust control plane. Authoritative context lives in `docs/001-prd.md` (product intent), `docs/002-tad.md` (technical architecture), `docs/004-expert-review.md` (SME review with severity-tiered findings), and `docs/005-deferred-from-review.md` (explicitly-deferred work).

## 🛡️ Core Mandates (Non-Negotiable)
1.  **Hardware Isolation:** 1:1 mapping of guest microVMs to host processes. 1:1 mapping of vCPUs to physical pthreads.
2.  **Resource Fencing:** Strict CPU and NUMA pinning using cgroups v2 (`cpuset`). No vCPU thread should ever migrate across physical cores.
3.  **Zero-Shared-State Networking:** No host bridges. Use Point-to-Point TAP routing with stateless eBPF NAT rewrites (`tc` hooks).
4.  **Ephemeral & Stateless:** All root filesystems are immutable/ephemeral. Mutate state only at the boundary via eBPF or CoW snapshots.
5.  **Direct I/O Path:** Prefer Device Mapper Thin Provisioning over loop devices. Open guest block devices with `O_DIRECT` (bypasses host page cache) and submit via `io_uring` (async, low-syscall-overhead I/O).

## 🏗️ Architectural Patterns
*   **Rust Control Plane:** The target orchestrator is a non-blocking Rust runtime (`tokio`) interacting directly with `/dev/kvm` and `ioctl` boundaries.
*   **Mission Packages:** Inject secrets and tasks via ephemeral `ext4` metadata drives (`/dev/vdb`) at boot.
*   **Multi-Distro RootFS:** We treat the Base OS as a pluggable component (Debian, Alpine, Wolfi) to benchmark latency and resource density.
*   **Memory Optimization:** Use NUMA-local pinning and either eager pre-faulting (`MADV_WILLNEED`) or `userfaultfd` for deterministic resume latencies.

## 📂 Project Structure
*   `control-plane/specs/`: Rust control-plane technical specifications. **This is the authoritative spec set for current and future work.**
*   `constraints/`: Enforced security, isolation, and resource-fencing rules.
*   `ADRs/`: Formal Architecture Decision Records.
*   `docs/`: PRD, TAD, expert review, deferred-work list.
*   `scripts/hermes/`, `scripts/steel/`: Bash-era prototype automation (Hermes Python agent, Steel browser). Retained for reference; new work goes in the Rust control plane.
*   `archive/specs/`: Bash-era component specs. Archived 2026-05-19 when the project moved off bash. Do not extend; see `control-plane/specs/` for the live spec set.

## 🛠️ Tech Stack (The "Microvisor" Toolkit)
*   **Hypervisor:** Firecracker (KVM), launched via the `jailer` binary.
*   **Control Plane:** Rust on `tokio` (target). Bash prototype is retired.
*   **Linux Primitives:** eBPF (`libbpf-rs` or `aya`), cgroups v2, Device Mapper Thin Provisioning, NUMA, KVM ioctls, `io_uring`.
*   **Guest OS:** Modular (Debian, Alpine, Wolfi).

## ⚠️ Workflow Rules
*   **Consult the Specs:** Always check `control-plane/specs/`, `constraints/`, `ADRs/`, and `docs/002-tad.md` before proposing changes. `archive/specs/` is read-only history.
*   **Performance First:** Avoid heavy CLI wrappers (e.g., `ip`, `dmsetup`) in final designs; prefer direct syscalls/ioctls where possible.
*   **Captain's Log:** At the end of every session, create a short summary entry in `captains-log/YYMMDD-HHMM.md` capturing achievements, technical decisions, and pending tasks. Update the project history to ensure seamless handoffs.
