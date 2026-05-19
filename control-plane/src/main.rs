pub mod vmm;

use vmm::Orchestrator;
use std::io;
use tracing_subscriber::fmt::format::FmtSpan;

#[tokio::main]
async fn main() -> io::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_span_events(FmtSpan::CLOSE)
        .json()
        .init();

    tracing::info!("Microvisor Control Plane starting...");

    let _orchestrator = Orchestrator::new(
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
    use vmm::state::{VmStateMachine, VmState};
    use tempfile::tempdir;

    #[test]
    fn test_state_machine_transitions() {
        let mut sm = VmStateMachine::new(12345);
        assert_eq!(sm.current_state(), VmState::Pending);

        sm.transition_to(VmState::ProvisioningStorage).unwrap();
        sm.transition_to(VmState::ConfiguringNetwork).unwrap();

        // Test invalid transition
        let result = sm.transition_to(VmState::Running);
        assert!(result.is_err());
    }

    #[test]
    fn test_cgroup_creation() {
        let dir = tempdir().unwrap();
        let manager = CgroupManager::new(dir.path()).unwrap();
        let vm_cgroup = manager.create_vm_cgroup("test-vm").unwrap();
        
        assert!(dir.path().join("vm-test-vm").exists());
        assert!(vm_cgroup.path().exists());
    }

    #[test]
    fn test_syscall_auditor() {
        use vmm::observability::SyscallAuditor;
        use std::thread;
        use std::time::Duration;

        SyscallAuditor::audit("fast_call", || {});

        SyscallAuditor::audit("slow_call", || {
            thread::sleep(Duration::from_millis(2));
        });
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
