# ADR 004: eBPF-Centric Kernel & Boundary Tracing

## Status
Accepted

## Context
Following **ADR 003 (Observability-Driven Architecture)**, we require a mechanism to trace events at the hypervisor boundary (KVM) and within the host kernel (e.g., networking, I/O) with near-zero overhead. Traditional tools like `strace`, `perf` (in sampling mode without BPF), or reading `/proc` files introduce significant context-switch overhead or lack the granularity to correlate events with specific Microvisor session IDs.

We need a deterministic, lockless way to export metrics such as VM-exits, interrupt latencies, and networking hook execution times.

## Decision
We will standardize on **eBPF (Extended Berkeley Packet Filter)** as the exclusive mechanism for kernel-level observability.

1. **Instrumentation:** eBPF programs will be attached to tracepoints (e.g., `tracepoint:kvm:kvm_exit`), kprobes, and networking hooks (`tc`, `xdp`).
2. **Data Export:** All telemetry data from the kernel hot path will be written as binary structs to an eBPF **Ring Buffer** (`BPF_MAP_TYPE_RINGBUF`).
3. **Session Correlation:** The Rust control plane will inject the `session_id` (64-bit integer) into a BPF Hash Map, keyed by the cgroup ID or the vCPU thread PID.
4. **Expanded Visibility:**
    *   **Networking:** eBPF programs on `tc` hooks will trace packet headers and NAT transformations, emitting flow logs with the `session_id`.
    *   **Scheduling:** Trace `sched_wakeup` and `sched_switch` to calculate CFS scheduling latency (`runnable_at` vs `scheduled_at`) for vCPU threads.
    *   **Memory:** Trace `khugepaged` events to monitor THP allocation and compaction latency.
5. **Out-of-Band Processing:** A dedicated, low-priority Rust thread will poll the ring buffer, deserialize the binary structs, and aggregate the data.

## Consequences

### Positive
*   **Minimal Overhead:** eBPF execution is highly optimized and runs entirely in the kernel. Lockless ring buffers prevent the tracing mechanism from impacting the latency of the traced events.
*   **Precise Attribution:** By joining kernel events with the `session_id` via BPF maps, we achieve perfect correlation of low-level exits to high-level VM sessions.

### Negative / Challenges
*   **eBPF Toolchain:** The build system must support compiling C-based eBPF programs alongside the Rust control plane (e.g., using `aya-rs` or `libbpf-rs`).
*   **Kernel Version Constraints:** Ring buffers require Linux 5.8+. The host kernel must be tightly controlled and updated to support the necessary BPF features.
*   **Complex Lifecycle Management:** The control plane must correctly load, attach, detach, and unload eBPF programs, and handle map synchronization during VM lifecycle events (creation, destruction).

## Related Documents
*   `ADRs/003-observability-driven-architecture.md`
*   `control-plane/specs/004-observability.md`
