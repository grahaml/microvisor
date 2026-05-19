use std::sync::atomic::{AtomicU64, Ordering};
use std::io;
use tracing::{instrument, info};

pub struct IpAm {
    /// Atomic bitset for IP allocation (up to 64 IPs for now)
    /// In a real system, this would be a larger bitset or a more complex structure.
    /// Bit 0 represents the first IP in the range.
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
        let index = ip - self.base_ip;
        if index < 64 {
            let mask = 1 << index;
            self.bits.fetch_and(!mask, Ordering::SeqCst);
        }
    }
}

pub struct TapDevice {
    name: String,
    // fd: RawFd,
}

impl TapDevice {
    #[instrument]
    pub fn create(name: &str) -> io::Result<Self> {
        // ioctl(TUNSETIFF) to /dev/net/tun
        info!(name, "Creating TAP device");
        Ok(Self {
            name: name.to_string(),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

pub struct EbpfProgram {
    // fd: RawFd,
}

impl EbpfProgram {
    #[instrument]
    pub fn load_nat_program(tap_name: &str, host_ip: u32) -> io::Result<Self> {
        // 1. Load eBPF bytecode using bpf() syscall
        // 2. Attach to tc egress/ingress on TAP device
        // 3. Update BPF map with host_ip
        info!(tap_name, host_ip, "Loading eBPF NAT program");
        Ok(Self {})
    }
}
