#![no_std]
#![no_main]

use aya_ebpf::{
    bindings::TC_ACT_PIPE,
    macros::{classifier, map},
    maps::Array,
    programs::TcContext,
};
use microvisor_common::VmConfig;

/// Per-TAP NAT config written by the orchestrator at VM creation time.
/// Key 0 holds the single VmConfig entry for this TAP device.
#[map]
static VM_CONFIG: Array<VmConfig> = Array::with_max_entries(1, 0);

/// tc egress classifier — packets leaving the guest, entering the host.
/// Full NAT rewrite logic lives here in Phase 2.
#[classifier]
pub fn tc_egress(_ctx: TcContext) -> i32 {
    TC_ACT_PIPE as i32
}

/// tc ingress classifier — packets arriving from the host, destined for the guest.
/// Full DNAT rewrite logic lives here in Phase 2.
#[classifier]
pub fn tc_ingress(_ctx: TcContext) -> i32 {
    TC_ACT_PIPE as i32
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
