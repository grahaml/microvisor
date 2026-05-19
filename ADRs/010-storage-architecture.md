# ADR 010: Storage Architecture

## Status
Accepted (v1 / POC scope). Migration triggers for v2 are explicit below.

## Context
The per-VM root disk substrate is one of the most consequential architectural choices in Microvisor: it determines provisioning latency, density ceiling, crash-recovery complexity, and the I/O path for every guest. The current spec set (`control-plane/specs/001-storage-subsystem.md`, `control-plane/specs/008-storage-ioctl-implementation.md`) commits to **Device Mapper Thin Provisioning (`dm-thin`)** with direct `ioctl`s to `/dev/mapper/control`, but does so implicitly. This ADR makes the choice explicit, names the alternatives we rejected, and documents the conditions under which we'd revisit.

The SME review (`docs/003-expert-review.md`):
- §1.1 — flagged the in-spec ioctl sequence as incomplete (now fixed in Spec-008).
- §2.4 — flagged the absence of a pool-exhaustion policy (dm-thin's default is silent I/O hang).
- §3.3 — flagged the `mkfs.ext4 + mount` metadata-drive path as violating the declarative-infrastructure constraint (now replaced with `mke2fs -d` in Spec-008).

A longer-term concern that bites at scale: dm-thin's single metadata lock per pool serializes all CoW allocations and becomes the throughput ceiling at ~50+ concurrent allocating writers (every write that needs a new block goes through that lock). That's a v2 problem, not a v1 problem.

## Alternatives Considered

| Substrate | Pros | Cons | Verdict |
|---|---|---|---|
| **dm-thin (chosen)** | In-kernel CoW; mature; integrates with LVM2 tooling for ops; ioctl interface is fast and direct; aligns with the "Linux as hardware multiplexer" philosophy. | Single metadata lock per pool serializes CoW allocations (matters at high density only); ioctl sequence is finicky (we now have the correct doc). | **v1** |
| File-backed `.img` on ext4/xfs | Dead simple; debuggable with file tools; works on any filesystem. | No native CoW unless using reflinks (xfs) — and reflinks have their own quirks; per-VM page-cache pressure unless every open uses `O_DIRECT`; slower than dm-thin's direct-block path. | Rejected — gives up the direct-block performance we just built the spec around. |
| btrfs subvolume snapshots | Native CoW at the filesystem layer; mature snapshot semantics; easy to script. | btrfs operational complexity (balance, scrub, ENOSPC behavior); historical performance issues under high-churn workloads; ties us to btrfs on the host. | Rejected — btrfs ops complexity > dm-thin ops complexity for our use case. |
| ZFS clones (ZoL) | Best-in-class CoW correctness; excellent snapshot semantics. | Requires the ZFS kernel module (license-driven distribution friction); makes host setup non-trivial; ARC competes with KSM for host memory. | Rejected — host-setup friction is too high for a POC. |
| SPDK blobstore + `vhost-user-blk` | Bypasses the kernel block layer entirely; no per-pool metadata-lock ceiling; highest performance ceiling of any option. | Significant implementation cost; DPDK-style poll-mode driver setup; pulls a userspace storage stack into the orchestrator. | Rejected for v1; **named as the v2 substrate** if/when density forces the issue. |

## Decisions

### 1. Storage substrate (v1): dm-thin
**Decision:** Use Linux Device Mapper Thin Provisioning as the per-VM root-disk substrate. Per-VM ephemeral disks are CoW snapshots of a read-only base thin device, created via the four-step ioctl sequence specified in Spec-008.

**Implementation note:** For the POC, the orchestrator may shell out to `dmsetup` or use the `devicemapper` crate (which wraps `libdevmapper`) — the goal is to get the *correct* ioctl sequence in place, not to optimize the dispatch path. Replacing this with raw open-coded ioctls is a follow-up perf optimization, not a v1 requirement.

### 2. Pool-exhaustion policy (POC): `error_if_no_space=y`
**Decision:** Configure the thin pool with `error_if_no_space=y`. When the pool fills, allocating writes return `EIO` to the guest, which surfaces as filesystem errors inside the VM. Loud, fast, debuggable.

**Why not `dmeventd` auto-extend (yet):** Auto-extend requires an LVM2 stack with a VG that has free PEs to extend into, plus `dmeventd` running as a separate daemon. For a single-developer-machine POC, that's complexity we don't need — and we *want* the loud failure mode while we're developing the orchestrator.

The two-tier production posture (`dmeventd` primary + `error_if_no_space` fallback) is the right end state, but is **not** part of v1. It moves to `docs/004-deferred-from-review.md` as a deferred item with the trigger noted in §3 below.

### 3. v2 migration triggers
This ADR's substrate choice is sufficient up to the following thresholds. Any one of them reopens the ADR:

- **Density:** > 50 concurrent VMs sharing the same thin pool, **and** profiling shows the per-pool metadata lock as the dominant contention point. Mitigation order: (a) shard into N pools of ~50 VMs each — each pool gets its own metadata device and lock, so 4 pools of 50 ≈ 4× metadata throughput with minimal architectural change; (b) move to SPDK blobstore + `vhost-user-blk` (bypass the kernel block layer entirely; aligns with the "Linux as hardware multiplexer" philosophy but is a significant implementation cost).
- **Operations:** First production incident caused by a silent pool fill, **or** first deployment to a host where developer-grade loud failure is unacceptable. Trigger: implement the `dmeventd` auto-extend tier per `docs/004-deferred-from-review.md`.
- **Performance:** Sustained per-VM disk throughput requirements exceed what the kernel block layer can deliver at our concurrency (a real measurement, not a guess). Trigger: SPDK evaluation.

### 4. Metadata drive substrate (already in Spec-008)
The 1 MB ephemeral `ext4` sidecar drive (`/dev/vdb`) is built via `mke2fs -t ext4 -d <staging-dir>` — populates the filesystem from a host directory without ever mounting it. No loop-mount, no `CAP_SYS_ADMIN`, fully declarative per Constraint-003. This decision is locked in via the Spec-008 edit; this ADR records it for completeness.

### 5. Crash-recovery for thin devices
Orphaned thin-pool internal IDs are a real failure mode (orchestrator crashes after `DM_TARGET_MSG create_snap` but before tracking the ID). Recovery is owned by ADR 012's Phase B adoption scan and Phase C reconciliation loop, which cross-reference the IPAM bitset and the cgroup tree against the thin pool's internal-ID space.

## Consequences

**Positive:**
- v1 storage substrate is the same one the spec set already encodes — no rewrite, just locking the choice in with a written rationale.
- Pool-exhaustion behavior is now defined; the silent-hang failure mode is closed.
- Metadata-drive provisioning is fully declarative.
- v2 migration is named (sharded pools → SPDK), with the trigger thresholds explicit. No hidden upgrade work.

**Negative:**
- We will hit dm-thin's metadata-lock ceiling eventually if we succeed at the density story. The migration is non-trivial.
- `error_if_no_space=y` means a transient pool-pressure spike causes guest-visible I/O errors during the POC. Acceptable because the POC operator is the developer.

## Related
- `docs/003-expert-review.md` §1.1, §2.4, §3.3
- `docs/004-deferred-from-review.md` (auto-extend tier — when to add it)
- `control-plane/specs/001-storage-subsystem.md`
- `control-plane/specs/008-storage-ioctl-implementation.md`
- `ADRs/012-crash-recovery-contract.md` (orphaned-resource handling)
