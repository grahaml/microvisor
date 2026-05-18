# Spec-002: Stateless Multi-Tenant Networking

## Overview
Implements zero-shared-state networking using Point-to-Point TAP devices and eBPF-based NAT. Avoids host software bridges.

## Constraints
- **Zero-Shared-State:** No host bridges or global ARP tables.
- **Stateless NAT:** 1:1 NAT executed in the data-path via eBPF.
- **Immutable Guest Layout:** Guest uses a static IP (`169.254.1.2`) and MAC.
- **Lock-Free IPAM:** Use atomic bitsets to ensure zero mutex contention during IP allocation.

## Implementation Details
1. **TAP Provisioning:** Create a unique TAP device for every VM.
2. **IPAM:** A lock-free atomic bitset manages the allocation of host-routable `/32` IPs.
3. **eBPF Injection:**
    - Load a `tc` classifier eBPF program on the host-side TAP interface.
    - Update a `BPF_MAP_TYPE_ARRAY` with the allocated host-routable IP.
4. **Packet Rewriting:** eBPF intercepts `tc_egress`/`tc_ingress` to swap `169.254.1.2` with the external IP, updating L3 checksums incrementally.

## Integration
- Configured by **Spec-000** during VM initialization.
