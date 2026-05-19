# Constraint-007: Extreme Performance & Ultra-Low Latency

## Requirement
The system must achieve "Layer 1" performance by treating the host as a programmable hardware multiplexer. 

## Target Metrics

All targets are subject to revision once a CI benchmark establishes a baseline. They are *defensible engineering targets*, not marketing aspirations.

| Metric | Target | Notes |
|---|---|---|
| p99 cold boot (spawn → first guest instruction) | **< 200 ms** | Firecracker cold boot is ~125 ms baseline; the budget covers orchestrator overhead. |
| Snapshot resume (paused → running) | **< 10 ms** | Firecracker snapshot-restore is ~3–5 ms; budget covers state-machine + cgroup setup. |
| vCPU `KVM_RUN` re-entry after VM-exit | **< 5 µs** | Hardware-bounded; primarily a measure of mitigations + interrupt-handling overhead. |
| eBPF NAT per-packet overhead (64-byte packet) | **< 200 ns** | Measured in the eBPF program itself, not end-to-end. |
| Control-plane API request → state machine transition | **< 1 ms** | Excludes the work the transition kicks off; this is dispatch latency only. |

Nanosecond claims for "memory access" are not engineering targets — they are hardware physics. The goal is to **not regress** the hardware floor through software overhead, not to "achieve" nanosecond latency.

## Principles
1. **Direct Kernel Interfaces:** Favor `ioctl` and system calls over executing external CLI wrappers.
2. **Hardware Passthrough/Mapping:** 1:1 mapping of virtual to physical resources (vCPU, NUMA, I/O).
3. **Bypass Overheads:** Utilize `O_DIRECT`, eBPF `tc` hooks, and `io_uring` to bypass host filesystem page caches and standard bridge networking.
4. **Deterministic Execution:** Use `isolcpus` and strict pinning to eliminate jitter caused by host scheduler migrations.
