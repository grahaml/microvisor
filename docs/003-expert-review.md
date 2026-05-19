# Microvisor — Expert SME Review

**Reviewer scope:** Linux kernel architecture, KVM/Firecracker, virtual memory, eBPF, low-latency Rust on tokio.
**Reviewed:** `docs/000-002`, all of `constraints/`, all of `control-plane/specs/`, all of `specs/`, all of `ADRs/`. Skipped: `docs/003-microvm-improvement-opportunities.md` per request.
**Severity scale:** 🔴 blocker / wrong · 🟠 significant / will bite you · 🟡 imprecise but recoverable · 🟢 nit · 💡 opportunity.

---

## 0. Executive Summary

The architectural direction is sound: 1:1 vCPU pinning, dm-thin CoW, eBPF stateless NAT, KVM exit tracing, hierarchical state machine — all of these are legitimate building blocks of a high-density Firecracker control plane. The vision is internally coherent.

However, the docs have ~6 outright technical errors, ~10 significant gaps, and a structural bias problem: **five of eight ADRs are about observability, zero are about security boundaries, storage, or networking.** For a system whose entire pitch is "hardware-enforced multi-tenant isolation," that is the wrong center of gravity.

For the immediate POC (1–2 VMs via the Rust control plane), the docs are **executable but not safe to copy-paste**. The dm-thin ioctl sequence in `008-storage-ioctl-implementation.md` is incomplete and will not actually produce a snapshot if implemented literally. The `isolcpus` example assumes a 16-core machine. The `169.254.1.2`/`169.254.1.1` link-local choice has subtle conflicts. These are fixable.

---

## 1. Hard Technical Errors (🔴 / 🟠)

These will produce wrong behavior if implemented as written.

### 1.1 🔴 dm-thin ioctl sequence is incomplete (Spec-008)

`control-plane/specs/008-storage-ioctl-implementation.md` describes:

> `DM_DEV_CREATE` → `DM_TABLE_LOAD` (target=thin, args: pool_dev, internal_id) → `DM_DEV_SUSPEND` to activate

This is missing a critical step. A `thin` target with a fresh `internal_id` references a thin device that **does not yet exist inside the pool**. You must first send a `thin_pool` message to the pool device to create or snap the thin device:

```
ioctl(fd, DM_TARGET_MSG, "create_snap <new_dev_id> <origin_dev_id>")
```

The full correct sequence is:

