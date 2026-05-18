# AI Agent Instructions: Microvisor

You are an AI engineer assisting in the development of a high-density, ultra-low latency private cloud orchestration plane ("Microvisor"). This project uses Rust, Firecracker, and native Linux kernel primitives to treat the host as a programmable hardware multiplexer.

## 🎯 Project Vision
The goal is to deeply understand and implement a bare-metal orchestrator that provides hardware-enforced isolation with "Layer 1" performance. While currently prototyping with Hermes and Steel Browser, every architectural decision must move toward the "Microvisor" spec (`docs/0002-microvisor.md`).

## 🛡️ Core Mandates (Non-Negotiable)
1.  **Hardware Isolation:** 1:1 mapping of guest microVMs to host processes. 1:1 mapping of vCPUs to physical pthreads.
2.  **Resource Fencing:** Strict CPU and NUMA pinning using cgroups v2 (`cpuset`). No vCPU thread should ever migrate across physical cores.
3.  **Zero-Shared-State Networking:** No host bridges. Use Point-to-Point TAP routing with stateless eBPF NAT rewrites (`tc` hooks).
4.  **Ephemeral & Stateless:** All root filesystems are immutable/ephemeral. Mutate state only at the boundary via eBPF or CoW snapshots.
5.  **Direct I/O Path:** Prefer Device Mapper Thin Provisioning over loop devices. Use `O_DIRECT` + `O_NONBLOCK` to bypass the host page cache.

## 🏗️ Architectural Patterns
*   **Rust Control Plane:** The target orchestrator is a non-blocking Rust runtime (`tokio`) interacting directly with `/dev/kvm` and `ioctl` boundaries.
*   **Mission Packages:** Inject secrets and tasks via ephemeral `ext4` metadata drives (`/dev/vdb`) at boot.
*   **Multi-Distro RootFS:** We treat the Base OS as a pluggable component (Debian, Alpine, Wolfi) to benchmark latency and resource density.
*   **Memory Optimization:** Use NUMA-local pinning and either eager pre-faulting (`MADV_WILLNEED`) or `userfaultfd` for deterministic resume latencies.

## 📂 Project Structure
*   `scripts/hermes/`: AI Agent automation (Python guest).
*   `scripts/steel/`: Browser isolation infrastructure.
    *   `build/`: Distro-specific rootfs providers (Debian, Alpine, Wolfi).
    *   `bench/`: Latency and performance benchmarking suite.
*   `ADRs/`: Formal Architecture Decision Records.
*   `specs/`: Component-level technical specifications.
*   `constraints/`: Enforced security and resource fencing rules.

## 🛠️ Tech Stack (The "Microvisor" Toolkit)
*   **Hypervisor:** Firecracker (KVM)
*   **Control Plane:** Rust (Target), Bash (Prototype)
*   **Linux Primitives:** eBPF, cgroups v2, Device Mapper, NUMA, KVM ioctls.
*   **Guest OS:** Modular (Debian, Alpine, Wolfi).

## ⚠️ Workflow Rules
*   **Consult the Specs:** Always check `specs/`, `constraints/`, and `docs/0002-microvisor.md` before proposing changes.
*   **Performance First:** Avoid heavy CLI wrappers (e.g., `ip`, `dmsetup`) in final designs; prefer direct syscalls/ioctls where possible.
*   **Verify in Sandbox:** Every change to init scripts or rootfs requires a rebuild and test launch in the current prototype scripts.
*   **Captain's Log:** At the end of every session, create a short summary entry in `captains-log/YYMMDD-HHMM.md` capturing achievements, technical decisions, and pending tasks. Update the project history to ensure seamless handoffs.
