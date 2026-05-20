pub mod cgroup;
pub mod storage;
pub mod network;
pub mod observability;
pub mod state;
pub mod firecracker_api;

use std::ffi::CString;
use std::os::unix::fs::MetadataExt;
use std::process::{Command, Child};
use std::io;
use std::path::{Path, PathBuf};
use std::os::unix::process::CommandExt;
use tracing::{instrument, info, warn};
use self::cgroup::{VmCgroup, CgroupManager, CpusetAllocator, NaiveCpusetAllocator, NumaTopology};
use self::storage::{StorageManager, MetadataDrive};
use self::network::{IpAm, TapDevice, EbpfProgram};
use self::state::{VmState, VmStateMachine, OrchestratorError};
use self::firecracker_api::{
    FirecrackerClient, MachineConfig, BootSource, Drive, NetworkInterface,
};

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
    /// Jailer chroot root — `/srv/jailer/firecracker/<vm-id>/root`.
    /// Retained so ADR-012 Phase B adoption scan can re-attach to surviving jails.
    pub jail_dir: PathBuf,
    pub process: Option<Child>,
}

pub struct Orchestrator {
    cgroup_manager: CgroupManager,
    storage_manager: StorageManager,
    ipam: IpAm,
    cpuset_allocator: Box<dyn CpusetAllocator>,
    numa_topology: NumaTopology,
    /// Path to the `jailer` binary (ships alongside Firecracker).
    jailer_path: PathBuf,
    /// Path to the `firecracker` binary; passed to jailer as `--exec-file`.
    firecracker_path: PathBuf,
    /// Guest kernel image; hard-linked into each VM's chroot on launch.
    kernel_path: PathBuf,
    /// Base directory under which the jailer creates per-VM chroots.
    /// Layout: `<chroot_base_dir>/firecracker/<vm-id>/root/`.
    chroot_base_dir: PathBuf,
    /// UID/GID the jailer drops to before exec'ing Firecracker (ADR-009).
    jailer_uid: u32,
    jailer_gid: u32,
}

impl Orchestrator {
    pub fn new(
        cgroup_root: &str,
        pool_name: &str,
        base_ip: u32,
        jailer_path: &str,
        firecracker_path: &str,
        kernel_path: &str,
        chroot_base_dir: &str,
        jailer_uid: u32,
        jailer_gid: u32,
    ) -> io::Result<Self> {
        info!(cgroup_root, pool_name, base_ip, "Initializing Orchestrator");
        Ok(Self {
            cgroup_manager: CgroupManager::new(cgroup_root)?,
            storage_manager: StorageManager::new(pool_name),
            ipam: IpAm::new(base_ip),
            cpuset_allocator: Box::new(NaiveCpusetAllocator),
            numa_topology: NumaTopology::new(),
            jailer_path: PathBuf::from(jailer_path),
            firecracker_path: PathBuf::from(firecracker_path),
            kernel_path: PathBuf::from(kernel_path),
            chroot_base_dir: PathBuf::from(chroot_base_dir),
            jailer_uid,
            jailer_gid,
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

        let (rootfs, snapshot_id) = self.storage_manager.create_snapshot(1, &config.id).await
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;
        // Record both identifiers for cleanup before any subsequent fallible operation.
        ctx.snapshot_name = Some(config.id.clone());
        ctx.snapshot_internal_id = Some(snapshot_id);

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

        // 4. Launch VMM via jailer (Spec-006 / ADR-009)
        sm.transition_to(VmState::LaunchingVMM)
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e))?;

        // Compute and record jail_dir before touching the filesystem so that cleanup()
        // can remove a partially-created jail if setup_jail fails midway.
        let jail_dir = self.chroot_base_dir
            .join("firecracker")
            .join(&config.id)
            .join("root");
        ctx.jail_dir = Some(jail_dir.clone());

        self.setup_jail(&jail_dir, &rootfs)
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;

        // SAFETY: pre_exec runs post-fork/pre-exec in the child. prctl(2) is
        // async-signal-safe. No allocations, no panics, no Rust runtime calls.
        let child = unsafe {
            Command::new(&self.jailer_path)
                .args([
                    "--exec-file", self.firecracker_path.to_str().unwrap_or_default(),
                    "--id", config.id.as_str(),
                    "--uid", &self.jailer_uid.to_string(),
                    "--gid", &self.jailer_gid.to_string(),
                    "--chroot-base-dir", self.chroot_base_dir.to_str().unwrap_or_default(),
                    "--",
                    "--api-sock", "/run/firecracker.socket",
                ])
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

        // 5. Configure and boot the VMM via the Firecracker API (Spec-006)
        sm.transition_to(VmState::ConfiguringVMM)
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e))?;

        // The API socket lives at <jail_root>/run/firecracker.socket from the host.
        let api_socket = jail_dir.join("run/firecracker.socket");
        let fc = FirecrackerClient::new(&api_socket);

        fc.wait_for_socket().await
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;

        fc.put_machine_config(&MachineConfig {
            vcpu_count: config.cpu_count,
            mem_size_mib: config.mem_size_mib,
            smt: false,
        })
        .await
        .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;

        fc.put_boot_source(&BootSource {
            // /vmlinux is the kernel hard-linked into the chroot by setup_jail.
            kernel_image_path: "/vmlinux".to_string(),
            boot_args: "console=ttyS0 reboot=k panic=1 pci=off nomodules".to_string(),
        })
        .await
        .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;

