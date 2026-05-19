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
    use std::fs;
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

    #[tokio::test]
    async fn test_metadata_drive_create() {
        use vmm::storage::MetadataDrive;
        let dir = tempdir().unwrap();
        let staging = dir.path().join("staging");
        fs::create_dir(&staging).unwrap();
        fs::write(staging.join("hello.txt"), "world").unwrap();
        
        let drive_path = dir.path().join("metadata.img");
        let _drive = MetadataDrive::create(&drive_path, &staging).await.unwrap();
        
        assert!(drive_path.exists());
        // We could use `dumpe2fs` or `ls -l` to verify size, but existence is a good start
        let metadata = fs::metadata(&drive_path).unwrap();
        assert_eq!(metadata.len(), 1024 * 1024); // 1MB
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
    fn test_ipam_allocation_exhaustive() {
        let ipam = IpAm::new(0x0A000000);
        // Allocate all 64 IPs
        for i in 0..64 {
            assert_eq!(ipam.allocate().unwrap(), 0x0A000000 + i);
        }
        // Next one should fail
        assert!(ipam.allocate().is_err());
        
        // Release one and re-allocate
        ipam.release(0x0A000000 + 10);
        assert_eq!(ipam.allocate().unwrap(), 0x0A000000 + 10);
    }

    #[test]
    fn test_ipam_release_out_of_bounds() {
        let ipam = IpAm::new(0x0A000000);
        let ip = ipam.allocate().unwrap();
        
        // Releasing an IP from a different range should be a no-op
        ipam.release(0x0B000000);
        
        // The original IP should still be allocated (bit 0 set)
        // We check this by trying to allocate again and getting bit 1
        assert_eq!(ipam.allocate().unwrap(), ip + 1);
    }

    #[tokio::test]
    async fn test_tap_device_name() {
        // We can't easily create a real TAP without root, but we can verify
        // that it fails with the expected OS error (Permission Denied) 
        // rather than a logic error.
        let result = vmm::network::TapDevice::create("test-tap-0").await;
        if let Err(e) = result {
            // On most CI/dev systems this will be PermissionDenied or EACCES
            assert!(e.kind() == io::ErrorKind::PermissionDenied || e.raw_os_error() == Some(1));
        }
    }
}
