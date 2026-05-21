use std::sync::atomic::{AtomicU64, Ordering};
use std::io;
use std::os::unix::io::AsRawFd;
use tracing::{instrument, info, warn};
use crate::vmm::observability::SyscallAuditor;

use aya::{
    include_bytes_aligned,
    maps::Array,
    programs::{SchedClassifier, TcAttachType},
    Ebpf,
};
use microvisor_common::VmConfig;

// ---------------------------------------------------------------------------
// Compiled eBPF ELF, embedded at build time.
// Run `cargo xtask build-ebpf` before `cargo build` to produce this file.
// ---------------------------------------------------------------------------

static EBPF_BYTES: &[u8] = include_bytes_aligned!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/bpf/microvisor-ebpf"
));

// TUNSETIFF / TUNSETPERSIST: standard Linux TUN/TAP ioctls (stable ABI).
const TUNSETIFF: libc::c_ulong = 0x400454ca;
const TUNSETPERSIST: libc::c_ulong = 0x400454cb;

// SIOCSIFFLAGS / SIOCGIFFLAGS: standard Linux socket ioctl to set/get interface flags.
// Not exposed in all libc platform targets; values are stable Linux ABI.
const SIOCGIFFLAGS: libc::c_ulong = 0x8913;
const SIOCSIFFLAGS: libc::c_ulong = 0x8914;

// ---------------------------------------------------------------------------
// IP address manager
// ---------------------------------------------------------------------------

pub struct IpAm {
    /// Atomic bitset for IP allocation (up to 64 IPs for now)
    bits: AtomicU64,
    base_ip: u32, // Host-routable base IP as u32
}

impl IpAm {
    pub fn new(base_ip: u32) -> Self {
        Self {
            bits: AtomicU64::new(0),
            base_ip,
        }
    }

    #[instrument(skip(self))]
    pub fn allocate(&self) -> io::Result<u32> {
        loop {
            let current = self.bits.load(Ordering::SeqCst);
            let first_free = (!current).trailing_zeros();
            if first_free >= 64 {
                return Err(io::Error::new(io::ErrorKind::Other, "No free IPs"));
            }
            let mask = 1 << first_free;
            if self.bits.compare_exchange(current, current | mask, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                return Ok(self.base_ip + first_free);
            }
        }
    }

    #[instrument(skip(self))]
    pub fn release(&self, ip: u32) {
        if ip < self.base_ip { return; }
        let index = ip - self.base_ip;
        if index < 64 {
            let mask = 1 << index;
            self.bits.fetch_and(!mask, Ordering::SeqCst);
        }
    }
}

// ---------------------------------------------------------------------------
// TAP device
// ---------------------------------------------------------------------------

pub struct TapDevice {
    name: String,
}

impl TapDevice {
    /// Creates a persistent TAP device, brings it UP, then releases the fd.
    ///
    /// TUNSETPERSIST keeps the interface alive after we close our fd so that
    /// Firecracker (running inside the jailer chroot) can attach to it by name
    /// without hitting EBUSY from a competing open fd.
    #[instrument]
    pub async fn create(name: &str) -> io::Result<Self> {
        info!(name, "Creating TAP device");
        let name_owned = name.to_string();

        SyscallAuditor::spawn_blocking("tap_create", move || {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open("/dev/net/tun")?;

            let mut ifr: libc::ifreq = unsafe { std::mem::zeroed() };

            // IFF_TAP: Ethernet TAP device
            // IFF_NO_PI: Do not provide packet information (standard for Firecracker)
            ifr.ifr_ifru.ifru_flags = (libc::IFF_TAP | libc::IFF_NO_PI) as i16;

            let bytes = name_owned.as_bytes();
            let len = std::cmp::min(bytes.len(), libc::IFNAMSIZ - 1);
            unsafe {
                std::ptr::copy_nonoverlapping(
                    bytes.as_ptr(),
                    ifr.ifr_name.as_mut_ptr() as *mut u8,
                    len,
                );
            }

            unsafe {
                let res = libc::ioctl(file.as_raw_fd(), TUNSETIFF, &ifr);
                if res < 0 {
                    return Err(io::Error::last_os_error());
                }
                // Mark persistent so the interface survives after we close this fd.
                let res = libc::ioctl(file.as_raw_fd(), TUNSETPERSIST, 1usize);
                if res < 0 {
                    return Err(io::Error::last_os_error());
                }
            }

            set_interface_up(&name_owned)?;

            // fd closes here — interface persists due to TUNSETPERSIST.
            Ok(Self { name: name_owned })
        }).await?
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Drop for TapDevice {
    fn drop(&mut self) {
        // Reopen the interface to get an fd, then un-persist and close — this
        // deletes the interface from the kernel network stack.
        let Ok(file) = std::fs::OpenOptions::new()
            .read(true).write(true).open("/dev/net/tun") else { return };

        let mut ifr: libc::ifreq = unsafe { std::mem::zeroed() };
        ifr.ifr_ifru.ifru_flags = (libc::IFF_TAP | libc::IFF_NO_PI) as i16;
        let bytes = self.name.as_bytes();
        let len = std::cmp::min(bytes.len(), libc::IFNAMSIZ - 1);
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ifr.ifr_name.as_mut_ptr() as *mut u8, len) };

        unsafe {
            if libc::ioctl(file.as_raw_fd(), TUNSETIFF, &ifr) == 0 {
                libc::ioctl(file.as_raw_fd(), TUNSETPERSIST, 0usize);
            }
        }
        // file closes here, taking the interface down.
    }
}

