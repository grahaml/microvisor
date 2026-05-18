# Constraint-007: Extreme Performance & Ultra-Low Latency

## Requirement
The system must achieve "Layer 1" performance by treating the host as a programmable hardware multiplexer. 

## Target Metrics
- **Sub-Second Boot Time:** From microVM spawn request to guest execution.
- **Nano-Second Latency:** Minimal overhead for vCPU execution, memory access, and networking data-path.

## Principles
1. **Direct Kernel Interfaces:** Favor `ioctl` and system calls over executing external CLI wrappers.
2. **Hardware Passthrough/Mapping:** 1:1 mapping of virtual to physical resources (vCPU, NUMA, I/O).
3. **Bypass Overheads:** Utilize `O_DIRECT`, eBPF `tc` hooks, and `io_uring` to bypass host filesystem page caches and standard bridge networking.
4. **Deterministic Execution:** Use `isolcpus` and strict pinning to eliminate jitter caused by host scheduler migrations.
