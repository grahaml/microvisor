use std::fs;
use std::path::{Path, PathBuf};
use std::io::{self, Write};
use std::collections::HashSet;
use tracing::{instrument, info, warn};
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
        
        Ok(Self { root_path })
    }

    #[instrument(skip(self))]
    pub async fn create_vm_cgroup(&self, vm_id: &str) -> io::Result<VmCgroup> {
        info!(vm_id, "Creating VM cgroup");
        let path = self.root_path.join(format!("vm-{}", vm_id));
        let vm_id_owned = vm_id.to_string();
        
        SyscallAuditor::spawn_blocking("cgroup_create", move || {
            if !path.exists() {
                fs::create_dir(&path)?;
            }
            Ok(VmCgroup { path, id: vm_id_owned })
        }).await?
    }
}

#[derive(Debug, Clone)]
pub struct VmCgroup {
    path: PathBuf,
    id: String,
}

impl VmCgroup {
    #[instrument(skip(self))]
    pub async fn set_cpuset(&self, cpus: &str, mems: &str) -> io::Result<()> {
        info!(cpus, mems, "Setting cpuset");
        let path = self.path.clone();
        let cpus = cpus.to_string();
        let mems = mems.to_string();

        SyscallAuditor::spawn_blocking("fs::write(cpuset.cpus)", move || {
            fs::write(path.join("cpuset.cpus"), cpus)?;
            fs::write(path.join("cpuset.mems"), mems)?;
            Ok(())
        }).await?
    }

    /// Discovers SMT siblings for a given set of CPUs and verifies they are not split.
    /// Returns the full list of logical CPUs that should be pinned.
    pub fn discover_smt_siblings(requested_cpus: &str) -> io::Result<Vec<u32>> {
        let mut full_set = HashSet::new();
        for cpu_str in requested_cpus.split(',') {
            let cpu: u32 = cpu_str.parse().map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
            let sibling_path = format!("/sys/devices/system/cpu/cpu{}/topology/thread_siblings_list", cpu);
            let siblings = fs::read_to_string(sibling_path)?;
            
            for sibling in siblings.trim().split(',') {
                let sibling_id: u32 = sibling.parse().map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                full_set.insert(sibling_id);
            }
        }
        
        let mut result: Vec<u32> = full_set.into_iter().collect();
        result.sort();
        Ok(result)
    }

    #[instrument(skip(self))]
    pub async fn set_memory_limit(&self, max_bytes: u64) -> io::Result<()> {
        info!(max_bytes, "Setting memory limits");
        let path = self.path.clone();
        SyscallAuditor::spawn_blocking("fs::write(memory.max)", move || {
            fs::write(path.join("memory.max"), max_bytes.to_string())?;
            fs::write(path.join("memory.swap.max"), "0")?;
            Ok(())
        }).await?
    }

    #[instrument(skip(self))]
    pub async fn add_process(&self, pid: u32) -> io::Result<()> {
        info!(pid, "Adding process to cgroup");
        let path = self.path.clone();
        SyscallAuditor::spawn_blocking("fs::OpenOptions::open(cgroup.procs)", move || {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .open(path.join("cgroup.procs"))?;
            writeln!(file, "{}", pid)
        }).await?
    }

    #[instrument(skip(self))]
    pub async fn delete(&self) -> io::Result<()> {
        info!("Deleting VM cgroup");
        let path = self.path.clone();
        SyscallAuditor::spawn_blocking("fs::remove_dir", move || {
            if path.exists() {
                fs::remove_dir(&path)?;
            }
            Ok(())
        }).await?
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
