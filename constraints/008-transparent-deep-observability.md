# Constraint 008: Transparent & Deep Observability

Observability in the Microvisor is not an "add-on" but a foundational requirement. To maintain extreme performance, every component must be introspectable with near-zero overhead. Measurement must not significantly alter the behavior or latency of the system (Observer Effect).

## 1. Zero-Cost Telemetry Principle
*   **eBPF-First:** Use eBPF for kernel-space and hypervisor-boundary tracing to avoid context switches and data copying.
*   **Lockless Buffers:** Move telemetry data from hot paths to the control plane using ring buffers or per-CPU maps.
*   **No String Formatting:** Hot-path events must be emitted as binary structures; formatting and serialization (e.g., JSON) are delegated to the observability consumer.

## 2. Required Observability Domains

### 2.1 Virtualization Boundary
*   **VM-Exits:** Count and categorize all KVM exits (I/O, EPT violations, HLT, MSR access). High exit rates indicate suboptimal guest-host interaction.
*   **VCPU Jitter:** Measure the "Steal Time" and scheduling latency of vCPU threads to ensure pinning and isolation are effective.
*   **Interrupt Latency:** Trace the time from physical interrupt arrival to guest delivery.
*   **CFS Behavior:** Monitor `runnable_at` vs. `scheduled_at` for vCPU pthreads. Even with pinning, we must observe CFS throttle events and migration attempts.

### 2.2 Micro-Architectural (Hardware) Events
*   **Cache Hierarchy:** Monitor L1/L2/L3 cache misses and TLB misses via Performance Monitoring Units (PMUs).
*   **Branch Misprediction:** Track misprediction rates in the control plane to optimize the hot-path event loop.
*   **Memory Bandwidth:** Observe NUMA-local vs. remote memory access patterns to validate memory affinity.
*   **Transparent Huge Pages (THP):** Observe THP allocation success vs. fallback and the latency of `khugepaged` compaction cycles to prevent "stutter" during memory-intensive operations.

### 2.3 Guest & Lifecycle State
*   **State Transitions:** Atomic logging of lifecycle events (Created -> Configured -> Booting -> Running -> Terminating).
*   **Boot Latency:** Microsecond-precision timing of the path from `ioctl(KVM_RUN)` to the first guest instruction and subsequent user-space readiness.
*   **Entropy Availability:** Monitor guest entropy pools to prevent blocking on cryptographic operations.

### 2.4 Control Plane Internals
*   **Task Jitter:** Measure the latency of the `tokio` runtime's task scheduling.
*   **I/O Path Latency:** Track the time spent in `O_DIRECT` operations and Device Mapper Thin Provisioning requests.
*   **eBPF Hook Overhead:** Observe the execution time of eBPF programs attached to `tc` or `xdp` hooks to ensure networking remains stateless and fast.
*   **System Call Auditing:** Log and time all syscalls made by the control plane (e.g., `ioctl`, `write`, `mmap`) to detect unexpected blocking or latency in the orchestration path.

### 2.5 Resource Fencing Integrity
*   **Cgroup Pressure:** Monitor `cpu.stat` (throttling) and `memory.pressure` (PSI) to detect resource contention before it impacts latency.
*   **NUMA Misalignment:** Alert on any process memory allocation occurring outside the designated NUMA node.
*   **Network Path & NAT:** Trace every packet hop from the guest TAP to the host physical interface, including eBPF-driven NAT rewrites and connection tracking states.

## 3. Correlation & Traceability
*   **Unique Session Identifiers:** Every VM instance must be assigned a unique `session_id` (high-entropy 64-bit integer) at creation. This avoids the computational overhead of UUID generation while providing sufficient collision resistance for single-node multi-tenancy.
*   **Trace Propagation:** This ID must be propagated through all layers:
    *   **Control Plane:** Tagging all `tokio` tasks and log entries related to the session.
    *   **Kernel/eBPF:** Every eBPF-emitted event must include the `session_id` (retrieved from task local storage or map lookups).
    *   **Hardware Events:** PMU samples collected during the vCPU thread's execution slice must be correlated to the active `session_id`.
*   **Causal Linking:** Observability data must allow for a "single-pane-of-glass" reconstruction of a session's lifecycle, linking a high-level API request to its resulting KVM exits and hardware cache misses.

## 4. Enforcement
*   **CI/CD Performance Regressions:** Any PR that increases the baseline latency of a VM-exit or boot sequence by >1% must be justified. **Exceptions to this rule require a formal Architectural Review and a new ADR detailing the technical necessity and the mitigation strategy for the performance impact.**
*   **Always-On:** Telemetry primitives must be compiled in by default, relying on eBPF attachment/detachment for runtime toggling rather than conditional compilation where possible.
