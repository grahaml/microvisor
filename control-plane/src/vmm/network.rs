use std::sync::atomic::{AtomicU64, Ordering};
use std::io;
use std::os::unix::io::AsRawFd;
use tracing::{instrument, info, warn};
use crate::vmm::observability::SyscallAuditor;

// TUNSETIFF is not always defined in libc for all architectures, define it manually
// This is the standard value for Linux x86_64
const TUNSETIFF: libc::c_ulong = 0x400454ca;

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
                // Ensure IP is converted to network order if needed elsewhere, 
                // but for now we store as host order u32.
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

pub struct TapDevice {
    name: String,
    _file: std::fs::File,
}

impl TapDevice {
    /// Creates a persistent TAP device via /dev/net/tun.
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
            }

            // Set the interface UP
            // For the POC, we'll shell out to 'ip link set' for simplicity in managing 
            // the control socket vs the link state.
            let status = std::process::Command::new("ip")
                .arg("link")
                .arg("set")
                .arg(&name_owned)
                .arg("up")
                .status()?;

            if !status.success() {
                warn!(name = %name_owned, "Failed to set TAP interface up");
            }

            Ok(Self {
                name: name_owned,
                _file: file,
            })
        }).await?
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

pub struct EbpfProgram {
    // bpf_obj: Option<Box<dyn std::any::Any>>,
}

impl EbpfProgram {
    /// Loads the stateless NAT eBPF program and attaches it to the TAP interface.
    #[instrument]
    pub async fn load_nat_program(tap_name: &str, host_ip: u32) -> io::Result<Self> {
        info!(tap_name, host_ip, "Loading eBPF NAT program (Stub)");
        // In a real implementation:
        // 1. Compile/Open eBPF object
        // 2. Load into kernel
        // 3. Attach to tc qdisc clsact
        // 4. Update maps with (169.254.1.2 -> host_ip)
        Ok(Self {})
    }
}
