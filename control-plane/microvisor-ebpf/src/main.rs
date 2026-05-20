#![no_std]
#![no_main]

use aya_ebpf::{
    bindings::{TC_ACT_PIPE, TC_ACT_SHOT},
    macros::{classifier, map},
    maps::Array,
    programs::TcContext,
};
use microvisor_common::VmConfig;

// ---------------------------------------------------------------------------
// Packet layout constants (Ethernet II + IPv4, no options assumed)
// ---------------------------------------------------------------------------

const ETH_HDR_LEN: usize = 14;
const IP_HDR_LEN: usize = 20;

// Offsets from start of packet
const ETH_PROTO_OFF: usize = 12; // 2 bytes: EtherType
const IP_PROTO_OFF: usize = ETH_HDR_LEN + 9; // 1 byte: IP protocol
const IP_CSUM_OFF: usize = ETH_HDR_LEN + 10; // 2 bytes: IP header checksum
const IP_SRC_OFF: usize = ETH_HDR_LEN + 12; // 4 bytes: source IP
const IP_DST_OFF: usize = ETH_HDR_LEN + 16; // 4 bytes: destination IP

// L4 checksum field offsets
const TCP_CSUM_OFF: usize = ETH_HDR_LEN + IP_HDR_LEN + 16;
const UDP_CSUM_OFF: usize = ETH_HDR_LEN + IP_HDR_LEN + 6;
const ICMP_CSUM_OFF: usize = ETH_HDR_LEN + IP_HDR_LEN + 2;
const ICMP_TYPE_OFF: usize = ETH_HDR_LEN + IP_HDR_LEN; // 1 byte: ICMP type

// Inner IP header embedded in ICMP error payload (outer ICMP header = 8 bytes)
const INNER_IP_OFF: usize = ETH_HDR_LEN + IP_HDR_LEN + 8;
const INNER_IP_CSUM_OFF: usize = INNER_IP_OFF + 10;
const INNER_IP_SRC_OFF: usize = INNER_IP_OFF + 12;
const INNER_IP_DST_OFF: usize = INNER_IP_OFF + 16;

// Minimum packet length that can contain an ICMP error with an inner IP header
const MIN_ICMP_ERROR_LEN: u32 = (INNER_IP_DST_OFF + 4) as u32;

// ---------------------------------------------------------------------------
// Protocol / EtherType constants — expressed in "network byte order as u32/u16"
// i.e. what ctx.load() returns on a little-endian (bpfel) target.
// u16::from_be / u32::from_be are const and do the right byte-swap at compile time.
// ---------------------------------------------------------------------------

const ETH_P_IP_NBO: u16 = u16::from_be(0x0800_u16); // IPv4 EtherType

const IPPROTO_TCP: u8 = 6;
const IPPROTO_UDP: u8 = 17;
const IPPROTO_ICMP: u8 = 1;

const ICMP_DEST_UNREACH: u8 = 3; // carries inner IP header
const ICMP_TIME_EXCEEDED: u8 = 11; // carries inner IP header

// 169.254.1.2  — static guest link-local address
const GUEST_IP_NBO: u32 = u32::from_be(0xA9FE_0102_u32);
// 169.254.169.254 — AWS/GCP/Azure instance-metadata endpoint; must be blocked
const METADATA_IP_NBO: u32 = u32::from_be(0xA9FE_A9FE_u32);

// ---------------------------------------------------------------------------
// BPF checksum helper flags
// ---------------------------------------------------------------------------

// Indicates the rewritten field lives in the pseudo-header (src/dst IP change).
// Combining with the replacement size (4 bytes for u32 IP) gives the full flags value.
const BPF_F_PSEUDO_HDR: u64 = 0x10;
// For UDP: if the recomputed checksum is 0, store 0xFFFF (RFC 768 requires this;
// a stored-zero means "checksum not computed" in IPv4 UDP).
const BPF_F_MARK_MANGLED_0: u64 = 0x20;

// ---------------------------------------------------------------------------
// BPF map: one entry per TAP device, populated by the orchestrator at VM launch
// ---------------------------------------------------------------------------

#[map]
static VM_CONFIG: Array<VmConfig> = Array::with_max_entries(1, 0);

// ---------------------------------------------------------------------------
// tc egress — packets leaving the guest, entering the host
// ---------------------------------------------------------------------------

#[classifier]
pub fn tc_egress(ctx: TcContext) -> i32 {
    match try_egress(ctx) {
        Ok(action) => action,
        // On unexpected parse failures we pass through rather than drop:
        // the packet hasn't been (fully) modified and dropping would break
        // connectivity in edge cases. A partially-modified packet that hits
        // this path is a defect to investigate via the ICMP / short-packet
        // paths below, not a common path.
        Err(_) => TC_ACT_PIPE as i32,
    }
}

