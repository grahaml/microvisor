# ADR 003: Observability-Driven Architecture (ODA)

## Status
Accepted

## Context
Microvisor operates as a high-density, ultra-low latency private cloud orchestration plane. Traditional observability methods (like verbose text logging or sidecar agents) introduce unacceptable context switch overhead, memory allocation pressure, and CPU scheduling jitter. 

However, failing to observe the system at a deep level makes it impossible to guarantee the strict resource isolation, minimal latency, and hardware fencing required by our core mandates. We need a way to continuously verify that our "Layer 1" performance targets are met in production, not just in synthetic benchmarks.

As defined in **Constraint 008 (Transparent & Deep Observability)**, we must observe everything from high-level state changes down to VM-exits and L1 cache misses. Developers need a clear architectural mandate that defines how to approach this requirement before designing specific subsystems.

## Decision
We will adopt an **Observability-Driven Architecture (ODA)**. 

Observability is not a feature added after implementation; it is a structural prerequisite for every component. 

Specifically, this means:
1. **Always-On Instrumentation:** Observability primitives must be compiled in and available by default. While *emission* of data may be toggled dynamically (e.g., attaching/detaching eBPF programs or adjusting tracing filters), the code paths for measurement must not be hidden behind conditional compilation.
2. **Zero-Cost Abstractions:** We mandate the use of zero-cost or near-zero-cost primitives (like eBPF and PMUs) for all hot-path telemetry.
3. **Session Traceability:** Every action in the system must be casually linked. A unique `session_id` (high-entropy 64-bit integer) must be propagated across all layers. With 18 quintillion possible IDs, we prioritize the speed of integer-based map lookups over the global uniqueness of UUIDs, ensuring we never "run out" of IDs within the lifetime of a host.
4. **Binary Over Text:** Hot-path observability data must be emitted as structured binary data (e.g., via eBPF ring buffers) to avoid serialization and string formatting overhead in the critical path.
5. **Modularity & Extensibility:** The observability stack must be designed as a pluggable framework. Adding a new eBPF sensor, a hardware performance counter, or a control plane trace span must be a low-friction operation that does not require refactoring the core orchestration logic. This ensures the system can evolve as we identify new micro-architectural bottlenecks or kernel-level dependencies.

## Consequences

### Positive
*   **Verifiable Performance:** We gain continuous, production-level insight into system jitter, KVM exit rates, and cache misses without degrading the very performance we are measuring.
*   **Unified Tracing:** The `session_id` propagation guarantees that an architect can trace a high-level API failure directly to a hardware-level contention issue.
*   **Future-Proofing:** The modular design allows us to quickly pivot and add instrumentation for new kernel subsystems (e.g., io_uring, cgroup v3) as they become relevant.
*   **Clear Development Contract:** Engineers have a clear mandate: if a new feature cannot be deeply observed with near-zero overhead, the design must be reworked. Any performance regression >1% must be justified by a new ADR.

### Negative / Challenges
*   **Increased Complexity:** Implementing lockless ring buffers and eBPF programs is significantly more complex than standard `log::info!()` statements.
*   **Orchestration Overhead:** Managing the lifecycle of multiple modular eBPF components requires a robust loading and attachment manager in the control plane.
*   **Kernel Dependencies:** Relying heavily on eBPF and PMUs requires modern kernel features and strict alignment between the control plane and the host OS kernel version.
*   **Data Volume:** High-frequency, binary telemetry data will require an efficient, out-of-band ingestion pipeline to parse and aggregate the events without impacting the control plane.

## Related Documents
*   `constraints/008-transparent-deep-observability.md`
*   Future ADR: Control Plane Observability Stack (Tokio/Tracing)
*   Future ADR: Kernel Observability Stack (eBPF/Ringbuf)
*   Future ADR: Hardware-Assisted Micro-Architectural Auditing
