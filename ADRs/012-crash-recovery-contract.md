# ADR 012: Crash-Recovery Contract

## Status
Accepted. All four components are required for v1; sequencing is given below.

## Context
The state machine spec (`control-plane/specs/005-state-machine-orchestrator.md`) describes happy-path transitions and "Rich Errors" that carry failure state. It does **not** specify what happens when the orchestrator itself crashes, when a Firecracker child dies unexpectedly, or when provisioning fails partway through a transition. For a system whose stated goal is deterministic lifecycle, those silences are load-bearing.

The four uncovered failure modes are:

1. **Orphan VMM processes on orchestrator crash.** Without `prctl(PR_SET_PDEATHSIG, SIGKILL)` at fork time, Firecracker children survive their parent and accumulate as ghost VMs across restarts.
2. **Adoption on restart.** A restarted orchestrator should be able to scan `/sys/fs/cgroup/orchestrator/`, find pre-existing per-VM cgroups, reattach to their UDS sockets, and re-enter the `Running` state. Otherwise every orchestrator restart is a fleet-wide outage.
3. **Partial-provisioning rollback.** A lifecycle that crosses `ProvisioningStorage → ConfiguringNetwork → InitializingCgroup → LaunchingVMM` allocates four kinds of resources (thin-pool snapshot ID, TAP device, IPAM bitset slot, cgroup tree). On mid-flight failure or orchestrator crash, each must be cleaned up — in-process Drop machinery is insufficient.
4. **Reconciliation loop.** Without periodic divergence-detection between in-memory state and kernel-visible state, leaks accumulate silently.

## Decision

All four components are part of v1. They are sequenced as follows:

### Phase A — Must ship in the first orchestrator commit
1. **`prctl(PR_SET_PDEATHSIG, SIGKILL)` on every Firecracker child.** Set immediately after `fork()`, before `exec()`. One-liner via the `nix` crate; no downside. Prevents orphan VMM accumulation on orchestrator crash.
2. **Per-state rollback handlers.** Each `VmStateMachine` state implements an idempotent `cleanup()` method that releases every resource it acquired. The state machine's error path composes these in reverse order. Idempotence is non-negotiable: `cleanup()` must succeed on the second call even if the first partially failed and was retried.

### Phase B — Must ship before the orchestrator is allowed to be restarted in place
3. **Adoption scan on startup.** Before accepting any new request, the orchestrator:
   - Walks `/sys/fs/cgroup/orchestrator/vm-*` to enumerate pre-existing VM cgroups.
   - For each, parses the VM ID from the cgroup name.
   - Attempts to connect to the UDS at `/run/microvisor/vm-{id}.sock` (or the `jailer`-rooted equivalent).
   - If the VMM responds healthy: re-enter the `Running` state for that VM, rebuild the in-memory `VmStateMachine` from the cgroup + UDS introspection.
   - If the VMM is unresponsive but the cgroup exists: enter a `Recovering` substate that runs `cleanup()` for every resource type associated with the VM ID, then transitions to `Destroyed`.

### Phase C — Required for production posture (post-POC)
4. **Reconciliation loop.** A periodic (every 30 s) async task that:
   - Diffs the orchestrator's in-memory VM set against the cgroup tree, the TAP device list (`/sys/class/net/`), the IPAM bitset, and the thin-pool internal-ID space.
   - Emits a metric per leaked resource type.
   - For unambiguously-owned-but-orphaned resources (e.g., a TAP device whose VM ID matches no live state machine), invokes the per-state `cleanup()` handler.
   - Refuses to act on ambiguous resources; surfaces them as alerts.

   Inspired by Kubernetes' kube-controller-manager reconciliation pattern.

## Implementation Notes

- The `cleanup()` handlers in Phase A must work without the orchestrator's full in-memory state — they have to be reconstructable from kernel state alone (cgroup name, TAP device name, thin-pool internal-ID lookup). This shapes how resource identifiers are assigned: every resource must encode its VM ID in its name.
- The adoption scan in Phase B requires the UDS path and jailer chroot layout to be deterministic from the VM ID. ADR 009 already commits to `jailer` from day one; the jailer's directory layout (`/srv/jailer/firecracker/<vm-id>/`) gives us a stable on-disk anchor.
- The reconciliation loop in Phase C should reuse the adoption-scan code rather than duplicating it.

## Consequences

**Positive:**
- The orchestrator can be restarted in place without a fleet-wide outage (Phase B).
- Mid-flight failures don't leak resources (Phase A).
- Long-term operation doesn't accumulate cruft (Phase C).
- The Phase A + B work is bounded; Phase C can be staged after the POC ships.

**Negative:**
- Phase A's idempotent cleanup discipline costs orchestrator code on every state, every release path. Every new resource type added later has to ship its own `cleanup()`.
- The adoption-scan logic is a second code path through state-machine construction; it must be kept in sync with the happy-path logic. Worth investing in shared test fixtures.

## Related
- `docs/004-expert-review.md` §2.2, §4.3, §4.4
- `control-plane/specs/005-state-machine-orchestrator.md`
- `ADRs/009-multi-tenancy-threat-model.md` (jailer commitment — anchors the adoption scan's on-disk layout)
- `docs/003-microvm-improvement-opportunities.md` §11