1. `DM_TARGET_MSG` on the pool → `create_snap <new_id> <origin_id>` (registers a snapshot inside the pool's metadata)
2. `DM_DEV_CREATE` → create the device-mapper device node (`/dev/mapper/vm-<id>-rootfs`)
3. `DM_TABLE_LOAD` → bind the dm device to the `thin` target with `<pool_dev> <new_id>`
4. `DM_DEV_SUSPEND` with flag `0` → "resume" (activate) the device

The current text also conflates "DM_DEV_SUSPEND" with "Resume/Activate" without explaining that DM_DEV_SUSPEND is the same ioctl for both, distinguished by the `DM_SUSPEND_FLAG` bit. Production reference: `dmsetup` source (`drivers/md/dm-ioctl.c` in the kernel, `lib/ioctl.c` in the `device-mapper` userland library).

**Example in production:** systemd-machined, libvirt's `storage_driver_lvm`, and the lvm2 toolchain all go through `liblvm2cmd`/`libdevmapper` rather than open-coded ioctls precisely because the sequence is fiddly. Recommend either using the `devicemapper` crate (Rust, wraps `libdevmapper`) for the POC and only dropping to raw ioctls in a v2 hot path, or vendoring the proven sequence from `libdevmapper`'s `dm_task_create + dm_task_set_message + dm_task_run` flow.

### 1.2 🔴 `entropy_avail >= 3000` validation is outdated (Constraint-006)

> **Guest Check:** Run `cat /proc/sys/kernel/random/entropy_avail` inside the guest. It should consistently show values above 3000.

Since Linux 5.18 (Jason Donenfeld's overhaul of the RNG subsystem in 2022), `entropy_avail` no longer represents a bit-pool depth. It's capped at 256 and is effectively useless as a validation signal. On any modern kernel (which is the only kind running Firecracker), the guest will *always* report something close to 256, and that is fine — the RNG is initialized and non-blocking once seeded.

**Better validation:**
- Read `/proc/sys/kernel/random/poolsize` (constant) and `/proc/sys/kernel/random/urandom_min_reseed_secs`.
- Check the kernel log for `random: crng init done` early in boot. Firecracker + virtio-rng + a properly seeded host will hit this in a few ms.
- For TLS-handshake-latency validation (your real intent), just measure handshake p99 directly. That's the metric that matters.

### 1.3 🔴 `io_uring` does not bypass the page cache (Spec-008, TAD §3.2)

> Firecracker opens the block device with `O_DIRECT`, `io_uring` queues bypass host filesystem page cache.

`io_uring` is an asynchronous submission/completion ring. It is orthogonal to caching. `O_DIRECT` is what bypasses the page cache. `io_uring` can be used with or without `O_DIRECT`. Reword to:

> Firecracker opens the block device with `O_DIRECT` (bypasses host page cache) and submits requests via `io_uring` (async, low-syscall-overhead I/O path).

This is a minor wording fix but matters because it suggests a misunderstanding of what each primitive does.

### 1.4 🔴 SMT security claim is half-correct (Spec-006)

> `smt: false` (Always disable SMT inside the guest for security)

Firecracker's `smt` flag controls whether SMT is advertised to the **guest**. It does *not* prevent the host from co-scheduling two unrelated VMs onto two SMT siblings of the same physical core. That is the actual MDS / L1TF / Spectre-v2 cross-VM leak vector.

If you want real SMT security across tenants, you need one of:

1. `nosmt` on the host kernel command line (gives up half your throughput but is bulletproof). What public clouds running multi-tenant Firecracker do for hostile workloads.
2. `cpuset` pinning that *always* pins both SMT siblings of any physical core to the same VM (i.e., never split a core across tenants). This is the AWS Nitro approach. The spec doesn't articulate this; it just says "1:1 vCPU to pthread."
3. Disable `mitigations` selectively — but you've already opted into the strict-isolation constraint, so no.

**Production reference:** Firecracker's own threat model docs explicitly call this out (`firecracker/docs/threat_model.md` — "to mitigate SMT attacks, customers should disable hyperthreading on the host or co-locate siblings"). Microvisor's spec is silent on it.

Severity: 🟠 for the POC (single-tenant), 🔴 for production multi-tenancy.

### 1.5 🟠 "Microsecond-level boot times" (PRD)

PRD §1: *"microsecond-level boot times"*. Firecracker cold boot is ~125 ms. Snapshot resume is ~3–5 ms. There is no path to microseconds — three orders of magnitude off. Probably a typo for "millisecond." Same hyperbole shows up as "Sub-Second Boot Time" + "Nano-Second Latency" in `constraints/007`. Nanosecond latency is hardware physics, not a software goal. Recommend tightening these to defensible numbers (e.g., "p99 cold boot under 200 ms; snapshot resume under 10 ms; vCPU KVM_RUN re-entry under 5 µs").

### 1.6 🟠 `bpf_l3_csum_replace` / `bpf_l4_csum_replace` need pseudo-header handling (Spec-007)

The eBPF NAT spec says:

> Update L3 (IP) and L4 (TCP/UDP) checksums incrementally (`bpf_l3_csum_replace`, `bpf_l4_csum_replace`).

These helpers do work — but TCP/UDP checksums include a **pseudo-header** that incorporates source and destination IPs. When you rewrite the source IP, you must update L4 even if you didn't touch the L4 payload at all. The verbatim spec implies this but doesn't make the dependency explicit. Implementers regularly miss this and ship NAT that "works" until they hit a checksum-offload-disabled NIC and packets get silently dropped.

**Also missing:** ICMP and ICMP-inside-ICMP rewrite (path MTU discovery breaks otherwise). And the spec is silent on fragmentation — a packet from the guest larger than the host's egress MTU after rewrite will be dropped unless you handle PMTUD or set the TAP MTU correctly.

### 1.7 🟠 Single-NUMA assumption is not handled (Spec-000, Spec-003)

Spec-003 says "Configure `cpuset.mems` in the VM's cgroup to match the physical CPU socket." On a single-NUMA-node host, `cpuset.mems` must be `0`, full stop. The spec needs explicit logic:

```
if num_numa_nodes == 1:
    cpuset.mems = "0"
    skip remote-NUMA observability  # no remote accesses possible
else:
    pin to NUMA node of assigned cpuset.cpus
```

This is **not just a POC concern** — the orchestrator hardcoding "pick a NUMA node based on CPU" without checking the topology will panic / misconfigure on any single-socket production server too (these are very common — Threadrippers, EPYC-1S, single-socket Xeons). Read NUMA topology from `/sys/devices/system/node/` at startup; treat `num_nodes == 1` as a valid case.

**Production reference:** libvirt's `virCapsHostNUMA` and Kubernetes' Topology Manager both read `/sys/devices/system/node/` and degrade gracefully when there's one node.

### 1.8 🟠 `isolcpus=2-15` is hardcoded (TAD §1.1)

The example assumes 16 cores. That's fine *as an example*, but the doc reads as a recipe. Modern guidance is:

- `isolcpus` is technically deprecated in favor of cgroups v2 `cpuset.cpus.partition=root`. It still works but kernel devs prefer the cgroup path.
- You also need `irqaffinity=0-1` (or write to `/proc/irq/*/smp_affinity`) — otherwise NIC and storage IRQs land on isolated cores anyway and shred your latency budget.
- `kthread_cpus=0-1` (or `tuna`/`cset` to evacuate kernel threads) to push kworker, ksoftirqd, etc. off the isolated cores.

**Production reference:** Cloud Native real-time tuning guides from Red Hat (`tuned-adm profile realtime-virtual-host`) and Kubernetes' CPU Manager use a combination of all of the above. Recommend the spec call out the *full* boot-time + runtime tuning set, not just `isolcpus`.

### 1.9 🟠 Tokio + blocking syscalls — detection ≠ prevention (Spec-005)

> Any blocking operation exceeding 1ms must be logged as a warning to detect tokio worker thread starvation.

By the time you've logged a warning, you've already starved your reactor. Two changes:

1. **Mandate** `spawn_blocking` or `block_in_place` for known-blocking calls (dm ioctls, cgroup writes, sched_setaffinity). Don't just instrument them — wrap them.
2. Set the tokio runtime to fail-loud on starvation: enable `tokio::runtime::Builder::enable_metrics_poll_count_histogram()` and alert on slow-poll counts, not just per-call latency.

The 1ms threshold is reasonable as a guardrail metric. As the *primary* defense, it's too late.

### 1.10 🟠 Static `169.254.1.2` guest IP — three concrete hazards

The link-local choice is clever (no IPAM concerns inside the guest) but introduces foot-guns:

1. **`169.254.169.254` is the AWS/GCE/Azure instance metadata endpoint.** Browsers in the guest will sometimes probe it. The default route inside the guest is `169.254.1.1`, so the packet goes to the host TAP. If the host has a real cloud-metadata service running (rare on a workstation, common on a cloud VM running Microvisor), the guest can SSRF into it. Spec needs an explicit "drop dst=169.254.169.254 in eBPF" rule.
2. **Systemd-networkd and NetworkManager treat 169.254.0.0/16 specially.** They auto-assign IPs in that range as a fallback. If the host's wlan0/eth0 ever IPv4LL-fails, you can get a collision. Spec should add `LinkLocalAddressing=no` on the relevant host interfaces or use a non-link-local /30 (e.g., from 100.64.0.0/10 CGNAT space, which is what Firecracker's own examples use).
3. **The same `169.254.1.2` for every guest** is correct by design, but combined with `nftables`/`iptables` rules elsewhere in the stack that may try to match by source IP, you can get surprising hairpinning. Spec should mention that anti-spoof rules must match by **TAP interface**, not source IP.

### 1.11 🟡 `session_id` framing (ADR 003 / Constraint 008)

> 64-bit `session_id`, "18 quintillion IDs, never run out during host lifetime"

This is technically right but oddly framed. If you're picking IDs randomly (rather than via a counter), the bound is the **birthday paradox**, not the address space. With random u64 IDs, collision probability becomes non-trivial at ~2^32 IDs (~4 billion). That's still way more than any single host's lifetime sessions, so the conclusion is correct — but if you ever cross-host correlate sessions (which the observability stack hints at), you'd want to bump to u128 or namespace by host. Worth a sentence.

---

## 2. Significant Gaps (🟠)

Things the docs simply don't address, which you will hit.

### 2.1 🟠 No seccomp-bpf profile for Firecracker — and no sandbox for the orchestrator

Firecracker ships with a seccomp filter by default (`--seccomp-level 2` is the modern equivalent — strict, 50ish allowed syscalls). The Microvisor spec doesn't mention seccomp at all. If you bypass it (which is easy by accident — Firecracker auto-loads a JSON filter from `/etc/firecracker/seccomp.json` if present), you've undone half of Firecracker's isolation story.

Similarly, the Rust orchestrator runs as a non-root user (per Constraint 000) but otherwise has no sandbox. It holds `/dev/kvm`, `/dev/mapper/control`, and `CAP_BPF` (or `CAP_SYS_ADMIN`). It should run with its own seccomp filter, ideally under a `systemd.exec` unit with `ProtectSystem=strict`, `ProtectHome=true`, `NoNewPrivileges=true`, `RestrictAddressFamilies=AF_UNIX AF_NETLINK`, `MemoryDenyWriteExecute=true`.

**Production reference:** AWS Lambda's Firecracker invokes go through both a jailer (the `jailer` binary that ships with Firecracker — chroots, sets up cgroups, drops caps, applies seccomp before exec'ing the VMM) and an outer Nitro hardware boundary. Microvisor's spec calls for "least privilege" but never invokes the `jailer`, which is a free win.

