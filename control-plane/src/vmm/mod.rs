pub mod cgroup;
pub mod storage;
pub mod network;
pub mod observability;

use std::process::{Command, Child};
use std::io;
use std::path::PathBuf;
use self::cgroup::{VmCgroup, CgroupManager};
use self::storage::StorageManager;
use self::network::{IpAm, TapDevice, EbpfProgram};

pub struct VmConfig {
    pub id: String,
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
        Ok(Self {
            cgroup_manager: CgroupManager::new(cgroup_root)?,
            storage_manager: StorageManager::new(pool_name),
            ipam: IpAm::new(base_ip),
            firecracker_path: firecracker_path.to_string(),
        })
    }

    pub async fn launch_vm(&self, config: VmConfig) -> io::Result<Vm> {
        // 1. Storage provisioning (Spec-001)
        let rootfs = self.storage_manager.create_snapshot(&config.base_image, &config.id)?;
        
        // 2. Network provisioning (Spec-002)
        let tap_name = format!("tap-{}", config.id);
        let tap = TapDevice::create(&tap_name)?;
        let host_ip = self.ipam.allocate()?;
        let _ebpf = EbpfProgram::load_nat_program(&tap_name, host_ip)?;
        
        // 3. Cgroup setup (Spec-000, Spec-003)
        let cgroup = self.cgroup_manager.create_vm_cgroup(&config.id)?;
        cgroup.set_cpuset(&config.vcpu_cores, &config.numa_node)?;
        cgroup.set_memory_limit(config.mem_size_mib as u64 * 1024 * 1024)?;
        
        let mut vm = Vm {
            config,
            cgroup,
            rootfs,
            tap,
            host_ip,
            process: None,
        };

        // 4. Launch VMM
        // In reality, we'd pass all the above info to Firecracker
        let child = Command::new(&self.firecracker_path)
            .spawn()?;
        
        vm.cgroup.add_process(child.id())?;
        vm.process = Some(child);

        Ok(vm)
    }
}
