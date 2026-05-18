# ADR 006: Hardware-Assisted Micro-Architectural Auditing

## Status
Accepted

## Context
Our Microvisor mandates strict hardware-level resource fencing and near-zero latency variations (**Constraint 008**). We pin vCPU threads to physical cores and enforce NUMA locality to avoid cross-socket memory accesses. However, software-level tracing (even eBPF) cannot tell us *why* a function took longer than expected if the delay is caused by hardware contention (e.g., L3 cache eviction caused by a "noisy neighbor" on another core, or branch mispredictions in the control plane hot loop).

We need an observability layer that dips below the kernel and directly monitors CPU micro-architectural behavior.

## Decision
We will use **Performance Monitoring Units (PMUs)** via Linux `perf_events` and eBPF to gather hardware-level telemetry.

1. **Required Metrics:** The control plane and telemetry stack must continuously monitor:
    *   L1/L2/L3 Cache Misses.
    *   TLB Misses (Virtual and Physical).
    *   Branch Misprediction Rates.
    *   Local vs. Remote NUMA Memory Accesses.
2. **Scoping:** Hardware counters will be sampled on a per-cgroup (per-VM) basis, correlated to the 64-bit `session_id`.
3. **eBPF Integration:** We will use `bpf_perf_event_read_value()` within our eBPF programs to read hardware counters exactly when KVM exits or CFS scheduling events occur.

## Implementation Strategy

To ensure hardware auditing does not violate our "Zero-Cost" mandate, we will employ the following strategies:

1. **Hardware-Assisted Counting:** We will prioritize "Counting Mode" over "Sampling Mode." The PMU will maintain running totals in dedicated hardware registers. We will only read these registers during existing context-switch events (e.g., KVM exits), making the cost of the read practically invisible to the guest's execution slice.
2. **Piggybacked Reads via eBPF:** We will not use a separate monitoring process to poll counters. Instead, our eBPF programs attached to `kvm_exit` will perform the read. Since a VM exit is already a high-latency event, adding a few nanoseconds to read a PMU register introduces zero meaningful jitter to the guest.
3. **Cgroup-Scoped Attribution:** We will utilize `perf_event_open` attached to the VM's cgroup file descriptor. This allows the Linux kernel to handle the "context switching" of hardware counters, ensuring that metrics like L3 cache misses are accurately attributed to the specific `session_id` even across vCPU migrations (if any occur).
4. **Register Multiplexing & Prioritization:** Given the physical limit of ~4-8 PMU registers per core, we will prioritize a "Core Four" set of metrics (L3 Misses, TLB Misses, Instructions Retired, and Local/Remote Memory). Less critical metrics will be multiplexed by the kernel, providing a high-fidelity statistical view without requiring additional hardware resources.

## Consequences

### Positive
*   **Contention Detection:** We can detect subtle performance regressions (like cache thrashing) that otherwise manifest only as vague latency spikes.
*   **Validation of NUMA/Pinning:** Hard data on remote memory accesses will definitively prove whether our cgroup v2 `cpuset` configurations are working as intended.
*   **Complete Lifecycle View:** Combined with ADRs 004 and 005, we achieve the ultimate "single pane of glass"—from an async Rust API call, down to the kernel KVM exit, and all the way to the CPU's cache hit rate.

### Negative / Challenges
*   **Hardware Dependency:** PMU counters vary significantly between Intel, AMD, and ARM processors. The telemetry logic must gracefully handle missing counters or abstract hardware differences.
*   **Privilege Requirements:** Accessing raw PMU data requires elevated capabilities (`CAP_PERFMON` or `CAP_SYS_ADMIN`), requiring the orchestrator to run with high privileges.
*   **Counter Limits:** Modern CPUs have a limited number of PMU registers (often 4 or 8). We may have to multiplex events, which reduces accuracy and adds overhead.

## Related Documents
*   `ADRs/003-observability-driven-architecture.md`
*   `ADRs/004-ebpf-kernel-tracing.md`
*   `constraints/008-transparent-deep-observability.md`
