# ADR 009: Multi-Tenancy Threat Model, SMT Policy, and Sandboxing

## Status
Accepted (POC scope). The multi-tenant SMT policy is deferred until a second tenant joins the host; the sandboxing decisions are effective immediately.

## Context
Microvisor's pitch is hardware-enforced multi-tenant isolation. The spec set documents the building blocks (1:1 vCPU pinning, cgroups v2, eBPF NAT, ephemeral rootfs) but the SME review (`docs/003-expert-review.md` §1.4, §2.1) flagged that:

- `smt: false` in the Firecracker config controls only what the **guest** sees; it does not prevent the host scheduler from co-locating two unrelated VMs' vCPUs on the two SMT siblings of the same physical core, which is the actual MDS / L1TF / Spectre-v2 cross-VM side channel.
- The Firecracker process can run *without* the `jailer` binary, which means the default chroot, capabilities-drop, and seccomp profile are skipped — undoing half of Firecracker's isolation story.
- The Rust orchestrator itself holds `/dev/kvm`, `/dev/mapper/control`, and BPF capabilities, but has no sandbox of its own beyond running as a non-root user.

This ADR locks in the posture for the POC and identifies the upgrade path before multi-tenant operation.

## Decisions

### 1. SMT policy — POC: single-tenant, allocator built for drop-in upgrade
For the POC the host is operated as **single-tenant** (option C of the review). We do **not** boot the host with `nosmt`, and we do **not** require sibling-aware pinning at runtime.

However, **the cpuset allocator interface must be designed so that a sibling-aware policy is a drop-in upgrade**, not a rewrite. Concretely:

- The allocator is implemented behind a `trait CpusetAllocator` (or equivalent) that returns a `cpuset.cpus` string given a request for N cores.
- The POC implementation (`NaiveCpusetAllocator`) hands out the next N logical CPUs from a free pool — SMT-blind.
- A future `SiblingAwareCpusetAllocator` reads `/sys/devices/system/cpu/cpuN/topology/thread_siblings` at startup, builds a sibling map, and only hands out **whole physical cores** (both siblings) to a single VM — never splitting siblings across tenants.
- The switch is a config-only change once the second tenant lands. The state machine, cgroup writer, and request-validation paths know nothing of the allocator strategy.

This preserves the simplest possible POC while ensuring the multi-tenant upgrade is a small, well-scoped change rather than a refactor.

### 2. `jailer` is mandatory from day one
Every Firecracker process is launched via the `jailer` binary that ships with Firecracker. No exceptions, no "POC only" carve-out.

The `jailer` provides — for free — chroot to a per-VM directory, cgroup attachment, capability drop, UID/GID switch, and the default Firecracker seccomp filter. Retrofitting it after the orchestrator already manages bare Firecracker processes is significantly more painful than building on top of it from the start.

**Implementation note:** The state machine's `LaunchingVMM` state invokes `jailer`, not `firecracker` directly. The jailer's `--exec-file` flag points at the Firecracker binary; the jailer handles the rest. The jail directory layout (`/srv/jailer/firecracker/<vm-id>/`) becomes the orchestrator's per-VM workspace.

### 3. Firecracker default seccomp profile is preserved
We do **not** pass `--seccomp-level 0` or supply a custom JSON filter that broadens the syscall set. The default Firecracker seccomp filter (loaded automatically) is the baseline; any future override requires its own ADR.

### 4. Rust orchestrator sandboxing — deferred to first non-team deployment
For the POC the orchestrator runs as a non-root user with the capabilities it requires (`CAP_NET_ADMIN`, `CAP_BPF`, access to `/dev/kvm` and `/dev/mapper/control` via group permissions). It is **not** wrapped in a systemd unit with `ProtectSystem=strict`, no custom seccomp filter is applied, `MemoryDenyWriteExecute=true` is **not** set.

This is acceptable for a developer-machine POC. **Before any non-team user touches the orchestrator**, a follow-up ADR must specify:
- The systemd hardening directives (`ProtectSystem`, `ProtectHome`, `NoNewPrivileges`, `RestrictAddressFamilies`, `MemoryDenyWriteExecute`).
- A seccomp-bpf filter for the orchestrator process itself.
- Whether the orchestrator's process tree is itself jailed (defense in depth).

### 5. Multi-tenant trigger — what reopens this ADR
This ADR's decisions are sufficient for **single-tenant operation only**. Any of the following events reopens the ADR and requires the multi-tenant SMT decision to be made before the change ships:

- A second tenant (any actor whose code is not mutually-trusted with the existing tenant) joins the host.
- The orchestrator begins accepting workload requests from a public API.
- The host moves from a developer machine to any shared environment.

When reopened, the decision is between (A) host-`nosmt` baseline and (B) sibling-aware pinning; both options are *already supported* by the allocator interface from decision #1.

## Consequences

**Positive:**
- POC ships with `jailer` and the default Firecracker seccomp filter — strong isolation baseline at near-zero engineering cost.
- The cpuset allocator's trait-based design means SMT-correctness arrives as a drop-in implementation, not a refactor.
- The multi-tenant trigger is explicit, so the SMT decision cannot be silently skipped.

**Negative:**
- The POC orchestrator process itself is under-sandboxed (acceptable for developer-machine use, not for anything beyond).
- The single-tenant assumption is load-bearing — running this configuration with a second tenant present is a security regression, not a degraded mode.

## Related
- `docs/003-expert-review.md` §1.4, §2.1, §4.1
- `control-plane/specs/006-firecracker-vmm-api.md` (SMT host-policy note + jailer integration)
- `constraints/000-least-privilege.md`
- `constraints/001-strict-isolation.md`