### 2.2 🟠 No failure-domain analysis

The state machine spec describes happy-path transitions. It doesn't say:

- What happens if the orchestrator crashes while VMs are running? Are they reaped by `prctl(PR_SET_PDEATHSIG)`? Or do they survive as orphans?
- Can a restarted orchestrator adopt running VMs by scanning `/sys/fs/cgroup/orchestrator/`? If not, every restart is a customer-visible outage.
- Partial-provisioning rollback: storage created → network failed → who deletes the thin volume? Spec-005's "Rich Errors" carry the failure state but don't define rollback handlers per state.

For a POC this is fine. For anything beyond, you need a reconciliation loop (kube-controller-runtime-style) and a startup adoption scan.

### 2.3 🟠 No quota / admission control

Nothing in the spec prevents the orchestrator from accepting "launch 500 VMs" requests on a host with capacity for 50. You need an admission controller that consults cgroup pressure, available memory, free thin-pool blocks, and free IPs *before* entering `Pending`.

### 2.4 🟠 Thin-pool exhaustion behavior

dm-thin's default behavior when the pool fills is to **silently hang I/O** on writes that would allocate. Guests just freeze. There's a `--errorwhenfull` mode and `dmeventd` can grow the pool, but the spec doesn't pick a policy. (This is also in `docs/003-improvement-opportunities` which I was told to skip — but it's worth flagging that without an answer here, your first OOM-on-disk incident is silent.)