/// Brings a network interface UP via SIOCSIFFLAGS.
///
/// Opens a temporary SOCK_DGRAM, reads current flags with SIOCGIFFLAGS,
/// ORs in IFF_UP, and writes back with SIOCSIFFLAGS. Avoids shelling out
/// to `ip link set`, keeping the control-plane free of CLI dependencies.
fn set_interface_up(name: &str) -> io::Result<()> {
    struct FdGuard(libc::c_int);
    impl Drop for FdGuard {
        fn drop(&mut self) {
            unsafe { libc::close(self.0); }
        }
    }

    let sock = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) };
    if sock < 0 {
        return Err(io::Error::last_os_error());
    }
    let _guard = FdGuard(sock);

    let mut ifr: libc::ifreq = unsafe { std::mem::zeroed() };
    let bytes = name.as_bytes();
    let len = std::cmp::min(bytes.len(), libc::IFNAMSIZ - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(
            bytes.as_ptr(),
            ifr.ifr_name.as_mut_ptr() as *mut u8,
            len,
        );
    }

    if unsafe { libc::ioctl(sock, SIOCGIFFLAGS, &mut ifr) } < 0 {
        return Err(io::Error::last_os_error());
    }

    unsafe { ifr.ifr_ifru.ifru_flags |= libc::IFF_UP as i16; }

    if unsafe { libc::ioctl(sock, SIOCSIFFLAGS, &ifr) } < 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// eBPF NAT program loader
// ---------------------------------------------------------------------------

pub struct EbpfProgram {
    /// Holds the loaded BPF object alive for the lifetime of the VM's network.
    /// Dropping this detaches the programs and removes the maps.
    _ebpf: Ebpf,
}

impl EbpfProgram {
    /// Loads the stateless NAT eBPF programs and attaches them to `tap_name`.
    ///
    /// `host_ip` is the IPAM-allocated host-routable IP in **host byte order**.
    /// It is stored in the BPF map as network byte order (`.to_be()`).
    #[instrument]
    pub async fn load_nat_program(tap_name: &str, host_ip: u32) -> io::Result<Self> {
        info!(tap_name, host_ip, "Loading eBPF NAT program");
        let tap_name = tap_name.to_string();

        SyscallAuditor::spawn_blocking("ebpf_load", move || {
            let mut ebpf = Ebpf::load(EBPF_BYTES)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("aya load: {e}")))?;

            // Populate VM_CONFIG[0] before attaching so the programs never see
            // an uninitialised entry.
            {
                let map = ebpf
                    .map_mut("VM_CONFIG")
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "VM_CONFIG map not found"))?;
                let mut config_map: Array<_, VmConfig> = map
                    .try_into()
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("array map: {e}")))?;
                let config = VmConfig {
                    host_ip: host_ip.to_be(),
                    guest_ip: 0xA9FE_0102_u32.to_be(), // 169.254.1.2
                    mac: [0u8; 6],
                    _pad: [0u8; 2],
                };
                config_map
                    .set(0, config, 0)
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("map set: {e}")))?;
            }

            // Create the clsact qdisc on the TAP interface.
            // Ignore EEXIST: a prior run may have already installed it.
            let _ = aya::programs::tc::qdisc_add_clsact(&tap_name);

            // Egress: packets leaving the guest → rewrite src IP (SNAT).
            {
                let prog: &mut SchedClassifier = ebpf
                    .program_mut("tc_egress")
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "tc_egress not found"))?
                    .try_into()
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("program type: {e}")))?;
                prog.load()
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("egress load: {e}")))?;
                prog.attach(&tap_name, TcAttachType::Egress)
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("egress attach: {e}")))?;
            }

            // Ingress: packets arriving at the guest → rewrite dst IP (DNAT).
            {
                let prog: &mut SchedClassifier = ebpf
                    .program_mut("tc_ingress")
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "tc_ingress not found"))?
                    .try_into()
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("program type: {e}")))?;
                prog.load()
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("ingress load: {e}")))?;
                prog.attach(&tap_name, TcAttachType::Ingress)
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("ingress attach: {e}")))?;
            }

            Ok(Self { _ebpf: ebpf })
        }).await?
    }
}
