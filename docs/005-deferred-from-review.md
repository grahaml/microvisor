# Deferred from Expert Review

This document tracks items raised in `docs/004-expert-review.md` (and side-questions raised when reviewing its recommendations) that have been **explicitly deferred** rather than addressed in the current spec set. Each entry names what it is, why we deferred it, and what re-evaluation trigger should reopen it.

Deferred is not the same as "ignored." These items are intentional commitments to *not* do something now, with a documented condition under which we will.

---

## D-1. `vhost-net` for the network fast path

**What it is.** Firecracker's default virtio-net implementation copies packets via the VMM thread in userspace. `vhost-net` is a kernel-side TAP fast path supported by Firecracker that delivers 2–5× higher throughput by eliminating the userspace copy.

**Why deferred.** At POC density (1–2 VMs) the default virtio-net is plentiful. The default path is simpler to debug and doesn't require additional kernel module configuration.

**Re-evaluation trigger.** Aggregate egress > 1 Gbps sustained, or per-VM packets-per-second exceeding ~100 K. Whichever comes first.

**Related.** Review §2.5. ADR 011 lists this as a v2 substrate.

---

## D-2. KSM (Kernel Same-page Merging) for memory deduplication

**What it is.** KSM scans guest memory pages and merges identical pages across VMs, giving 5–10× memory over-commit when many VMs share an identical binary footprint (e.g., the same Chromium build).

**Why deferred.** Irrelevant below ~10 VMs running the same image. The orchestrator can enable KSM on the guest memory regions at any time without spec changes — it's a runtime tuning question, not an architectural one.

**Re-evaluation trigger.** ≥ 10 concurrent VMs sharing the same base rootfs **and** memory headroom becoming a density bottleneck.

**Tuning hint.** When enabled, drive `pages_to_scan` high during pool warm-up (maximum dedup window) and throttle back once VMs leave the pool. Exclude V8 JIT writable+executable regions via `madvise(MADV_UNMERGEABLE)` — they churn and never merge, just waste CPU.

**Related.** Review §2.7. `docs/003-microvm-improvement-opportunities.md` §2 has the prior thinking.

---

## D-3. Transparent Huge Pages (THP) for guest backing memory

**What it is.** Backing guest RAM with 2 MB pages instead of 4 KB pages reduces TLB pressure and gives ~5–15% steady-state perf improvement for memory-heavy workloads (browsers fall in this bucket).

**Why deferred.** Easy to enable later (`madvise(MADV_HUGEPAGE)` on the guest memory region after `mmap`); doesn't require spec changes. Better to bench it as a tuning lever once we have a baseline than to commit to it upfront.

**Re-evaluation trigger.** First end-to-end performance benchmark of a Rust-control-plane-managed VM. Try with/without THP; commit the winning configuration.

**Related.** Review §2.6.

---

## D-4. Admission control / quota

**What it is.** A check that runs *before* a VM-launch request enters the state machine, consulting host capacity (free CPUs, free memory, free thin-pool blocks, free IPs) and refusing requests that can't fit.

**Why deferred.** At 1–2 VMs there's nothing to admit-control against. Adding this prematurely would force decisions on quota semantics (per-tenant? per-host? burst vs. sustained?) that we don't have enough usage data to make.

**Re-evaluation trigger.** First time we have ≥ 3 concurrent VMs **and** the orchestrator is accepting requests from anything other than a single developer's shell.

**Related.** Review §2.3.

---

## D-5. `session_id` widening to u128

**What it is.** The `session_id` is currently a `u64` chosen randomly. Birthday-paradox collision risk becomes meaningful at ~2³² IDs — still way more than a single host will see. If we ever **cross-host correlate** sessions (e.g., a fleet-wide tracing aggregator), the safer choice is `u128` or a host-namespaced ID scheme.

**Why deferred.** Single-host POC; no cross-host correlation. `u64` is the right call today.

**Re-evaluation trigger.** First feature that aggregates sessions across multiple Microvisor hosts (e.g., a centralized tracing backend that ingests OTLP from multiple control planes).

**Related.** Review §1.11. ADR 003 documents the current u64 choice.

---

## D-6. CGNAT range (`100.64.0.0/10`) vs. link-local `169.254.0.0/16` for the guest network

**What it is.** The current design (Spec-002, Spec-007) hardcodes the guest IP as `169.254.1.2` and the host-side TAP as `169.254.1.1`. Link-local space is convenient but has three known foot-guns (reviewed in `docs/004-expert-review.md` §1.10):

- `169.254.169.254` is the cloud instance-metadata endpoint (SSRF risk). Already mitigated by the eBPF drop rule added to Spec-007.
- `systemd-networkd` and `NetworkManager` auto-assign IPs from this range on IPv4LL fallback, causing potential host-NIC collisions.
- Match-by-source-IP anti-spoof rules can hairpin if multiple guests share the same source IP. Already mitigated by matching on the TAP interface in Spec-007.

CGNAT space (`100.64.0.0/10`) is what Firecracker's own examples use and has none of these issues.

**Why deferred.** Two of the three concerns are already addressed in spec. The third (host-NIC IPv4LL collision) is a low-probability event on a developer machine. Migrating off `169.254.x.x` would touch the eBPF map layout, the guest init script, and every reference in the spec set — not free.

**Re-evaluation trigger.** First time we hit an actual `169.254.x.x` collision in development, **or** the first non-developer-laptop host (e.g., a server with `systemd-networkd` configured to manage all interfaces).

**Related.** Review §1.10. ADR 011 holds the open decision.

---

## How to Process This List

When picking up an item:

1. Move its entry **out of this file** into either a new ADR or a spec change.
2. Update the section in `docs/004-expert-review.md` that originally raised it to point at the new doc.
3. Don't delete from `docs/004-expert-review.md` itself — that doc is a snapshot of the review and should remain as written.

New items should be added here whenever a future review surfaces something that we consciously decide to defer.