### 2.5 🟠 No virtio-net fast path

The spec uses Firecracker's default virtio-net (userland packet copies via the VMM thread). For high throughput, `vhost-net` (kernel-side TAP fast path) is 2–5× faster and is supported by Firecracker via the `--api-sock` config. The spec is silent. For the POC, default is fine. For the density goals, this matters.

### 2.6 🟠 No THP / huge-pages strategy for guest backing memory

Transparent Huge Pages (`/sys/kernel/mm/transparent_hugepage/enabled=madvise` + `madvise(MADV_HUGEPAGE)` on the guest memory region) reduces TLB misses dramatically for guest workloads. Firecracker does not enable this by default. Constraint 008 mentions monitoring `khugepaged` events but the spec never *enables* THP. ~5–15% steady-state perf left on the table.

### 2.7 🟠 KSM is mentioned only in the (skipped) improvement doc

For browser-heavy workloads where every guest has an identical Chromium binary in memory, KSM is the single largest density win available — 5–10x over-commit on backing memory. The spec proper doesn't mention it.

### 2.8 🟠 No mention of vsock for guest-host control channel

Firecracker supports vsock. It's how AWS Lambda's runtime talks to the host control plane. If you want any host→guest control path (graceful shutdown, health probes, mid-flight config), vsock is the right primitive — and the spec says nothing about it.

