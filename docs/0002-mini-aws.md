# Architectural Specification: Bare-Metal MicroVM Orchestrator ("Mini-AWS")

This document outlines the low-level technical design for a high-density, ultra-low latency private cloud orchestration plane using Rust, Firecracker, and native Linux kernel primitives.

---

## 1. High-Level Architectural Overview

The target design treats the Linux kernel as a high-performance, programmable hardware multiplexer rather than a general-purpose operating system. The control plane is built as an asynchronous Rust runtime (`tokio`) that interacts directly with kernel interfaces via system calls and `ioctl` boundaries, avoiding the latency overhead of executing external CLI wrappers (e.g., `ip`, `dmsetup`).

### Core Design Principles

* **Single Process Topology:** Every guest microVM maps 1:1 to a host Linux process. Every guest vCPU maps 1:1 to a host `pthread`.
* **Zero-Shared-State Networking:** Elimination of host software bridges to prevent broadcast storms, ARP table scaling issues, and global lock contention.
* **Immutable Snapshot Pools:** Utilization of a single master memory/disk snapshot for warm pools, using eBPF and Copy-on-Write (CoW) to mutate state statelessly at the boundary.

---

## 2. Compute & Virtualization Engine

### Linux Primitives

* `/dev/kvm` ioctl API (`KVM_SET_USER_MEMORY_REGION`, `KVM_RUN`)
* Linux Completely Fair Scheduler (CFS) isolation
* cgroups v2 `cpuset` controller

### Implementation Architecture

To prevent heavy **VM-Exits**, L1/L2 cache invalidation, and Translation Lookaside Buffer (TLB) thrashing caused by the host OS migrating vCPU threads across physical cores, strict asymmetry must be configured between host and guest execution zones.

#### Host Boot Configuration (`/etc/default/grub`)

Isolate dedicated physical cores entirely from the host scheduler:

```text
isolcpus=2-15 nohz_full=2-15 rcu_nocbs=2-15

```

#### Rust Orchestrator Allocation

When spawning a Firecracker process, the control plane must immediately register the process PID into a dedicated cgroups v2 tree to enforce hardware boundaries before vCPU execution begins:

```bash
# Set up hardware pinning bucket
mkdir -p /sys/fs/cgroup/orchestrator/vm-01
echo "2-3" > /sys/fs/cgroup/orchestrator/vm-01/cpuset.cpus

# Migrate Firecracker VMM into the isolated subset
echo $FIRECRACKER_PID > /sys/fs/cgroup/orchestrator/vm-01/cgroup.procs

```

---

## 3. Storage Subsystem & Snapshot Fast-Restores

### Linux Primitives

* Device Mapper Thin Provisioning (`dm-thin`)
* Direct I/O (`O_DIRECT` + `O_NONBLOCK`)

### Implementation Architecture

Standard loop devices (`/dev/loopX`) introduce global lock contention within the host kernel block layer at scale. Storage density is achieved by using a backing raw NVMe partition sliced dynamically via Device Mapper targets.

```
                  [ Read-Only Base RootFS Image ]
                                 |
        +------------------------+------------------------+
        | (Snapshot Target)                               | (Snapshot Target)
[ VM_01 CoW Layer ]                               [ VM_02 CoW Layer ]
(dmsetup thin volume 1)                           (dmsetup thin volume 2)

```

#### Orchestration Flow

1. **Base Image:** Maintain a read-only base rootfs block device inside a thin pool.
2. **Instant Ephemeral Disks:** For every microVM instance, the Rust plane issues an `ioctl` to `/dev/mapper/control` to provision an instantaneous copy-on-write thin volume linked to the base target.
3. **Execution:** Firecracker opens the resulting `/dev/mapper/vm-XX` block device using `O_DIRECT`, allowing `io_uring` queues inside the VMM to bypass the host filesystem page cache entirely.

---

## 4. Stateless Multi-Tenant Networking

### Linux Primitives

* `TAP` virtual network kernel devices
* Traffic Control (`tc`) subsystem
* eBPF (`BPF_MAP_TYPE_ARRAY` + `tc` classifier hooks)

### Implementation Architecture