        // Rootfs block device is at /dev/rootfs inside the chroot (mknod'd by setup_jail).
        fc.put_drive(&Drive {
            drive_id: "rootfs".to_string(),
            path_on_host: "/dev/rootfs".to_string(),
            is_root_device: true,
            is_read_only: false,
        })
        .await
        .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;

        // Create the metadata drive (ephemeral ext4 image) and place it inside the chroot.
        let metadata_host_path = jail_dir.join("metadata.ext4");
        let staging_dir = jail_dir.join("metadata-staging");
        std::fs::create_dir_all(&staging_dir)
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;
        MetadataDrive::create(&metadata_host_path, &staging_dir)
            .await
            .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;

        fc.put_drive(&Drive {
            drive_id: "metadata".to_string(),
            // Path is relative to the chroot root.
            path_on_host: "/metadata.ext4".to_string(),
            is_root_device: false,
            is_read_only: false,
        })
        .await
        .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;

        fc.put_network_interface(&NetworkInterface {
            iface_id: "eth0".to_string(),
            host_dev_name: tap_name.clone(),
        })
        .await
        .map_err(|e| OrchestratorError::new(session_id, sm.current_state(), e.to_string()))?;

        fc.instance_start()
            .await
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
            jail_dir,
            process: ctx.process.take(),
        })
    }

    /// Sets up the jailer chroot directory for a VM.
    ///
    /// Creates the directory tree, hard-links the kernel image in, and creates a
    /// block-device node for the rootfs DM device so that Firecracker (which runs
    /// chrooted) can open it by path. Requires `CAP_MKNOD` for the device node.
    fn setup_jail(&self, jail_dir: &Path, rootfs_dev: &Path) -> io::Result<()> {
        std::fs::create_dir_all(jail_dir.join("run"))?;
        std::fs::create_dir_all(jail_dir.join("dev"))?;

        // Hard-link the kernel into the chroot root to avoid a full copy.
        // Falls back to copy if the kernel lives on a different filesystem.
        let kernel_dest = jail_dir.join("vmlinux");
        std::fs::hard_link(&self.kernel_path, &kernel_dest)
            .or_else(|_| std::fs::copy(&self.kernel_path, &kernel_dest).map(|_| ()))?;

        // Mirror the rootfs DM device inside the chroot as /dev/rootfs.
        // mknod(2) requires CAP_MKNOD; the orchestrator must hold this capability.
        let rdev = std::fs::metadata(rootfs_dev)?.rdev();
        let dev_node = jail_dir.join("dev/rootfs");
        let c_path = CString::new(
            dev_node.to_str()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "jail path is not valid UTF-8"))?,
        )
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let ret = unsafe { libc::mknod(c_path.as_ptr(), libc::S_IFBLK | 0o600, rdev as libc::dev_t) };
        if ret != 0 {
            return Err(io::Error::last_os_error());
        }

        info!(?jail_dir, ?rootfs_dev, "Jail directory ready");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Provisioning context — tracks non-RAII resources for rollback
// ---------------------------------------------------------------------------

/// Tracks resources allocated during VM provisioning.
///
/// `TapDevice` and `EbpfProgram` have RAII cleanup via Drop so they are held
/// as local variables in `provision_vm`. The resources here require explicit
/// async teardown (DM snapshot, IPAM bitset, cgroup hierarchy, VMM process,
/// jail directory).
///
/// Each field is `Option` so that `cleanup()` is idempotent: each resource is
/// released via `take()` exactly once even if `cleanup()` is called repeatedly
/// or if provisioning failed before the resource was allocated.
struct ProvisioningContext {
    session_id: u64,
    vm_id: String,
    /// DM thin snapshot name; same as vm_id, used for `dmsetup remove`.
    snapshot_name: Option<String>,
    /// DM thin-pool internal device ID; used for `dmsetup message … delete <id>`.
    /// Must be set alongside `snapshot_name` — skipping it leaks a pool metadata slot.
    snapshot_internal_id: Option<u32>,
    /// IPAM bitset slot (host-order IP); released via `IpAm::release`.
    host_ip: Option<u32>,
    /// cgroup hierarchy for the VM; deleted via `VmCgroup::delete`.
    cgroup: Option<VmCgroup>,
    /// Jailer chroot root; removed via `fs::remove_dir_all` on rollback.
    jail_dir: Option<PathBuf>,
    /// Firecracker process; killed on cleanup to avoid orphans.
    process: Option<Child>,
}

impl ProvisioningContext {
    fn new(session_id: u64, vm_id: &str) -> Self {
        Self {
            session_id,
            vm_id: vm_id.to_string(),
            snapshot_name: None,
            snapshot_internal_id: None,
            host_ip: None,
            cgroup: None,
            jail_dir: None,
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
        // Both identifiers must be present; if either is missing the snapshot was never
        // fully created and there is nothing to clean up.
        if let (Some(name), Some(id)) = (self.snapshot_name.take(), self.snapshot_internal_id.take()) {
            if let Err(e) = storage.delete_snapshot(&name, id).await {
                warn!(
                    session_id = self.session_id,
                    vm_id = %self.vm_id,
                    error = %e,
                    "Snapshot cleanup failed — manual dmsetup remove may be required"
                );
            }
        }

        // Remove jail directory tree last — contains only device nodes and file links,
        // so its removal has no kernel resource implications.
        if let Some(jail_dir) = self.jail_dir.take() {
            if let Err(e) = std::fs::remove_dir_all(&jail_dir) {
                warn!(
                    session_id = self.session_id,
                    vm_id = %self.vm_id,
                    error = %e,
                    "Jail directory cleanup failed — manual removal may be required"
                );
            }
        }
    }
}