---

## 3. Structural / Framing Issues (🟡)

### 3.1 🟡 ADR coverage is observability-heavy

ADRs 003, 004, 005, 006, 008 are all observability. ADR 002 is ephemeral state. ADRs 001 and 007 are the only non-observability decisions. Missing ADRs that should exist for a multi-tenant isolation product:

- **ADR: Storage architecture (dm-thin vs file-backed vs SPDK)**
- **ADR: Network model (per-VM TAP + eBPF NAT vs macvtap vs vhost-user)**
- **ADR: Multi-tenancy threat model and SMT policy**
- **ADR: Boot-source contract (which kernel, which initramfs, with what minimum configs)**
- **ADR: Crash-recovery contract (PR_SET_PDEATHSIG, adoption-on-restart)**

The current ADR set documents observability decisions extremely well but treats the core isolation/storage/network choices as implicit. They aren't — they're the most consequential decisions in the system.

### 3.2 🟡 Two conflicting network models with no migration story

`specs/001-networking.md` (the bash spec) describes `tap0`, `172.16.0.0/24`, `iptables MASQUERADE`. `control-plane/specs/002-networking-ebpf.md` describes per-VM TAP, `169.254.1.2`, eBPF NAT. Both are correct in isolation. There's no doc that says "we are migrating from A to B; here's the contract guests see." For the POC, since you're moving to the Rust plane and abandoning the bash, just *delete* the bash networking spec or move it to `archive/`.

### 3.3 🟡 Constraint 003 (Declarative Infrastructure) conflicts with Spec-008 (mkfs.ext4 / fallocate)

Spec-008 metadata-drive provisioning says "Inject data using `genext2fs` or by mounting locally if permissions allow." Mounting a guest-visible filesystem **on the host** to inject secrets is the opposite of declarative and a security smell (loop-mount + write + umount + race conditions + needs CAP_SYS_ADMIN). Use `genext2fs` exclusively, or better, build the metadata image from a tar via `mke2fs -d` (supported in e2fsprogs ≥ 1.43, ships everywhere now). No mounts on the host, no `CAP_SYS_ADMIN`, fully declarative.

### 3.4 🟡 "Always-on" observability + "1% regression requires new ADR" is a strong promise

Constraint 008 says "any PR that increases baseline latency by >1% requires a new ADR." That's a great ratchet, but it requires:

1. A baseline that is automatically measured per-PR (CI benchmark, not "I ran it once on my laptop")
2. A delta calculation with confidence intervals
3. An owner who maintains the benchmark

The spec implies all this but doesn't operationalize it. If the team isn't ready to commit eng time to a perf-CI, downgrade the language.

---

## 4. Performance Opportunities (💡)

These are improvements the docs don't mention.

### 4.1 💡 Sibling-aware pinning

