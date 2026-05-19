# Spec-003: Memory Optimization & NUMA Pinning

## Overview
Ensures deterministic memory access latencies by enforcing NUMA locality and optimizing snapshot resume paths.

## Constraints
- **NUMA Locality:** MicroVMs are strictly bounded to the NUMA node local to their assigned CPUs (preventing 30-50ns interconnect latency).
- **Resource Limits:** Hard memory ceilings via cgroups v2 (`memory.max`).
- **No Swap:** Swap is disabled for all VM processes to ensure deterministic response times.

## Implementation Details

### 1. Topology Discovery (Startup, Once)
On orchestrator startup, read `/sys/devices/system/node/online` to enumerate NUMA nodes. For each node, read `/sys/devices/system/node/nodeN/cpulist` to map CPUs to nodes. Cache the topology for the orchestrator's lifetime. Single-socket and consumer hosts are valid and common targets.

### 2. NUMA Pinning (Branches on Topology)
- **If `num_nodes == 1`** (typical for consumer hosts, single-socket Xeons, Threadripper-1S, EPYC-1S, and most workstations): set `cpuset.mems = "0"`. No remote-NUMA penalty exists, so no further pinning is needed. **Skip remote-NUMA observability paths** entirely on this branch — they would emit no signal and waste CPU.
- **If `num_nodes > 1`:** determine the node owning the assigned `cpuset.cpus` from the cached topology, and set `cpuset.mems` to that node ID. Refuse a config that mixes CPUs from multiple nodes unless explicitly opted into (cross-socket pinning trades isolation for capacity).

The orchestrator must not panic, refuse to start, or emit warnings on single-NUMA hosts. Treat single-node as a first-class case.

### 3. Memory Limits
Set `memory.max` (hard ceiling, per-request) and `memory.swap.max=0` (swap permanently disabled for VM cgroups to preserve deterministic latency).

### 4. Resume Optimization
- **Eager:** Use `madvise(MADV_WILLNEED)` to pre-fault guest memory. Recommended POC default — deterministic, simple, no async fault-handling code path.
- **Lazy:** Use `userfaultfd` for on-demand page fetching over UDS. Higher complexity; defer until VM density justifies it.
- **Optional optimization:** `mmap(... MAP_POPULATE | MAP_HUGETLB ...)` is cheaper than `mmap` + `madvise(MADV_WILLNEED)` for the eager path — single syscall, kernel does the page-table population in one batch.

## Integration
- Applied by **Spec-000** during cgroup setup.
