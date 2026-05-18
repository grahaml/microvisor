# Spec-000: Compute Topology & Virtualization Engine

## Overview
This component manages the lifecycle of guest microVMs, ensuring a 1:1 mapping between guest vCPUs and physical host threads. It leverages the KVM API and cgroups v2 for hardware-enforced isolation.

## Constraints
- **Hardware Isolation:** 1:1 mapping of guest microVMs to host processes.
- **Resource Fencing:** Strict CPU pinning using cgroups v2 (`cpuset`).
- **No Migration:** vCPU threads must never migrate across physical cores.
- **Extreme Performance:** Alignment with **Constraint-007** for sub-second boot and nano-second latency.

## Implementation Details
1. **Direct Kernel Interfaces:** The control plane must interact with `/dev/kvm` and cgroups via direct syscalls/ioctls to minimize execution overhead.
2. **Host Boot Configuration:** Host must be booted with `isolcpus`, `nohz_full`, and `rcu_nocbs` for the target execution range (e.g., cores 2-15).
2. **Process Spawning:** The control plane spawns Firecracker VMM instances.
3. **Cgroup Integration:**
    - Create a dedicated cgroup per VM: `/sys/fs/cgroup/orchestrator/vm-<id>`.
    - Configure `cpuset.cpus` to assign specific isolated cores.
    - Write the VMM PID to `cgroup.procs`.
4. **Integration:**
    - Storage is provisioned via **Spec-001**.
    - Networking is attached via **Spec-002**.
    - Memory is constrained via **Spec-003**.
