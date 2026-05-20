# Spec-007: eBPF Networking & Stateless NAT

## Overview
This specification details the implementation of zero-shared-state, bridge-less networking for the Microvisor using eBPF `tc` (Traffic Control) hooks on the host-side TAP interfaces.

## Architecture
- **Guest IP:** Always `169.254.1.2` (Static).
- **Host TAP IP:** Always `169.254.1.1` (Static gateway for the guest).
- **Host Routable IP:** Dynamically assigned from the IPAM pool (e.g., `10.0.0.X`).
- **Data Path:** `Guest -> TAP -> eBPF NAT -> Host Eth0 -> Internet`.

## eBPF Program Design

### 1. BPF Maps
- **`vm_config_map` (Type: `BPF_MAP_TYPE_ARRAY`):**
    - Key: `0` (Only one entry per TAP interface eBPF program).
    - Value: `struct { u32 host_ip; u32 guest_ip; u8 mac[6]; }`.
    - Purpose: Stores the mapping for the NAT rewrite.

### 2. Egress Hook (`tc_egress` on TAP)
Triggered when a packet leaves the Guest and enters the Host.
- **Action:**
    1. Drop the packet if the destination IP is `169.254.169.254/32` (cloud instance-metadata endpoint). This prevents SSRF into a host-resident metadata service — see Security Guardrails below.
    2. Check if the source IP is `169.254.1.2`; drop otherwise (anti-spoof, see Security Guardrails).
    3. Replace source IP with `host_ip` from the map.
    4. Update the L3 (IP) header checksum incrementally via `bpf_l3_csum_replace`.
    5. **Update the L4 (TCP/UDP) checksum** via `bpf_l4_csum_replace` with the `BPF_F_PSEUDO_HDR` flag (or compute the diff manually with `bpf_csum_diff`). TCP and UDP checksums incorporate a **pseudo-header containing the source and destination IPs**, so an IP rewrite *requires* updating L4 even when no L4 bytes change. Skipping this works on NICs with checksum offload masking the bug, then breaks silently on NICs with offload disabled.
    6. For ICMP: rewrite the source IP and update the ICMP checksum. For ICMP error messages (Type 3 / Type 11), additionally rewrite the **inner IP header** carried in the ICMP payload — otherwise Path MTU Discovery and traceroute responses break.

### 3. Ingress Hook (`tc_ingress` on TAP)
Triggered when a packet is destined for the Guest from the Host.
- **Action:**
    1. Check if the destination IP is the allocated `host_ip`.
    2. Replace destination IP with `169.254.1.2`.
    3. Update L3 checksum (`bpf_l3_csum_replace`).
    4. Update L4 checksum (`bpf_l4_csum_replace` with `BPF_F_PSEUDO_HDR`) to reflect the changed destination IP in the pseudo-header.
    5. For inbound ICMP errors, rewrite the inner IP header to match the guest's view (`169.254.1.2`).

### 4. MTU and Fragmentation
The host TAP MTU must be set to match the host's egress NIC MTU (typically 1500). After source-IP rewrite the packet size is unchanged, so no fragmentation is introduced; but if the guest sends DF=1 packets larger than the egress path's effective MTU, the eBPF program must emit a "Fragmentation Needed" ICMP back to the guest. The simplest path is to lower the guest's MTU to (host_mtu − 0) via boot-arg `mtu=` or DHCP-less static config, since no encapsulation is added.

## Tooling & Integration
- **Loading:** Use `libbpf-rs` or `aya` to load the compiled eBPF object file.
- **Attachment:** Attach the program to the `clsact` qdisc of the TAP device.
- **Correlation:** The `session_id` should be injected into the map for observability (linking the data path to the orchestrator).

## Security Guardrails
- **Anti-Spoofing:** Drops any packet from the TAP that does not have the source IP `169.254.1.2`.
- **Isolation:** No packets are routed between TAP interfaces; all traffic must go through the host-level NAT.

## Build Toolchain & Host Prerequisites

### Host Prerequisites

Two tools must be installed on the developer's machine before `cargo xtask build-ebpf` will
succeed. Neither is a Cargo dependency — they are host-level build tools.

#### 1. Rust nightly toolchain + `rust-src`

```bash
rustup toolchain install nightly-2026-05-19
rustup component add rust-src --toolchain nightly-2026-05-19
```

`bpfel-unknown-none` is a tier-3 target with no pre-compiled `core` artifacts. Cargo must
compile `core` from source via `-Z build-std=core`, which requires (a) a nightly toolchain
and (b) the `rust-src` component so the source is available locally. The orchestrator
(`control-plane`) is unaffected — it builds on stable.

The pinned date is recorded in `microvisor-ebpf/rust-toolchain.toml`; see the upgrade
procedure below.

#### 2. `bpf-linker`

```bash
cargo install bpf-linker
```

The standard system linker (`ld`/`lld`) cannot produce valid BPF ELF objects. BPF requires
specific handling of relocations, section naming conventions, and BTF metadata that
general-purpose linkers do not implement. `bpf-linker` is a thin Rust wrapper around LLVM's
BPF backend that fills this role. It is installed once per developer machine and is not a
per-project Cargo dependency — the same pattern as `protoc` for prot/tonic projects.

`bpf-linker` bundles LLVM internally; the `cargo install` step is slow (it compiles LLVM)
but only needs to run once. Pre-built binaries are not reliably available, so source install
is the canonical path.

### Nightly Toolchain Pin

The `microvisor-ebpf` crate pins a specific nightly date in `microvisor-ebpf/rust-toolchain.toml`.

**Why pinned, not rolling nightly.** A rolling `channel = "nightly"` updates daily and can
silently break the build when aya-ebpf or the compiler changes. Pinning to a specific date
makes the build reproducible and immune to churn until the pin is deliberately advanced.

**Why the xtask unsets `RUSTUP_TOOLCHAIN`.** When `cargo xtask` runs, the parent stable
cargo process sets `RUSTUP_TOOLCHAIN=stable` in the environment. This would override the
`rust-toolchain.toml` file. The xtask explicitly calls `.env_remove("RUSTUP_TOOLCHAIN")`
before invoking the eBPF build so rustup falls back to the directory-level toolchain file.

**Upgrade procedure.** To advance the pin:
1. Pick a target date (ideally a date known to work with the current `aya-ebpf` version).
2. Update `channel` in `microvisor-ebpf/rust-toolchain.toml`.
3. Run `rustup toolchain install <new-date>` and `rustup component add rust-src --toolchain <new-date>`.
4. Run `cargo xtask build-ebpf` and confirm the build is green.
5. Commit the `rust-toolchain.toml` change alongside any aya-ebpf version bumps that required it.

## Related Documents
- `specs/002-networking-ebpf.md`
- `ADRs/004-ebpf-kernel-tracing.md`
