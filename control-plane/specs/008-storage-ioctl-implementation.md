# Spec-008: Storage I/O & Device Mapper Implementation

## Overview
This specification details the native Rust implementation of the storage subsystem, moving from mocked snapshots to direct Linux Device Mapper `ioctl` interactions.

## Target: Device Mapper Thin Provisioning
The host node must be configured with a standard LVM/DM thin pool (e.g., `microvisor-pool`). The control plane will interact with this pool to create ephemeral, CoW (Copy-on-Write) snapshots for each VM.

## I/O Path Performance
- **Zero Page Cache:** All block devices must be opened by Firecracker with the `O_DIRECT` flag, which bypasses the host page cache.
- **Async I/O:** Submit requests via `io_uring` (when supported by the kernel and Firecracker) for an async, low-syscall-overhead I/O path. `io_uring` is orthogonal to caching — `O_DIRECT` is what eliminates the page cache; `io_uring` is the submission interface.

## DM Control Implementation (The `ioctl` Sequence)
To create a writable snapshot of a read-only base thin device, the control plane performs the following four-step sequence. Step 1 targets the pool device; steps 2–4 target `/dev/mapper/control`.

### 1. `DM_TARGET_MSG` on the thin pool → `create_snap <new_dev_id> <origin_dev_id>`
- **Goal:** Register the snapshot inside the thin pool's metadata. Until this message is acknowledged, no device-mapper device referencing `<new_dev_id>` can be activated — the pool simply doesn't know that ID exists yet.
- **Parameters:** A `dm_ioctl` struct targeting the **pool device** (not `/dev/mapper/control`), carrying the message string `create_snap <new_dev_id> <origin_dev_id>`.
- **Note:** Both `<new_dev_id>` and `<origin_dev_id>` are 24-bit thin-internal identifiers managed by the orchestrator. The control plane allocates `<new_dev_id>` from its own ID-space (e.g., a `BitVec`) and persists the allocation alongside the VM record.

### 2. `DM_DEV_CREATE`
- **Goal:** Register a new device-mapper device node in the kernel (e.g., `vm-{id}-rootfs`). This creates the device in the **inactive** state.

### 3. `DM_TABLE_LOAD`
- **Goal:** Bind the device created in step 2 to a `thin` target referencing the snapshot registered in step 1.
- **Parameters:**
    - Sector range (matches base image size).
    - Target type: `thin`.
    - Args: `<pool_device_path> <new_dev_id>`.

### 4. `DM_DEV_SUSPEND` with flag `0` ("resume")
- **Goal:** Transition the device from inactive to active, making it available at `/dev/mapper/vm-{id}-rootfs`.
- **Note:** `DM_DEV_SUSPEND` is the same ioctl used for both suspend and resume. The two operations are distinguished by the `DM_SUSPEND_FLAG` bit in the `flags` field of the `dm_ioctl` struct: flag set = suspend, flag cleared (0) = resume. Activating a freshly-created device is a resume operation with the flag cleared.

### 5. Cleanup (`DM_DEV_REMOVE`) on VM teardown
- **Goal:** Destroy the device-mapper node. The thin pool internal ID is released separately via a `DM_TARGET_MSG` `delete <new_dev_id>` to the pool, which must follow the `DM_DEV_REMOVE`.

## Mission Packages (Metadata Drive)
- **Format:** `ext4`.
- **Provisioning (declarative, no loop-mount on host):**
    1. Create a sparse file: `fallocate -l 1M /tmp/vm-{id}-metadata`.
    2. Build the filesystem and inject the contents in a single step using `mke2fs -t ext4 -d <staging-dir> /tmp/vm-{id}-metadata`. The `-d` flag (e2fsprogs ≥ 1.43) populates the new filesystem from a host directory tree without ever mounting it. This avoids `CAP_SYS_ADMIN`, loop-device exhaustion, and umount race conditions, and keeps the operation fully declarative per Constraint-003.
    3. Pass as `/dev/vdb` to Firecracker.

## Implementation Details
- **Crate:** Use the `nix` crate for safe `ioctl` wrappers or define custom `ioctl!` macros.
- **Safety:** Ensure `DM_DEV_REMOVE` is called in the `Destroyed` state of the state machine, even if the orchestrator process crashed previously (via startup cleanup scan).

## Related Documents
- `specs/001-storage-subsystem.md`
- `constraints/005-host-resource-fencing.md`
