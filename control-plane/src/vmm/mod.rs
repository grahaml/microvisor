pub mod cgroup;
pub mod storage;
pub mod network;
pub mod observability;
pub mod state;

use std::process::{Command, Child};
use std::io;
use std::path::PathBuf;
use tracing::{instrument, info};
use self::cgroup::{VmCgroup, CgroupManager, CpusetAllocator, NaiveCpusetAllocator, NumaTopology};
use self::storage::StorageManager;
use self::network::{IpAm, TapDevice, EbpfProgram};
use self::state::{VmState, VmStateMachine, OrchestratorError};

#[derive(Debug, Clone)]
pub struct VmConfig {
    pub id: String,
    pub session_id: u64,
    pub cpu_count: u32,
    pub mem_size_mib: u32,
    pub vcpu_cores: String, // e.g., "2,3"
    pub numa_node: String,  // e.g., "0"
    pub base_image: String,
}

pub struct Vm {
    pub config: VmConfig,
    pub cgroup: VmCgroup,
    pub rootfs: PathBuf,
    pub tap: TapDevice,
    pub host_ip: u32,
    pub process: Option<Child>,
}

pub struct Orchestrator {
    cgroup_manager: CgroupManager,
    storage_manager: StorageManager,
    ipam: IpAm,
    cpuset_allocator: Box<dyn CpusetAllocator>,
    numa_topology: NumaTopology,
    firecracker_path: String,
}

impl Orchestrator {
    pub fn new(
        cgroup_root: &str,
        pool_name: &str,
        base_ip: u32,
        firecracker_path: &str,
    ) -> io::Result<Self> {
        info!(cgroup_root, pool_name, base_ip, "Initializing Orchestrator");
        Ok(Self {
            cgroup_manager: CgroupManager::new(cgroup_root)?,
            storage_manager: StorageManager::new(pool_name),
            ipam: IpAm::new(base_ip),
            cpuset_allocator: Box::new(NaiveCpusetAllocator),
            numa_topology: NumaTopology::new(),
            firecracker_path: firecracker_path.to_string(),
        })
    }

    #[instrument(skip(self), fields(session_id = %config.session_id, vm_id = %config.id))]
    pub async fn launch_vm(&self, config: VmConfig) -> Result<Vm, OrchestratorError> {
        let mut sm = VmStateMachine::new(config.session_id);
        
        // 1. Storage provisioning (Spec-001)
        sm.transition_to(VmState::ProvisioningStorage)
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e))?;

        // For POC, assume base image internal ID is 1
        let base_image_id = 1;
        let rootfs = self.storage_manager.create_snapshot(base_image_id, &config.id).await
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e.to_string()))?;
        
        // 2. Network provisioning (Spec-002)
        sm.transition_to(VmState::ConfiguringNetwork)
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e))?;

        let tap_name = format!("tap-{}", config.id);
        let tap = TapDevice::create(&tap_name)
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e.to_string()))?;
        let host_ip = self.ipam.allocate()
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e.to_string()))?;
        let _ebpf = EbpfProgram::load_nat_program(&tap_name, host_ip)
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e.to_string()))?;
        
        // 3. Cgroup setup (Spec-000, Spec-003)
        sm.transition_to(VmState::InitializingCgroup)
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e))?;

        let cgroup = self.cgroup_manager.create_vm_cgroup(&config.id).await
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e.to_string()))?;
        
        let allocated_cpus = self.cpuset_allocator.allocate_cpus(&config.vcpu_cores)
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e.to_string()))?;
            
        let allocated_mems = self.numa_topology.get_mems_string(&config.numa_node)
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e.to_string()))?;

        cgroup.set_cpuset(&allocated_cpus, &allocated_mems).await
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e.to_string()))?;
        cgroup.set_memory_limit(config.mem_size_mib as u64 * 1024 * 1024).await
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e.to_string()))?;
        
        // 4. Launch VMM
        sm.transition_to(VmState::LaunchingVMM)
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e))?;

        // In reality, we'd pass all the above info to Firecracker
        let child = Command::new(&self.firecracker_path)
            .spawn()
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e.to_string()))?;
        
        cgroup.add_process(child.id()).await
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e.to_string()))?;

        sm.transition_to(VmState::Running)
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e))?;

        Ok(Vm {
            config,
            cgroup,
            rootfs,
            tap,
            host_ip,
            process: Some(child),
        })
    }
}
