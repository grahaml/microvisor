pub mod cgroup;
pub mod storage;
pub mod network;
pub mod observability;
pub mod state;

use std::process::{Command, Child};
use std::io;
use std::path::PathBuf;
use std::os::unix::process::CommandExt;
use tracing::{instrument, info, warn};
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
    pub ebpf: EbpfProgram,
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

    /// Provisions a VM through the full lifecycle state machine.
    ///
    /// On any error, `ProvisioningContext::cleanup` is called to release all
    /// resources acquired up to the point of failure — in reverse allocation order.
    #[instrument(skip(self), fields(session_id = %config.session_id, vm_id = %config.id))]
    pub async fn launch_vm(&self, config: VmConfig) -> Result<Vm, OrchestratorError> {
        let session_id = config.session_id;
        let vm_id = config.id.clone();
        let mut ctx = ProvisioningContext::new(session_id, &vm_id);

        match self.provision_vm(config, &mut ctx).await {
            Ok(vm) => Ok(vm),
            Err(e) => {
                warn!(session_id, vm_id, state = %e.state, "VM provisioning failed; running cleanup");
                ctx.cleanup(&self.storage_manager, &self.ipam).await;
                Err(e)
            }
        }
    }

    async fn provision_vm(
        &self,
        config: VmConfig,
        ctx: &mut ProvisioningContext,
    ) -> Result<Vm, OrchestratorError> {
        let session_id = config.session_id;
        let mut sm = VmStateMachine::new(session_id);

        // 1. Storage provisioning (Spec-001)
        sm.transition_to(VmState::ProvisioningStorage)
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e))?;

        let rootfs = self.storage_manager.create_snapshot(1, &config.id).await
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;
        // Record for cleanup before any subsequent fallible operation.
        ctx.snapshot_name = Some(config.id.clone());

        // 2. Network provisioning (Spec-002 / Spec-007)
        sm.transition_to(VmState::ConfiguringNetwork)
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e))?;

        let tap_name = format!("tap-{}", config.id);
        // TapDevice is non-persistent: its fd close destroys the interface on Drop.
        let tap = TapDevice::create(&tap_name).await
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;

        let host_ip = self.ipam.allocate()
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;
        ctx.host_ip = Some(host_ip);

        // EbpfProgram Drop detaches tc hooks and removes BPF objects.
        let ebpf = EbpfProgram::load_nat_program(&tap_name, host_ip).await
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;

        // 3. Cgroup setup (Spec-000 / Spec-003)
        sm.transition_to(VmState::InitializingCgroup)
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e))?;

        let cgroup = self.cgroup_manager.create_vm_cgroup(&config.id).await
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;
        // Move into context so cleanup can call cgroup.delete() on error.
        ctx.cgroup = Some(cgroup);

        let allocated_cpus = self.cpuset_allocator.allocate_cpus(&config.vcpu_cores)
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;
        let allocated_mems = self.numa_topology.get_mems_string(&config.numa_node)
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;

        ctx.cgroup.as_ref().unwrap()
            .set_cpuset(&allocated_cpus, &allocated_mems).await
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;
        ctx.cgroup.as_ref().unwrap()
            .set_memory_limit(config.mem_size_mib as u64 * 1024 * 1024).await
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;

        // 4. Launch VMM (Spec-006)
        sm.transition_to(VmState::LaunchingVMM)
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e))?;

        // SAFETY: pre_exec runs post-fork/pre-exec in the child. prctl(2) is
        // async-signal-safe. No allocations, no panics, no Rust runtime calls.
        let child = unsafe {
            Command::new(&self.firecracker_path)
                .pre_exec(|| {
                    // Ensure the child receives SIGKILL if the orchestrator exits.
                    // Prevents orphan Firecracker processes accumulating across restarts.
                    let ret = libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0);
                    if ret != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    Ok(())
                })
                .spawn()
        }
        .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;

        let pid = child.id();
        ctx.process = Some(child);

        ctx.cgroup.as_ref().unwrap()
            .add_process(pid).await
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;

        sm.transition_to(VmState::Running)
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e))?;

        // All resources provisioned successfully. Move out of the context and into Vm.
        // Any field left as None after this point will be a no-op if cleanup is called.
        Ok(Vm {
            config,
            cgroup: ctx.cgroup.take().expect("cgroup was set above"),
            rootfs,
            tap,
            ebpf,
            host_ip,
            process: ctx.process.take(),
        })
    }
}

// ---------------------------------------------------------------------------
// Provisioning context — tracks non-RAII resources for rollback
// ---------------------------------------------------------------------------

/// Tracks resources allocated during VM provisioning.
///
/// `TapDevice` and `EbpfProgram` have RAII cleanup via Drop so they are held
/// as local variables in `provision_vm`. The resources here require explicit
/// async teardown (DM snapshot, IPAM bitset, cgroup hierarchy, VMM process).
///
/// Each field is `Option` so that `cleanup()` is idempotent: each resource is
/// released via `take()` exactly once even if `cleanup()` is called repeatedly
/// or if provisioning failed before the resource was allocated.
struct ProvisioningContext {
    session_id: u64,
    vm_id: String,
    /// DM thin snapshot name; same as vm_id, used for `dmsetup remove`.
    snapshot_name: Option<String>,
    /// IPAM bitset slot (host-order IP); released via `IpAm::release`.
    host_ip: Option<u32>,
    /// cgroup hierarchy for the VM; deleted via `VmCgroup::delete`.
    cgroup: Option<VmCgroup>,
    /// Firecracker process; killed on cleanup to avoid orphans.
    process: Option<Child>,
}

impl ProvisioningContext {
    fn new(session_id: u64, vm_id: &str) -> Self {
        Self {
            session_id,
            vm_id: vm_id.to_string(),
            snapshot_name: None,
            host_ip: None,
            cgroup: None,
            process: None,
        }
    }

    /// Releases all allocated resources in reverse allocation order.
    ///
    /// Errors from individual cleanup steps are logged as warnings but do not
    /// abort the remaining cleanup steps — partial failure is better than a
    /// leak cascade.
    async fn cleanup(&mut self, storage: &StorageManager, ipam: &IpAm) {
        // Kill VMM first so the process vacates the cgroup before we try to remove it.
        if let Some(mut child) = self.process.take() {
            warn!(
                session_id = self.session_id,
                vm_id = %self.vm_id,
                "Killing VMM process during rollback"
            );
            let _ = child.kill();
        }

        // Remove cgroup hierarchy.
        if let Some(cgroup) = self.cgroup.take() {
            if let Err(e) = cgroup.delete().await {
                warn!(
                    session_id = self.session_id,
                    vm_id = %self.vm_id,
                    error = %e,
                    "Cgroup cleanup failed — manual removal may be required"
                );
            }
        }

        // Release IPAM slot so the IP can be re-issued to future VMs.
        if let Some(ip) = self.host_ip.take() {
            ipam.release(ip);
        }

        // Remove DM thin snapshot.
        if let Some(name) = self.snapshot_name.take() {
            if let Err(e) = storage.delete_snapshot(&name).await {
                warn!(
                    session_id = self.session_id,
                    vm_id = %self.vm_id,
                    error = %e,
                    "Snapshot cleanup failed — manual dmsetup remove may be required"
                );
            }
        }
    }
}
