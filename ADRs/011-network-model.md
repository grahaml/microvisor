# ADR 011: Network Model

## Status
Accepted (v1 / POC scope). v2 trigger for `vhost-net` and config-shape decisions are explicit below.

## Context
The network data path is one of the three load-bearing architectural choices in Microvisor (alongside compute and storage). It directly determines:

- Inter-VM isolation strength (Constraint-001).
- Tenant observability granularity (Constraint-008, ADR 004).
- Per-VM rate-limiting and traffic-shaping capability.
- Per-packet overhead and density ceiling.

The spec set commits to **per-VM TAP + eBPF stateless 1:1 NAT** across `control-plane/specs/002-networking-ebpf.md` and `control-plane/specs/007-ebpf-networking-implementation.md`. The SME review (`docs/004-expert-review.md`) found this design fundamentally sound but flagged three implementation gaps — pseudo-header L4 checksums (§1.6), `169.254.0.0/16` foot-guns (§1.10), and the absence of `vhost-net` mention (§2.5). The first two are now addressed in Spec-007's edits; the third is a v2 migration concern. This ADR exists to record the architectural choice, what we rejected and why, and the conditions under which we revisit.

## Alternatives Considered

| Model | Pros | Cons | Verdict |
|---|---|---|---|
| **Host bridge + iptables MASQUERADE** (the bash prototype) | Simple, well-understood, ships with every Linux distro. | All VMs share an L2 broadcast domain → ARP spoofing, broadcast amplification, cross-VM visibility. Directly violates Constraint-001 "no lateral movement." | Rejected — security-incompatible with the project's isolation goals. |
| **macvtap (kernel L2 fast path)** | Faster than software bridge (avoids the bridge code path); each VM gets its own MAC on the host LAN; mature kernel support. | Each VM is visible on the upstream LAN — requires the upstream switch to accept multiple MACs per port; exposes VMs to L2 broadcast traffic; conflicts with anti-spoofing policy at the eBPF layer; doesn't naturally support per-tenant NAT. | Rejected — wrong layer abstraction for our isolation model. |
| **vhost-user-net (DPDK / SR-IOV-class userspace fast path)** | Highest possible throughput; bypass kernel networking stack entirely; suitable for line-rate (40+ Gbps) density. | Significant complexity — DPDK environment, hugepages dedicated to networking, poll-mode driver setup; per-VM userspace socket plumbing; not needed below very high density. | Rejected for v1; named as the v3 substrate if `vhost-net` (the v2 substrate) also runs out of headroom. |
| **vhost-net (in-kernel TAP fast path)** | 2–5× higher throughput than default virtio-net; supported transparently by Firecracker; zero-copy in the kernel TAP path. | Adds a kernel module dependency and a config flag; default virtio-net is plenty for the POC. | **Named as the v2 substrate.** Same per-VM TAP abstraction, just a faster backend — drop-in upgrade when throughput becomes the binding constraint. |
| **Per-VM TAP + eBPF stateless NAT** (chosen) | No shared L2; no host bridge; per-tenant observability via eBPF map; per-tenant rate-limiting available as a future extension via eBPF token bucket; deterministic guest network layout (no DHCP). | Requires careful pseudo-header L4 checksum handling; requires explicit ICMP and inner-IP rewrite for PMTUD; requires anti-spoof matching on TAP interface (not source IP). All addressed in Spec-007. | **v1** |

## Decisions

### 1. Data path (v1): per-VM TAP + eBPF stateless 1:1 NAT
Every VM gets a dedicated host-side TAP device. The guest is configured with a fixed link-local IP (`169.254.1.2`) and points its default route at `169.254.1.1` (the host TAP). An eBPF program attached to the TAP's `clsact` qdisc performs stateless 1:1 SNAT on egress (`169.254.1.2` → allocated host-routable IP) and DNAT on ingress, with the mapping stored in a `BPF_MAP_TYPE_ARRAY` keyed by the TAP. Spec-007 specifies the egress / ingress program logic, including the v0.1 guardrails added during the review:

- Pseudo-header-aware L4 checksum updates (`bpf_l4_csum_replace` with `BPF_F_PSEUDO_HDR`).
- ICMP rewrite, including inner IP header for Type 3 / Type 11 messages, so PMTUD and traceroute work.
- Explicit drop rule for `dst=169.254.169.254/32` to prevent cloud-metadata SSRF from a guest.
- Anti-spoof rule matches on the TAP interface, not the source IP — multiple guests legitimately share `169.254.1.2`.
- Host TAP MTU is set to match host egress MTU (no encapsulation added; no fragmentation introduced).

