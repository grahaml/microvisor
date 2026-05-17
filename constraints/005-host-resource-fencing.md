# Constraint: Host Resource Fencing

## Definition
The "mini-cloud" ecosystem must not be able to starve the host machine of resources (CPU, Memory, or I/O). Resource isolation is just as critical as security isolation.

## What We Do NOT Do
*   **No Unbounded Execution:** The Orchestrator and Firecracker processes must not run unconstrained in the default host process namespace.
*   **No CPU Thrashing:** VMs should not randomly float across all host CPU cores, which causes context-switching overhead and impacts host responsiveness.
*   **No I/O Monopolization:** A single VM must not be able to consume 100% of the host disk or network bandwidth.

## Implementation Guide
*   **cgroups v2:** The Rust Orchestrator and all child Firecracker processes must run inside a dedicated `systemd` slice or `cgroup` with strict maximum memory limits.
*   **CPU Pinning:** The Orchestrator must assign specific, dedicated host CPU cores (e.g., using `taskset` or `numactl`) to the Firecracker microVMs. The host OS should ideally be isolated to its own set of cores.
*   **Firecracker Rate Limiting:** The Orchestrator must configure Token Bucket rate limiters via the Firecracker REST API for both `virtio-net` (networking) and `virtio-blk` (storage) devices on every VM launch.
