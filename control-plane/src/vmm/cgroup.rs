use std::fs;
use std::path::{Path, PathBuf};
use std::io::{self, Write};
use tracing::{instrument, info};
use crate::vmm::observability::SyscallAuditor;

pub struct CgroupManager {
    root_path: PathBuf,
}

impl CgroupManager {
    #[instrument]
    pub fn new<P: AsRef<Path> + std::fmt::Debug>(root: P) -> io::Result<Self> {
        info!(?root, "Initializing CgroupManager");
        let root_path = root.as_ref().to_path_buf();
        if !root_path.exists() {
            fs::create_dir_all(&root_path)?;
        }
        
        // Ensure controllers are enabled in the root if it's a sub-cgroup
        // This might require being in the parent cgroup or having permissions
        Ok(Self { root_path })
    }

    #[instrument(skip(self))]
    pub fn create_vm_cgroup(&self, vm_id: &str) -> io::Result<VmCgroup> {
        info!(vm_id, "Creating VM cgroup");
        let path = self.root_path.join(format!("vm-{}", vm_id));
        if !path.exists() {
            fs::create_dir(&path)?;
        }
        Ok(VmCgroup { path })
    }
}

pub struct VmCgroup {
    path: PathBuf,
}

impl VmCgroup {
    #[instrument(skip(self))]
    pub fn set_cpuset(&self, cpus: &str, mems: &str) -> io::Result<()> {
        info!(cpus, mems, "Setting cpuset");
        SyscallAuditor::audit("fs::write(cpuset.cpus)", || fs::write(self.path.join("cpuset.cpus"), cpus))?;
        SyscallAuditor::audit("fs::write(cpuset.mems)", || fs::write(self.path.join("cpuset.mems"), mems))?;
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn set_memory_limit(&self, max_bytes: u64) -> io::Result<()> {
        info!(max_bytes, "Setting memory limits");
        SyscallAuditor::audit("fs::write(memory.max)", || fs::write(self.path.join("memory.max"), max_bytes.to_string()))?;
        SyscallAuditor::audit("fs::write(memory.swap.max)", || fs::write(self.path.join("memory.swap.max"), "0"))?;
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn add_process(&self, pid: u32) -> io::Result<()> {
        info!(pid, "Adding process to cgroup");
        SyscallAuditor::audit("fs::OpenOptions::open(cgroup.procs)", || {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .open(self.path.join("cgroup.procs"))?;
            writeln!(file, "{}", pid)
        })?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
