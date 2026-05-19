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

## Related Documents
- `specs/002-networking-ebpf.md`
- `ADRs/004-ebpf-kernel-tracing.md`
