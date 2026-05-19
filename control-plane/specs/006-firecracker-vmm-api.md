# Spec-006: Firecracker VMM API Integration

## Overview
This specification defines how the Microvisor control plane interacts with the Firecracker VMM process to configure hardware, attached resources, and initiate the guest boot sequence.

## Communication Channel
- **Mechanism:** UNIX Domain Socket (UDS).
- **Socket Path:** `/run/microvisor/vm-{id}.sock`.
- **Protocol:** HTTP over UDS (Standard Firecracker API).

## API Interaction Lifecycle

### 1. Pre-Boot Configuration
After spawning the Firecracker process but before sending the `InstanceStart` command, the control plane must issue the following `PUT` requests:

#### `PUT /boot-source`
- **Goal:** Define the kernel and boot arguments.
- **Parameters:**
    - `kernel_image_path`: Path to the guest kernel (e.g., `resources/vmlinux`).
    - `boot_args`: `console=ttyS0 reboot=k panic=1 pci=off nomodules`.

#### `PUT /drives/{id}`
- **Goal:** Attach the rootfs and metadata drives.
- **Parameters:**
    - `drive_id`: `rootfs` (vda), `metadata` (vdb).
    - `path_on_host`: Path to the DM snapshot or metadata image.
    - `is_root_device`: `true` for vda.
    - `is_read_only`: `false`.

#### `PUT /network-interfaces/{id}`
- **Goal:** Attach the TAP device.
- **Parameters:**
    - `iface_id`: `eth0`.
    - `host_dev_name`: Name of the TAP device created by the networking module.

#### `PUT /machine-config`
- **Goal:** Configure vCPU and memory.
- **Parameters:**
    - `vcpu_count`: N.
    - `mem_size_mib`: M.
    - `smt`: `false` (do not advertise SMT to the guest).

> **⚠️ SMT host policy — `smt: false` alone is insufficient for multi-tenant isolation.**
>
> Firecracker's `smt` flag only controls whether SMT is exposed *inside the guest*. It does **not** prevent the host scheduler from co-locating two unrelated VMs' vCPUs on the two SMT siblings of the same physical core, which is the actual MDS / L1TF / Spectre-v2 cross-VM leak vector.
>
> For multi-tenant deployments, the host must additionally do **one** of:
> 1. Boot with `nosmt` on the kernel command line (disables SMT entirely; halves logical-CPU count, eliminates the side channel). What public clouds running multi-tenant Firecracker do for hostile workloads.
> 2. Apply **sibling-aware pinning** in the cgroup `cpuset.cpus`: any physical core's SMT siblings (read from `/sys/devices/system/cpu/cpuN/topology/thread_siblings`) must be assigned to the same VM, never split across tenants. This is the AWS Nitro approach.
>
> For the POC (single-tenant), neither is required, but the policy must be selected and documented in `ADRs/009-multi-tenancy-threat-model.md` before any second tenant joins the host.

### 2. Boot Initiation
#### `PUT /actions`
- **Body:** `{ "action_type": "InstanceStart" }`

### 3. Monitoring & Management
- **`GET /`:** Basic health check.
- **`GET /vm`:** Retrieve current VM state.

## Implementation Details
- **Async Client:** Use `hyper` with a custom UNIX connector or a specialized Firecracker SDK crate.
- **Serialization:** Use `serde_json` to construct request bodies.
- **Timeout:** Any API request exceeding 100ms should trigger a `StateMachine` failure and emergency teardown.

## Related Documents
- `specs/000-compute-topology.md`
- `specs/005-state-machine-orchestrator.md`
