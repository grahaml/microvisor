# Microvisor: CTO Cheat Sheet

This document distills the core architecture, features, tradeoffs, and deep-dive technical details of the Microvisor control plane to facilitate high-level technical discussions.

## 1. Core Vision & Overall Architecture
**Objective:** A high-density, ultra-low latency bare-metal orchestration control plane. It treats standard Linux hosts as programmable hardware multiplexers to run untrusted AI agents and headless browsers securely.
- **Implementation:** A single, statically compiled Rust binary (built on `tokio`) with zero external daemon dependencies (no Docker, no JVM).
- **Virtualization:** Uses Firecracker (KVM) to run microVMs, achieving hardware-enforced isolation with container-like speed (p99 cold boot < 200ms).
- **Strict 1:1 Mapping:** One guest VM = exactly one host process. One guest vCPU = exactly one physical host thread.

## 2. Core Features & "Hardware-Enforced" Isolation
- **Compute Fencing (cgroups v2):** Strict vCPU and NUMA pinning to specific physical cores. Guarantees deterministic execution and prevents "noisy neighbor" cache thrashing.
- **Zero-Shared-State Networking:** Drops host software bridges completely. Uses dedicated point-to-point TAP interfaces per VM with stateless **eBPF NAT rewrites** (`tc` hooks) for near bare-metal throughput and complete tenant isolation.
- **Ephemeral Direct Storage (Device Mapper):** VMs boot from immutable snapshots via Device Mapper Thin Provisioning (CoW). Bypasses the host page cache via `O_DIRECT` and leverages `io_uring` for asynchronous I/O submission. State is injected at boot via ephemeral `ext4` metadata drives.
- **Transparent Observability:** Deep monitoring via KVM tracepoints and eBPF maps, entirely without agents inside the guest OS. High-level `session_id` correlates the Rust async tasks directly with low-level kernel events.

## 3. Overarching Flow: VM Provisioning (The State Machine)
The VM lifecycle is strictly managed by an event-driven `VmStateMachine` to ensure safe, atomic operations and clean recovery:
1. **`Pending`:** Configuration validated against system capacity.
2. **`ProvisioningStorage`:** Device Mapper issues an `ioctl` to create an instantaneous CoW snapshot. An ephemeral `ext4` metadata drive is generated to inject secrets/tasks.
3. **`ConfiguringNetwork`:** Dedicated TAP device is created, and eBPF NAT programs are loaded to handle IP translation statelessly.
4. **`InitializingCgroup`:** cgroups v2 boundaries are established (`memory.max`, `memory.swap.max=0`, `cpuset.cpus`). NUMA pinning is applied.
5. **`LaunchingVMM`:** The Firecracker hypervisor process is executed (via `jailer` for an outer security boundary).
6. **`Running`:** VM is active. The `session_id` to `cgroup_id` mapping is written to a shared eBPF map for hardware-level telemetry correlation.
7. **`Terminating` → `Destroyed`:** Graceful or emergency teardown of all resources (snapshots, IPs, cgroups).

## 4. Nitty-Gritty Details: Memory Management
Memory management in Microvisor is heavily optimized for predictability and hardware-level isolation.
- **No Swap Allowed:** `memory.swap.max=0` is permanently enforced for VM cgroups to guarantee deterministic latency.
- **NUMA Locality Pinning:**
  - Guests are strictly pinned to the NUMA node local to their assigned CPUs to avoid a 30-50ns interconnect latency penalty.
  - The system dynamically reads `/sys/devices/system/node/online` and gracefully handles single-NUMA nodes as a first-class citizen (where pinning is skipped).
- **Initialization Strategies (Resume Path):**
  - **Eager Pre-faulting (Current Default):** Uses `MADV_WILLNEED` (or `mmap` with `MAP_POPULATE | MAP_HUGETLB`) to force the host kernel to eager-load memory before vCPU activation. Trades ~10ms startup latency for zero page-fault runtime jitter.
  - **Lazy Loading (Future Iteration):** Uses `userfaultfd` for on-demand asynchronous page fetching over UDS. Optimizes for sub-millisecond boot times at the cost of execution jitter.
- **Hard OOM Protection:** Absolute limits set via `memory.max`. A memory leak inside a guest VM triggers a clean, isolated OOM kill of that specific microVM process without degrading host stability.

## 5. Key Tradeoffs & Technical Realities
- **Security vs. Capacity (SMT Mitigation):** To protect against cross-VM side-channel attacks (like L1TF/Spectre-v2), the host must either disable SMT (Simultaneous Multithreading) entirely—sacrificing 50% of theoretical compute capacity—or the orchestrator must strictly co-locate SMT siblings of a physical core within the same VM.
- **Control Plane Blocking Hazards:** System calls like `ioctl` (for storage) or writing to `/sys/fs/cgroup/` can starve the async `tokio` reactor thread. We mitigate this by aggressively wrapping known-blocking calls in `tokio::task::spawn_blocking` and monitoring runtime poll metrics.
- **Lack of Portability:** Microvisor treats the Linux kernel as an API. Heavy reliance on cgroups v2, specific eBPF features (`tc` egress hooks, `bpf_l3_csum_replace`), and KVM tracepoints means the control plane is tightly coupled to modern Linux kernel versions and cannot be easily ported to macOS/Windows or legacy Linux distributions.
- **Storage Pool Exhaustion Risk:** Device mapper thin pools will silently hang I/O for VMs when full. This requires robust admission control and strict quota tracking before allowing a VM into the `Pending` state.

## 6. Transparent Observability Stack
The Microvisor is built with "always-on" deep observability that operates entirely outside the guest boundary, treating telemetry as a first-class architectural primitive.
- **Agentless Monitoring:** Monitors CPU, memory, context switches, and I/O from the host without installing any agents inside the guest OS. 
- **End-to-End Correlation (`session_id`):** Every API request generates a 64-bit `session_id`. This propagates through high-level Rust async tasks (via OpenTelemetry `tracing` spans) all the way down to the Linux kernel.
- **Hardware & eBPF Tracing:** An eBPF map (`vm_session_map`) links the guest's `cgroup_id` directly to its `session_id`. This allows the system to monitor KVM tracepoints (`tracepoint:kvm:kvm_exit`) and hardware Performance Monitoring Units (PMUs) to track VM behavior natively.
- **Control Plane Auditing:** Automatically audits and alerts on slow, blocking kernel APIs (like storage `ioctl`s or cgroup filesystem writes) that exceed 1ms. This guarantees the `tokio` orchestration event loop is never starved.
- **"Dogfooding" via Tier-0 System VMs:** Instead of running observability infrastructure (like an OpenTelemetry Collector) as a host daemon, the orchestrator automatically provisions dedicated "System microVMs" to run telemetry pipelines. This maintains absolute host node purity and validates the virtualization stack before user workloads run.