fn try_egress(mut ctx: TcContext) -> Result<i32, ()> {
    // Only process IPv4; pass everything else through unchanged.
    let ethertype: u16 = ctx.load(ETH_PROTO_OFF).map_err(|_| ())?;
    if ethertype != ETH_P_IP_NBO {
        return Ok(TC_ACT_PIPE as i32);
    }

    let src_ip: u32 = ctx.load(IP_SRC_OFF).map_err(|_| ())?;
    let dst_ip: u32 = ctx.load(IP_DST_OFF).map_err(|_| ())?;
    let protocol: u8 = ctx.load(IP_PROTO_OFF).map_err(|_| ())?;

    // Security: drop SSRF attempts toward the cloud-metadata endpoint.
    if dst_ip == METADATA_IP_NBO {
        return Ok(TC_ACT_SHOT as i32);
    }

    // Security: drop packets that claim to come from an IP other than the
    // guest's fixed link-local address.  Matching on TAP interface (not src IP)
    // would also handle the multi-guest-same-IP case, but since each TAP is
    // dedicated to exactly one VM this is equivalent and simpler.
    if src_ip != GUEST_IP_NBO {
        return Ok(TC_ACT_SHOT as i32);
    }

    // Load the per-VM NAT config written by the orchestrator.
    let config = VM_CONFIG.get(0).ok_or(())?;
    let host_ip = config.host_ip; // network byte order

    // --- Outer IP rewrite ---------------------------------------------------

    // Replace source IP: GUEST_IP → host_ip
    ctx.store(IP_SRC_OFF, &host_ip, 0).map_err(|_| ())?;

    // Update IP header checksum (covers src_ip change; 4-byte replacement)
    ctx.l3_csum_replace(IP_CSUM_OFF, src_ip as u64, host_ip as u64, 4)
        .map_err(|_| ())?;

    // --- L4 checksum update -------------------------------------------------

    match protocol {
        IPPROTO_TCP => {
            // TCP checksum includes a pseudo-header with src/dst IPs.
            ctx.l4_csum_replace(
                TCP_CSUM_OFF,
                src_ip as u64,
                host_ip as u64,
                BPF_F_PSEUDO_HDR | 4,
            )
            .map_err(|_| ())?;
        }

        IPPROTO_UDP => {
            // In IPv4, a UDP checksum of 0 means "not computed" (RFC 768).
            // Recomputing in that case would be wrong — leave it as 0.
            let udp_csum: u16 = ctx.load(UDP_CSUM_OFF).map_err(|_| ())?;
            if udp_csum != 0 {
                ctx.l4_csum_replace(
                    UDP_CSUM_OFF,
                    src_ip as u64,
                    host_ip as u64,
                    BPF_F_PSEUDO_HDR | BPF_F_MARK_MANGLED_0 | 4,
                )
                .map_err(|_| ())?;
            }
        }

        IPPROTO_ICMP => {
            // ICMP does not use a pseudo-header, so changing the outer IP src
            // does NOT affect the ICMP checksum.  No l4_csum_replace needed here.
            //
            // Exception: ICMP error messages (Type 3 / Type 11) embed the original
            // IP header in their payload.  That inner header has dst=GUEST_IP
            // (the address the guest saw after DNAT on ingress).  From the external
            // network's perspective the original packet was addressed to host_ip,
            // so rewrite inner dst: GUEST_IP → host_ip.
            let icmp_type: u8 = ctx.load(ICMP_TYPE_OFF).map_err(|_| ())?;
            if (icmp_type == ICMP_DEST_UNREACH || icmp_type == ICMP_TIME_EXCEEDED)
                && ctx.len() >= MIN_ICMP_ERROR_LEN
            {
                let inner_dst: u32 = ctx.load(INNER_IP_DST_OFF).map_err(|_| ())?;
                if inner_dst == GUEST_IP_NBO {
                    ctx.store(INNER_IP_DST_OFF, &host_ip, 0).map_err(|_| ())?;

                    // Update the inner IP header's own checksum.
                    ctx.l3_csum_replace(
                        INNER_IP_CSUM_OFF,
                        inner_dst as u64,
                        host_ip as u64,
                        4,
                    )
                    .map_err(|_| ())?;

                    // Update the outer ICMP checksum — it covers the full ICMP
                    // payload, which includes the inner IP header we just changed.
                    // No BPF_F_PSEUDO_HDR because ICMP has no pseudo-header.
                    ctx.l4_csum_replace(
                        ICMP_CSUM_OFF,
                        inner_dst as u64,
                        host_ip as u64,
                        4,
                    )
                    .map_err(|_| ())?;
                }
            }
        }

        _ => {
            // Other protocols (GRE, ESP, …): L3 rewrite is sufficient.
            // Their checksums, if any, do not include IP addresses.
        }
    }

    Ok(TC_ACT_PIPE as i32)
}

