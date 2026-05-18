use std::fs;
use std::path::{Path, PathBuf};
use std::io::{self, Write};

pub struct CgroupManager {
    root_path: PathBuf,
}

impl CgroupManager {
    pub fn new<P: AsRef<Path>>(root: P) -> io::Result<Self> {
        let root_path = root.as_ref().to_path_buf();
        if !root_path.exists() {
            fs::create_dir_all(&root_path)?;
        }
        
        // Ensure controllers are enabled in the root if it's a sub-cgroup
        // This might require being in the parent cgroup or having permissions
        Ok(Self { root_path })
    }

    pub fn create_vm_cgroup(&self, vm_id: &str) -> io::Result<VmCgroup> {
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
    pub fn set_cpuset(&self, cpus: &str, mems: &str) -> io::Result<()> {
        fs::write(self.path.join("cpuset.cpus"), cpus)?;
        fs::write(self.path.join("cpuset.mems"), mems)?;
        Ok(())
    }

    pub fn set_memory_limit(&self, max_bytes: u64) -> io::Result<()> {
        fs::write(self.path.join("memory.max"), max_bytes.to_string())?;
        fs::write(self.path.join("memory.swap.max"), "0")?;
        Ok(())
    }

    pub fn add_process(&self, pid: u32) -> io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .open(self.path.join("cgroup.procs"))?;
        writeln!(file, "{}", pid)?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
