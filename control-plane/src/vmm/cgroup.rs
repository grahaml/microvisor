use std::fs;
use std::path::{Path, PathBuf};
use std::io::{self, Write};
use std::collections::HashSet;
use tracing::{instrument, info, warn};
use crate::vmm::observability::SyscallAuditor;

pub trait CpusetAllocator: Send + Sync {
    fn allocate_cpus(&self, request_cpus: &str) -> io::Result<String>;
}

/// Discovers NUMA topology to ensure correct memory pinning.
pub struct NumaTopology {
    sysfs_root: PathBuf,
}

impl NumaTopology {
    pub fn new() -> Self {
        Self {
            sysfs_root: PathBuf::from("/sys"),
        }
    }

    #[cfg(test)]
    pub fn with_root(root: PathBuf) -> Self {
        Self { sysfs_root: root }
    }

    /// Returns the correct cpuset.mems string for a requested node.
    /// If the host is single-NUMA, it always returns "0".
    pub fn get_mems_string(&self, requested_node: &str) -> io::Result<String> {
        let nodes_dir = self.sysfs_root.join("devices/system/node");
        
        // Count entries starting with 'node' in sysfs
        let mut node_count = 0;
        if nodes_dir.exists() {
            for entry in fs::read_dir(nodes_dir)? {
                let entry = entry?;
                if entry.file_name().to_string_lossy().starts_with("node") {
                    node_count += 1;
                }
            }
        }

        if node_count <= 1 {
            info!("Single NUMA node detected, forcing cpuset.mems to 0");
            Ok("0".to_string())
        } else {
            // In a multi-node system, we'd validate the requested node exists.
            // For now, we trust the config but could add validation here.
            Ok(requested_node.to_string())
        }
    }
}

pub struct NaiveCpusetAllocator;

impl CpusetAllocator for NaiveCpusetAllocator {
    fn allocate_cpus(&self, request_cpus: &str) -> io::Result<String> {
        Ok(request_cpus.to_string())
    }
}

pub struct SiblingAwareCpusetAllocator {
    sysfs_root: PathBuf,
}

impl SiblingAwareCpusetAllocator {
    pub fn new() -> Self {
        Self {
            sysfs_root: PathBuf::from("/sys"),
        }
    }

    #[cfg(test)]
    pub fn with_root(root: PathBuf) -> Self {
        Self { sysfs_root: root }
    }

    fn discover_smt_siblings(&self, requested_cpus: &str) -> io::Result<Vec<u32>> {
        let mut full_set = HashSet::new();
        for cpu_str in requested_cpus.split(',') {
            if cpu_str.is_empty() { continue; }
            let cpu: u32 = cpu_str.parse().map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
            let sibling_path = self.sysfs_root.join(format!("devices/system/cpu/cpu{}/topology/thread_siblings_list", cpu));
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
}

impl CpusetAllocator for SiblingAwareCpusetAllocator {
    fn allocate_cpus(&self, request_cpus: &str) -> io::Result<String> {
        let siblings = self.discover_smt_siblings(request_cpus)?;
        let siblings_str: Vec<String> = siblings.into_iter().map(|c| c.to_string()).collect();
        Ok(siblings_str.join(","))
    }
}

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
