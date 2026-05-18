# Spec-003: Memory Optimization & NUMA Pinning

## Overview
Ensures deterministic memory access latencies by enforcing NUMA locality and optimizing snapshot resume paths.

## Constraints
- **NUMA Locality:** MicroVMs are strictly bounded to the NUMA node local to their assigned CPUs (preventing 30-50ns interconnect latency).
- **Resource Limits:** Hard memory ceilings via cgroups v2 (`memory.max`).
- **No Swap:** Swap is disabled for all VM processes to ensure deterministic response times.

## Implementation Details
1. **NUMA Pinning:** Configure `cpuset.mems` in the VM's cgroup to match the physical CPU socket.
2. **Memory Limits:** Set `memory.max` and `memory.swap.max=0`.
3. **Resume Optimization:**
    - **Eager:** Use `madvise(MADV_WILLNEED)` to pre-fault guest memory.
    - **Lazy:** Use `userfaultfd` for on-demand page fetching over UDS.

## Integration
- Applied by **Spec-000** during cgroup setup.
