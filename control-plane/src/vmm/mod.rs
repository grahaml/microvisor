pub mod cgroup;
pub mod storage;
pub mod network;
pub mod observability;
pub mod state;

use std::process::{Command, Child};
use std::io;
use std::path::PathBuf;
use tracing::{instrument, info};
use rand::Rng;
use self::cgroup::{VmCgroup, CgroupManager};
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
            firecracker_path: firecracker_path.to_string(),
        })
    }

    pub fn generate_session_id() -> u64 {
        rand::rng().random()
    }

    #[instrument(skip(self), fields(session_id = %config.session_id, vm_id = %config.id))]
    pub async fn launch_vm(&self, config: VmConfig) -> Result<Vm, OrchestratorError> {
        let mut sm = VmStateMachine::new(config.session_id);
        
        // 1. Storage provisioning (Spec-001)
        sm.transition_to(VmState::ProvisioningStorage)
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e))?;

        let rootfs = self.storage_manager.create_snapshot(&config.base_image, &config.id)
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

        let cgroup = self.cgroup_manager.create_vm_cgroup(&config.id)
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e.to_string()))?;
        cgroup.set_cpuset(&config.vcpu_cores, &config.numa_node)
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e.to_string()))?;
        cgroup.set_memory_limit(config.mem_size_mib as u64 * 1024 * 1024)
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e.to_string()))?;
        
        // 4. Launch VMM
        sm.transition_to(VmState::LaunchingVMM)
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e))?;

        // In reality, we'd pass all the above info to Firecracker
        let child = Command::new(&self.firecracker_path)
            .spawn()
            .map_err(|e| OrchestratorError::new(config.session_id, sm.current_state(), e.to_string()))?;
        
        cgroup.add_process(child.id())
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