On a host with SMT, pinning one VM's vCPU to logical CPU N and another VM's vCPU to the sibling of N (N+core_count typically) means they fight for the same physical execution units. The cgroup spec needs sibling-aware allocation: read `/sys/devices/system/cpu/cpuN/topology/thread_siblings` and either co-pin both siblings to the same VM or refuse to schedule on a busy sibling.

### 4.2 💡 `MADV_DONTNEED` on snapshot teardown

When you destroy a VM, the guest memory region is `munmap`'d but the underlying pages may stick around until the next memory-pressure reclaim. `MADV_DONTNEED` (or `MADV_FREE` on newer kernels) on the region pre-munmap returns the pages to the host immediately — important for fast churn.

### 4.3 💡 `prctl(PR_SET_PDEATHSIG, SIGKILL)` in Firecracker children

If you fork+exec Firecracker, set `PR_SET_PDEATHSIG` so that if the orchestrator dies, the kernel signals all children. Otherwise orphaned VMMs survive indefinitely. Cheap and correct.

### 4.4 💡 Use `clone3` with `CLONE_INTO_CGROUP` instead of fork+write-pid-to-cgroup.procs

Modern kernels (5.7+) let you place a process into a cgroup *atomically at clone time* via `CLONE_INTO_CGROUP`. Eliminates the race window where a process exists outside its intended cgroup and could be unconstrained briefly. Matters for security and for getting clean per-VM accounting from the start.

### 4.5 💡 Pre-allocate Firecracker process pool

The Microvisor spec implies forking Firecracker per VM. Forking is cheap (~5 ms) but you can drive it lower by keeping a warm pool of un-configured Firecracker processes. Aligns with the warm-pool philosophy the team already has for VMs.

### 4.6 💡 `MAP_POPULATE` + `MAP_HUGETLB` for snapshot-resume regions

For the eager-prefault path that Spec-003 describes, `mmap(... MAP_POPULATE | MAP_HUGETLB ...)` is cheaper than `mmap` + `madvise(MADV_WILLNEED)`. Single syscall, kernel does the heavy lifting.

### 4.7 💡 Rate-limit `virtio-rng`

Constraint 006 backs virtio-rng with `/dev/urandom`. That's non-blocking, but a guest can still pull entropy at line rate and waste host CPU. Firecracker's `rate_limiter` config on the entropy device caps it. Recommend ~1 MB/s, which is 1000× more than any real workload needs.

---

## 5. POC Plausibility (1–2 VMs, Rust control plane, target = reasonable production host)

Going through the spec subsystem-by-subsystem with "what's the minimum to make this run":

| Subsystem | POC-ready? | Notes |
|---|---|---|
| **Compute topology (Spec-000)** | ✅ with caveats | `isolcpus` / NUMA must be made optional/configurable. Hardcoding `cpuset.mems=0..` is fine since most hosts will be single-NUMA. |
| **Storage (Spec-001, Spec-008)** | ⚠️ rewrite needed | The dm-thin ioctl sequence in Spec-008 is incomplete (§1.1). For the POC, recommend using the `devicemapper` crate or shelling out to `dmsetup` once, and replacing with raw ioctls later. |
| **Networking (Spec-002, Spec-007)** | ✅ | The eBPF NAT design is fine. Add ICMP handling + pseudo-header note + 169.254.169.254 drop rule before considering it production. For 1–2 VMs, even a single static rule works. |
| **Memory (Spec-003)** | ✅ | NUMA path must handle `nodes == 1`. Eager prefault via `MADV_WILLNEED` is the safer POC default; `userfaultfd` adds complexity not worth it for 1–2 VMs. |
| **Observability (Spec-004, Spec-005)** | ✅ | Heavy spec for a POC. Recommend starting with `tracing` + `tracing-subscriber` + structured stdout; defer eBPF PMU correlation until VM count justifies it. |
| **Firecracker API (Spec-006)** | ✅ | Solid. `hyperlocal` or `firepilot` crate save you writing the UDS HTTP client. Don't forget the `jailer` binary — use it from day one rather than retrofitting. |
| **State machine (Spec-005, ADR 007)** | ✅ | Good fit for the POC. For 1–2 VMs, skip the hierarchical sub-state-machines and just use a single `enum` per VM. Add hierarchy when you actually have ≥2 sub-resources with independent lifecycles. |
| **Security boundaries** | ❌ for production, ⚠️ for POC | No seccomp, no jailer, no SMT policy, no admission control. Document these as known gaps before the first non-team user touches it. |

