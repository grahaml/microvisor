pub mod vmm;

use vmm::Orchestrator;
use std::io;

#[tokio::main]
async fn main() -> io::Result<()> {
    println!("Microvisor Control Plane starting...");

    let orchestrator = Orchestrator::new(
        "/sys/fs/cgroup/orchestrator",
        "thin-pool-0",
        0x0A000001, // 10.0.0.1
        "./bin/firecracker"
    )?;

    println!("Orchestrator initialized.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vmm::cgroup::CgroupManager;
    use vmm::network::IpAm;
    use tempfile::tempdir;

    #[test]
    fn test_cgroup_creation() {
        let dir = tempdir().unwrap();
        let manager = CgroupManager::new(dir.path()).unwrap();
        let vm_cgroup = manager.create_vm_cgroup("test-vm").unwrap();
        
        assert!(dir.path().join("vm-test-vm").exists());
        assert!(vm_cgroup.path().exists());
    }

    #[test]
    fn test_ipam_allocation() {
        let ipam = IpAm::new(0x0A000000);
        let ip1 = ipam.allocate().unwrap();
        let ip2 = ipam.allocate().unwrap();
        
        assert_eq!(ip1, 0x0A000000);
        assert_eq!(ip2, 0x0A000001);
        
        ipam.release(ip1);
        let ip3 = ipam.allocate().unwrap();
        assert_eq!(ip3, 0x0A000000);
    }
}
