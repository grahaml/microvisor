# Spec-009: VM Identity Injection via Metadata Drive

## Status
Planned. Not yet implemented.

## Problem

When a VM boots it inherits the hostname baked into the base rootfs image
(`/etc/hostname`), so every VM reports the same hostname — currently the
host machine's name. There is no mechanism to inject per-VM identity
(hostname, VM ID, session ID, tenant metadata) into the guest at boot time
without per-launch latency.

## Goals

- Guest hostname matches the orchestrator's VM ID (e.g. `vm-test-001`)
- Zero per-VM latency overhead — no blocking I/O on the critical boot path
- No modification to the base rootfs on every launch
- Extensible: the same mechanism should carry arbitrary per-VM config
  (credentials, task manifests, network config) in future

## Design

### Metadata drive as the config channel

The metadata drive (`/dev/vdb`) already exists as a per-VM ephemeral ext4
image injected by the orchestrator before boot. It is the natural carrier
for per-VM identity. No new infrastructure is required.

### Orchestrator side (per-VM, already fast)

In `provision_vm`, write the VM ID to the metadata staging directory before
calling `MetadataDrive::create`. This is a single `fs::write` call — no
additional latency.

```rust
// In provision_vm, before MetadataDrive::create:
std::fs::write(staging_dir.join("hostname"), format!("{}\n", config.id))?;
```

Files to inject (extend as needed):

| File in metadata drive | Content |
|---|---|
| `hostname` | VM ID string, e.g. `vm-test-001` |
| `session_id` | Session ID as decimal string |
| `vm_id` | Full VM ID (same as hostname for now) |

### Base rootfs side (one-time, ~60s re-import)

Bake a systemd one-shot unit into `resources/browser-rootfs.ext4` that
runs at `sysinit.target`, before network or any application services:

**`/usr/lib/microvisor/apply-metadata.sh`**
```bash
#!/bin/bash
MOUNT=/mnt/metadata
mkdir -p "$MOUNT"
mount -t ext4 -o ro /dev/vdb "$MOUNT" 2>/dev/null || exit 0
[ -f "$MOUNT/hostname" ] && {
    cat "$MOUNT/hostname" > /etc/hostname
    hostname -F /etc/hostname
}
umount "$MOUNT"
```

**`/etc/systemd/system/microvisor-metadata.service`**
```ini
[Unit]
Description=Apply Microvisor VM configuration from metadata drive
DefaultDependencies=no
Before=sysinit.target

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/usr/lib/microvisor/apply-metadata.sh

[Install]
WantedBy=sysinit.target
```

Inject via `scripts/configure-rootfs.sh` (to be written): loop-mounts the
base image, installs the script and unit, enables it, unmounts. Idempotent
via `resources/.rootfs-configured` sentinel.

After running, the DM thin pool must be re-imported:
```
sudo scripts/teardown-pool.sh --purge
sudo make pool
```

### Makefile

Add `rootfs-configure` target with sentinel, wired into `make all` before
`pool` so the pool is always imported from a configured image.

## Latency profile

| Step | Cost | When |
|---|---|---|
| `fs::write` hostname to staging | <1ms | Per VM launch |
| `MetadataDrive::create` (mkfs.ext4 + copy) | ~5ms | Per VM launch (already paid) |
| Systemd unit execution in guest | ~10ms | Guest boot (already in sysinit) |
| Base image modification | ~60s | One-time, at `make rootfs-configure` |

No per-launch blocking I/O is added beyond what already exists.

## Re-evaluation trigger

Implement this spec when:
- Guest hostname needs to be correct for any reason (logging, networking,
  multi-VM debugging)
- The metadata drive is being extended to carry other per-VM config
- Any guest-side tooling references the hostname