**Recommended POC sequence (1–2 VMs):**

1. `jailer` + Firecracker boot from a static kernel + ext4 rootfs (no dm-thin yet) — proves the API integration.
2. Add cgroups v2 setup (`cpuset.cpus`, `memory.max`, `memory.swap.max=0`) — proves resource fencing.
3. Add per-VM TAP + simple host iptables NAT (no eBPF yet) — proves networking.
4. Replace iptables NAT with the eBPF program from Spec-007 — proves the eBPF path.
5. Replace ext4-file rootfs with dm-thin snapshot — proves storage.
6. Add `tracing` instrumentation, no eBPF/PMU yet — proves observability.

This sequences risk so that each step builds on a working baseline. The spec implies all six steps as one big bang, which is harder to debug.

---

## 6. Recommended Doc Edits (concrete, smallest changes)

1. **Spec-008**: Replace the ioctl section with the full 4-step `DM_TARGET_MSG → DM_DEV_CREATE → DM_TABLE_LOAD → DM_DEV_SUSPEND(resume)` sequence and call out `DM_SUSPEND_FLAG`. Drop the "mount locally" alternative.
2. **Constraint-006**: Remove `entropy_avail > 3000` check; replace with "verify `crng init done` in dmesg before user-space starts."
3. **Spec-008 / TAD §3.2**: Reword "io_uring bypasses page cache" → "O_DIRECT bypasses page cache; io_uring is the async submission interface."
4. **Spec-006**: Add "host must run with `nosmt` OR vCPU pinning must place SMT siblings of any physical core into the same VM's cpuset" alongside the `smt: false` line.
5. **TAD §1.1**: Add `irqaffinity=` and `kthread_cpus=` to the kernel cmdline example; note `cpuset.cpus.partition=root` as the modern equivalent of `isolcpus`.
6. **Spec-007**: Add ICMP rewrite, pseudo-header note for L4 checksum, and a hard "drop dst 169.254.169.254" rule.
7. **Spec-003**: Add explicit branch for `num_nodes == 1`.
8. **Spec-005**: Mandate `spawn_blocking`/`block_in_place` wrappers around ioctl/cgroup writes; keep the 1ms warning as a guardrail.
9. **PRD / Constraint-007**: Tighten "microsecond boot" / "nanosecond latency" to defensible specific numbers.
10. **New ADRs**: SMT/multi-tenancy threat model · Storage architecture · Network model · Crash-recovery contract.

---

## 7. What Was Done Well

Stuff worth keeping explicit so it doesn't get refactored away:

- The `session_id` correlation primitive is the right design. Threading a single u64 through `tracing` spans, eBPF maps, and PMU samples is what every production observability stack converges on eventually. Starting there is a real advantage.
- ADR 007 (hierarchical state machines) names the right pattern. The boilerplate cost is real but the typestate discipline pays off the moment you have more than one resource lifecycle to manage.
- The Constraint files are unusually disciplined — short, normative, "what we do NOT do" framing is good. Keep that style as the team grows.
- Dogfooding the observability stack as a Tier-0 microVM (ADR 008) is the right kind of architectural risk to take. It forces the control plane to handle its own failure cases for free.

---

## Verification

The review file itself does not need automated verification — it is documentation. However, before considering the review "applied," the following should be done by the team:

- [ ] Open issues/tasks for each 🔴 and 🟠 finding in §1 and §2.
- [ ] Edit the ten target docs per §6.
- [ ] Decide POC sequencing (§5) and update the project roadmap accordingly.
- [ ] Confirm `mke2fs -d` is available in the build environment before deleting the loop-mount path in Spec-008.
