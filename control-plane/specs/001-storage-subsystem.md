# Spec-001: Storage Subsystem & Snapshot Management

## Overview
Provides low-latency, high-density storage using Linux Device Mapper Thin Provisioning. Every VM receives an ephemeral, CoW-capable block device.

## Constraints
- **Direct I/O Path:** Use `O_DIRECT` + `O_NONBLOCK` to bypass the host page cache.
- **No Loop Devices:** Avoid loop devices to prevent global lock contention.
- **Ephemeral & Stateless:** Root filesystems are immutable/ephemeral.
- **Low Latency Control:** Provisioning must use direct `ioctl` to `/dev/mapper/control` to avoid `dmsetup` overhead.

## Implementation Details
1. **Thin Pool:** A master NVMe partition is managed as a `dm-thin` pool.
2. **Base Images:** Read-only base rootfs images are stored as thin volumes within the pool.
3. **Instance Provisioning:**
    - Issue `ioctl` to `/dev/mapper/control` to create a CoW snapshot of the base image for each VM.
    - Create an ephemeral `ext4` metadata drive for "Mission Packages" (secrets/tasks).
    - Resulting devices (rootfs and metadata) are passed to Firecracker as `/dev/vda` and `/dev/vdb`.
4. **VMM Configuration:** Firecracker opens the block devices with `O_DIRECT`, utilizing `io_uring` for asynchronous I/O.

## Integration
- Consumed by **Spec-000** during VM launch.