To decouple networking from the snapshot state, the guest OS inside the master snapshot is hardcoded with a static dummy network layout (IP: `169.254.1.2`, MAC: `06:00:aa:bb:cc:dd`). IP allocation is handled statelessly via 1:1 Network Address Translation (NAT) executed inside the host kernel data-path via eBPF.

```
 [ Guest MicroVM ] -> Thinks it is 169.254.1.2
        |
 (VirtIO-Net Pair)
        |
 [ Host TAP Interface ] -> Hooks: eBPF tc_egress / tc_ingress
        |                  (Rewrites 169.254.1.2 <-> 10.0.X.Y via Array Map)
        |
 [ Host Core Routing ] -> Direct /32 Point-to-Point Routing (No Bridge)

```

#### Control Plane Execution

1. **Lock-Free IPAM:** The Rust control plane tracks available IP slices in a global atomic bitset (`AtomicU64` arrays) to execute allocations with zero mutex contention across workers.
2. **eBPF Layer Injection:** Upon spawning a microVM, its allocated host-routable IP (e.g., `10.0.1.45`) is written to a dedicated 1-element eBPF array map tied to that instance's TAP device.
3. **Stateless Rewrite:** The eBPF program intercepts all egress packets on the host TAP interface, overwriting the source IP from `169.254.1.2` to `10.0.1.45`, updates the Layer 3 checksum incrementally (`bpf_l3_csum_replace`), and passes the packet to the host interface without network stack traversal.

---

## 5. Noisy Neighbor Defense & Memory Optimization

### Linux Primitives

* cgroups v2 unified memory limits (`memory.max`, `memory.swap.max`)
* NUMA Node Memory Pinning (`cpuset.mems`)
* Memory Advisement (`madvise` with `MADV_WILLNEED`) or `userfaultfd`

### Implementation Architecture

Cross-socket memory validation over Inter-Connect fabrics (e.g., UPI, Infinity Fabric) introduces a ~30–50ns latency penalty per memory access. MicroVM allocations must be strictly bounded to the NUMA node local to their assigned physical CPUs.

#### Hard Resource Limits via cgroups v2

```bash
# Enforce absolute execution inside local NUMA Node 0
echo "0" > /sys/fs/cgroup/orchestrator/vm-01/cpuset.mems

# Set absolute memory ceiling and strip swap capabilities entirely
echo "536870912" > /sys/fs/cgroup/orchestrator/vm-01/memory.max
echo "0" > /sys/fs/cgroup/orchestrator/vm-01/memory.swap.max

```

#### Optimizing Snapshot Memory Resumes

When resuming from a warm pool snapshot, loading memory lazily via standard `mmap` triggers massive spikes in **Major Page Faults** as the guest kernel initializes. The orchestrator must handle memory initialization through one of two strict execution modes:

* **Eager Pre-faulting Mode:** Force the host kernel to eagerly parse and populate the Extended Page Tables (EPT) prior to vCPU activation using the `MADV_WILLNEED` flag. This trades away ~10ms of initial startup latency to guarantee deterministic runtime latencies.
* **User-Space Faulting Mode (`userfaultfd`):** Register guest memory space to an asynchronous userfaultfd handler thread within the Rust control plane. This allows pages to be lazily requested over an active Unix Domain Socket (UDS) or fetched from a pre-allocated host memory pool without freezing the hypervisor process.

---

## 6. Kernel-Level Observability

### Linux Primitives

* KVM Tracepoints (`tracepoint:kvm:kvm_exit`)
* BPF Compiler Collection (BCC) / `bpftrace`

To validate that your cgroup isolation and core-pinning strategies are preventing execution thrashing, your observability pipeline must audit the frequency and causes of hypervisor context switches (VM-Exits).

### Hypervisor Exit Auditing Script (`trace_kvm_exits.bt`)

Feed this script directly into the host `bpftrace` subsystem to monitor exit storm signals scoped to your orchestration cgroup slice:

```cpp
/* Scopes telemetry purely to processes within the orchestrator cgroup tree */
tracepoint:kvm:kvm_exit
/ cgroup == cgroup_id("/sys/fs/cgroup/orchestrator") / {
    @[args->exit_reason] = count();
}

interval:s:5 {
    printf("--- KVM Exit Metrics (Last 5 Seconds) ---\n");
    print(@);
    clear(@);
}

```
