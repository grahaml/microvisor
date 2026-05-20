// Shared types between the eBPF kernel programs (no_std) and the userspace loader (std).
// The `userspace` feature gates anything that can't compile in a BPF context.
#![cfg_attr(not(feature = "userspace"), no_std)]

/// Per-VM NAT mapping written by the orchestrator and read by the eBPF programs.
///
/// One instance lives in each TAP device's `vm_config_map` (BPF_MAP_TYPE_ARRAY, key=0).
/// Layout is #[repr(C)] so it is identical in BPF and Rust userspace.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VmConfig {
    /// Host-routable IP assigned by IPAM (network byte order).
    pub host_ip: u32,
    /// Guest link-local IP (always 169.254.1.2 = 0xFEA90102, network byte order).
    pub guest_ip: u32,
    /// Guest MAC address (informational; used for future ARP stub).
    pub mac: [u8; 6],
    /// Explicit padding to reach a 12-byte natural size (4+4+6=14 → pad to 16).
    pub _pad: [u8; 2],
}

/// aya requires types stored in BPF maps to implement Pod (plain-old-data: no
/// uninitialized bytes, no pointers, safe to zero-init). VmConfig satisfies all
/// three: repr(C), explicit padding, Default zeros every byte.
#[cfg(feature = "userspace")]
unsafe impl aya::Pod for VmConfig {}