// ---------------------------------------------------------------------------
// tc ingress — packets arriving from the host, destined for the guest
// ---------------------------------------------------------------------------

#[classifier]
pub fn tc_ingress(ctx: TcContext) -> i32 {
    match try_ingress(ctx) {
        Ok(action) => action,
        Err(_) => TC_ACT_PIPE as i32,
    }
}

fn try_ingress(mut ctx: TcContext) -> Result<i32, ()> {
    // Only process IPv4.
    let ethertype: u16 = ctx.load(ETH_PROTO_OFF).map_err(|_| ())?;
    if ethertype != ETH_P_IP_NBO {
        return Ok(TC_ACT_PIPE as i32);
    }

    let dst_ip: u32 = ctx.load(IP_DST_OFF).map_err(|_| ())?;

    // Load config first so we can check dst against the allocated host_ip.
    let config = VM_CONFIG.get(0).ok_or(())?;
    let host_ip = config.host_ip;

    // Only process packets addressed to this VM's host-routable IP.
    if dst_ip != host_ip {
        return Ok(TC_ACT_PIPE as i32);
    }

    let protocol: u8 = ctx.load(IP_PROTO_OFF).map_err(|_| ())?;

    // --- Outer IP rewrite ---------------------------------------------------

    // Replace destination IP: host_ip → GUEST_IP
    ctx.store(IP_DST_OFF, &GUEST_IP_NBO, 0).map_err(|_| ())?;

    // Update IP header checksum.
    ctx.l3_csum_replace(IP_CSUM_OFF, dst_ip as u64, GUEST_IP_NBO as u64, 4)
        .map_err(|_| ())?;

    // --- L4 checksum update -------------------------------------------------

    match protocol {
        IPPROTO_TCP => {
            ctx.l4_csum_replace(
                TCP_CSUM_OFF,
                dst_ip as u64,
                GUEST_IP_NBO as u64,
                BPF_F_PSEUDO_HDR | 4,
            )
            .map_err(|_| ())?;
        }

        IPPROTO_UDP => {
            let udp_csum: u16 = ctx.load(UDP_CSUM_OFF).map_err(|_| ())?;
            if udp_csum != 0 {
                ctx.l4_csum_replace(
                    UDP_CSUM_OFF,
                    dst_ip as u64,
                    GUEST_IP_NBO as u64,
                    BPF_F_PSEUDO_HDR | BPF_F_MARK_MANGLED_0 | 4,
                )
                .map_err(|_| ())?;
            }
        }

        IPPROTO_ICMP => {
            // For inbound ICMP error messages, the payload carries the header of
            // the original packet the guest sent.  That packet went out with
            // src=host_ip (SNAT changed GUEST_IP → host_ip on egress).  The guest
            // needs to see src=GUEST_IP to match the error to its original socket.
            let icmp_type: u8 = ctx.load(ICMP_TYPE_OFF).map_err(|_| ())?;
            if (icmp_type == ICMP_DEST_UNREACH || icmp_type == ICMP_TIME_EXCEEDED)
                && ctx.len() >= MIN_ICMP_ERROR_LEN
            {
                let inner_src: u32 = ctx.load(INNER_IP_SRC_OFF).map_err(|_| ())?;
                if inner_src == host_ip {
                    ctx.store(INNER_IP_SRC_OFF, &GUEST_IP_NBO, 0).map_err(|_| ())?;

                    ctx.l3_csum_replace(
                        INNER_IP_CSUM_OFF,
                        inner_src as u64,
                        GUEST_IP_NBO as u64,
                        4,
                    )
                    .map_err(|_| ())?;

                    ctx.l4_csum_replace(
                        ICMP_CSUM_OFF,
                        inner_src as u64,
                        GUEST_IP_NBO as u64,
                        4,
                    )
                    .map_err(|_| ())?;
                }
            }
        }

        _ => {}
    }

    Ok(TC_ACT_PIPE as i32)
}

// ---------------------------------------------------------------------------
// Required panic handler for no_std BPF programs
// ---------------------------------------------------------------------------

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