### 2. IPAM (v1): lock-free atomic-bitset allocator with reconciliation
Host-routable per-VM IPs are allocated from a configured range via a lock-free atomic bitset (a vector of `AtomicU64` with CAS-based allocation; the `AtomicU64` in the spec is shorthand for the per-word allocator, not the entire space). Allocation is contention-free; reclamation on abnormal VM exit is handled by the reconciliation loop committed to in ADR 012 Phase C, which cross-references the bitset against the cgroup tree and TAP device list.

### 3. Host-routable IP range is configuration, not a constant
The current spec text uses `10.0.0.X` as an example. That range collides with most home and corporate networks, so it must not be hardcoded. **Decision:** the host-routable IP allocation range is a required orchestrator configuration parameter (CIDR), validated at startup against the host's existing routing table to refuse a range that overlaps with an active route. Default for the POC is a non-conflicting RFC1918 slice picked at startup based on what's *not* already in use.

### 4. Guest-link range (`169.254.0.0/16` vs CGNAT): keep `169.254.0.0/16` for v1
The link-local choice has known foot-guns (host IPv4LL collisions with `systemd-networkd` / NetworkManager fallback) covered in `docs/004-expert-review.md` §1.10. Two of the three concerns are already mitigated in Spec-007 (drop rule + TAP-iface anti-spoof). The remaining concern (host-NIC IPv4LL collision) is a low-probability event on a developer machine. **Decision:** stay on `169.254.1.2` / `169.254.1.1` for v1. The CGNAT (`100.64.0.0/10`) migration is tracked in `docs/005-deferred-from-review.md` (item D-6) with the trigger noted there.

### 5. Per-VM rate limiting (deferred to v2)
A token-bucket rate limiter in the same eBPF program (using `BPF_MAP_TYPE_PERCPU_ARRAY` for the bucket state) gives per-tenant traffic shaping without an extra hop. This is recommended by `docs/003-microvm-improvement-opportunities.md` §4 and the SME review §2.5 implicitly. **Decision:** not in v1. The POC runs without per-VM rate limits; the first multi-tenant deployment must add this before going live. Tracked as a v2 deliverable in this ADR rather than the deferred-list, because it's a known multi-tenant prerequisite, not an optional optimization.

### 6. v2 migration triggers
This ADR's data-path choice is sufficient up to the following thresholds. Any one reopens the ADR:

- **Throughput:** Aggregate egress sustained > 1 Gbps **or** per-VM packets-per-second > 100 K. Trigger: evaluate `vhost-net` (kernel-side zero-copy TAP path; same eBPF NAT keeps working; this is a substrate change, not a model change).
- **Density × throughput:** Aggregate egress > 10 Gbps. Trigger: evaluate `vhost-user-net` / DPDK-class userspace fast path; this is a real architecture change.
- **Multi-tenancy:** First time a second mutually-untrusted tenant joins the host. Trigger: per-VM rate limiting (decision #5) ships before the change.

## Consequences

**Positive:**
- Strong inter-VM isolation by construction (no shared L2; eBPF anti-spoof on the TAP interface).
- Deterministic guest network layout — no DHCP, no service discovery for the guest's view.
- eBPF map gives every packet a `session_id` correlation point for free (ADR 003).
- v2 throughput upgrade (`vhost-net`) is a substrate swap, not an architecture change.

**Negative:**
- The eBPF NAT program is the load-bearing component for *every* packet — bugs are expensive. The Spec-007 guardrails are not optional.
- Per-VM rate limiting is a known multi-tenant blocker; we ship without it for the POC and must add it before any non-trusted second tenant.
- Host-routable IP range as configuration adds an orchestrator-startup validation step (and a foot-gun if misconfigured).

## Related
- `docs/004-expert-review.md` §1.6, §1.10, §2.5
- `docs/005-deferred-from-review.md` D-1 (`vhost-net`), D-6 (CGNAT migration)
- `docs/003-microvm-improvement-opportunities.md` §4 (token-bucket rate limiting in eBPF), §5 (IP reclamation)
- `control-plane/specs/002-networking-ebpf.md`
- `control-plane/specs/007-ebpf-networking-implementation.md`
- `ADRs/012-crash-recovery-contract.md` Phase C (IPAM reconciliation)
- `archive/specs/001-networking.md` (legacy bash networking — read-only history)
