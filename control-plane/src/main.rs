pub mod vmm;

use vmm::Orchestrator;
use std::io;
use tracing_subscriber::prelude::*;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry::trace::TracerProvider;

#[tokio::main]
async fn main() -> io::Result<()> {
    // 1. Configure OTLP Exporter
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint("http://10.0.0.2:4317")
        .build()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .build();

    let tracer = provider.tracer("microvisor-control-plane");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    // 2. Configure JSON stdout layer
    let json_layer = tracing_subscriber::fmt::layer()
        .json();

    // 3. Register layers
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with(json_layer)
        .with(otel_layer)
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
    use vmm::cgroup::{CgroupManager, CpusetAllocator};
    use vmm::network::IpAm;
    use vmm::state::{VmStateMachine, VmState};
    use tempfile::tempdir;

    #[test]
    fn test_state_machine_transitions() {
        let mut sm = VmStateMachine::new(vmm::observability::generate_session_id());
        assert_eq!(sm.current_state(), VmState::Pending);

        sm.transition_to(VmState::ProvisioningStorage).unwrap();
        sm.transition_to(VmState::ConfiguringNetwork).unwrap();

        // Test invalid transition
        let result = sm.transition_to(VmState::Running);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cgroup_creation() {
        let dir = tempdir().unwrap();
        let manager = CgroupManager::new(dir.path()).unwrap();
        let vm_cgroup = manager.create_vm_cgroup("test-vm").await.unwrap();
        
        assert!(dir.path().join("vm-test-vm").exists());
        assert!(vm_cgroup.path().exists());
    }

    #[test]
    fn test_naive_cpuset_allocator() {
        let allocator = vmm::cgroup::NaiveCpusetAllocator;
        assert_eq!(allocator.allocate_cpus("0,1").unwrap(), "0,1");
        assert_eq!(allocator.allocate_cpus("2").unwrap(), "2");
    }

    #[test]
    fn test_sibling_aware_cpuset_allocator() {
        let dir = tempdir().unwrap();
        let sysfs = dir.path();
        
        // Mock CPU 0 and 4 as siblings
        let cpu0_dir = sysfs.join("devices/system/cpu/cpu0/topology");
        let cpu4_dir = sysfs.join("devices/system/cpu/cpu4/topology");
        std::fs::create_dir_all(&cpu0_dir).unwrap();
        std::fs::create_dir_all(&cpu4_dir).unwrap();
        std::fs::write(cpu0_dir.join("thread_siblings_list"), "0,4\n").unwrap();
        std::fs::write(cpu4_dir.join("thread_siblings_list"), "0,4\n").unwrap();

        // Mock CPU 1 and 5 as siblings
        std::fs::create_dir_all(sysfs.join("devices/system/cpu/cpu1/topology")).unwrap();
        std::fs::create_dir_all(sysfs.join("devices/system/cpu/cpu5/topology")).unwrap();
        std::fs::write(sysfs.join("devices/system/cpu/cpu1/topology/thread_siblings_list"), "1,5\n").unwrap();
        std::fs::write(sysfs.join("devices/system/cpu/cpu5/topology/thread_siblings_list"), "1,5\n").unwrap();

        let allocator = vmm::cgroup::SiblingAwareCpusetAllocator::with_root(sysfs.to_path_buf());
        
        // Requesting 0 should give 0,4
        assert_eq!(allocator.allocate_cpus("0").unwrap(), "0,4");
        
        // Requesting 0,1 should give 0,1,4,5 (sorted)
        assert_eq!(allocator.allocate_cpus("0,1").unwrap(), "0,1,4,5");
        
        // Requesting 4 should give 0,4
        assert_eq!(allocator.allocate_cpus("4").unwrap(), "0,4");
    }

    #[test]
    fn test_cpuset_allocator_edge_cases() {
        let dir = tempdir().unwrap();
        let sysfs = dir.path();
        let allocator = vmm::cgroup::SiblingAwareCpusetAllocator::with_root(sysfs.to_path_buf());

        // Missing CPU file
        assert!(allocator.allocate_cpus("99").is_err());

        // Invalid string format
        assert!(allocator.allocate_cpus("abc").is_err());
        
        // Empty string (should handle gracefully or error, let's see current impl)
        assert_eq!(allocator.allocate_cpus("").unwrap(), "");
    }

    #[test]
    fn test_numa_topology_single_node() {
        let dir = tempdir().unwrap();
        let sysfs = dir.path();
        
        // Mock single NUMA node (node0)
        let node0_dir = sysfs.join("devices/system/node/node0");
        std::fs::create_dir_all(&node0_dir).unwrap();

        let topology = vmm::cgroup::NumaTopology::with_root(sysfs.to_path_buf());
        
        // Requesting node 1 on single-node host should be forced to 0
        assert_eq!(topology.get_mems_string("1").unwrap(), "0");
    }

    #[test]
    fn test_numa_topology_multi_node() {
        let dir = tempdir().unwrap();
        let sysfs = dir.path();
        
        // Mock two NUMA nodes
        std::fs::create_dir_all(sysfs.join("devices/system/node/node0")).unwrap();
        std::fs::create_dir_all(sysfs.join("devices/system/node/node1")).unwrap();

        let topology = vmm::cgroup::NumaTopology::with_root(sysfs.to_path_buf());
        
        // Should honor requested node in multi-node system
        assert_eq!(topology.get_mems_string("1").unwrap(), "1");
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
