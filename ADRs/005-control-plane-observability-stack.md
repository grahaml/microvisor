# ADR 005: Async Trace Propagation with Unified Session IDs

## Status
Accepted

## Context
The Microvisor control plane is built in Rust using the asynchronous `tokio` runtime. In asynchronous environments, standard logging and timing mechanisms break down because a single logical request (e.g., "Start VM 123") is multiplexed across multiple OS threads and interleaved with other tasks. 

Following **Constraint 008** and **ADR 003**, we must be able to trace a session end-to-end. We need a way to propagate a `session_id` through the async execution graph and seamlessly link these high-level control plane events with the low-level kernel events defined in **ADR 004**.

## Decision
We will utilize the **`tracing` ecosystem** (`tokio-tracing`) as our standard observability framework within the Rust control plane.

1. **Span-Based Tracing:** Every major lifecycle operation will create a span.
2. **Session ID Ingestion:** At the entry point of any VM-related operation, a unique `session_id` (64-bit integer) will be generated.
3. **Task-Local Storage Propagation:** We will use `tracing::Instrument` to attach spans to asynchronous futures.
4. **Syscall Auditing:** We will use `tracing` to instrument critical system call boundaries (e.g., wrapping `ioctl` or `mmap` calls) to track latency and ensure the control plane remains non-blocking.
5. **Bridging User and Kernel Space:** The mapping between the OS-level identifier (PID/cgroup ID) and the `session_id` will be written to eBPF BPF Hash Maps.
6. **Unified Telemetry Sink:** A custom `tracing_subscriber::Layer` will export structured spans for correlation with eBPF ring buffer events.

## Consequences

### Positive
*   **Async-Aware:** `tracing` correctly handles yielding and resuming of futures, ensuring accurate timing and context regardless of thread migrations.
*   **Structured Data:** Spans encourage key-value pairs (e.g., `session_id = "abc", action = "boot"`), eliminating the need to parse text logs later.
*   **Seamless Correlation:** By injecting the same `session_id` into the eBPF maps, we create a unified timeline of both user-space orchestration decisions and kernel-space execution realities.

### Negative / Challenges
*   **Discipline Required:** Developers must rigorously remember to `.instrument()` spawned tasks, or the context will be lost across task boundaries.
*   **Subscriber Overhead:** While `tracing` is fast, emitting spans is not entirely zero-cost. We must carefully tune the subscriber to avoid blocking the `tokio` worker threads on telemetry I/O.
*   **Dependency Size:** Adding the `tracing` ecosystem pulls in a significant number of dependencies.

## Related Documents
*   `ADRs/003-observability-driven-architecture.md`
*   `ADRs/004-ebpf-kernel-tracing.md`
