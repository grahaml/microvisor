use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use tracing::{instrument, info};
use crate::vmm::observability::SyscallAuditor;

pub struct StorageManager {
    pool_name: String,
    next_device_id: AtomicU32, // Simplified ID management for POC
}

impl StorageManager {
    pub fn new(pool_name: &str) -> Self {
        Self {
            pool_name: pool_name.to_string(),
            next_device_id: AtomicU32::new(100), // Start IDs at 100
        }
    }

    /// Creates a CoW snapshot of a base image using the 4-step DM sequence.
    #[instrument(skip(self))]
    pub async fn create_snapshot(&self, base_image_internal_id: u32, snapshot_name: &str) -> io::Result<PathBuf> {
        let snapshot_internal_id = self.next_device_id.fetch_add(1, Ordering::SeqCst);
        let pool_name = self.pool_name.clone();
        let snapshot_name_owned = snapshot_name.to_string();

        info!(snapshot_name, snapshot_internal_id, "Creating DM thin snapshot");

        SyscallAuditor::spawn_blocking("dm_snapshot_create", move || {
            // Step 1: DM_TARGET_MSG on the pool -> create_snap
            // We use dmsetup for the message and table load to ensure the complex
            // buffer packing is correct for the POC, as per SME review §1.1.
            let status = Command::new("dmsetup")
                .arg("message")
                .arg(&pool_name)
                .arg("0")
                .arg(format!("create_snap {} {}", snapshot_internal_id, base_image_internal_id))
                .status()?;
            
            if !status.success() {
                return Err(io::Error::new(io::ErrorKind::Other, "dm_target_msg failed"));
            }

            // Step 2: DM_DEV_CREATE
            let status = Command::new("dmsetup")
                .arg("create")
                .arg(&snapshot_name_owned)
                .arg("--notable")
                .status()?;
            
            if !status.success() {
                return Err(io::Error::new(io::ErrorKind::Other, "dm_dev_create failed"));
            }

            // Step 3: DM_TABLE_LOAD
            // Sector range should match the base image size. Hardcoded for POC.
            let table = format!("0 20971520 thin /dev/mapper/{} {}", pool_name, snapshot_internal_id);
            let status = Command::new("dmsetup")
                .arg("load")
                .arg(&snapshot_name_owned)
                .arg("--table")
                .arg(table)
                .status()?;
            
            if !status.success() {
                return Err(io::Error::new(io::ErrorKind::Other, "dm_table_load failed"));
            }

            // Step 4: DM_DEV_SUSPEND (resume)
            let status = Command::new("dmsetup")
                .arg("resume")
                .arg(&snapshot_name_owned)
                .status()?;
            
            if !status.success() {
                return Err(io::Error::new(io::ErrorKind::Other, "dm_dev_resume failed"));
            }

            Ok(PathBuf::from(format!("/dev/mapper/{}", snapshot_name_owned)))
        }).await?
    }

    #[instrument(skip(self))]
    pub async fn delete_snapshot(&self, snapshot_name: &str) -> io::Result<()> {
        let snapshot_name_owned = snapshot_name.to_string();
        info!(snapshot_name, "Deleting DM snapshot");

        SyscallAuditor::spawn_blocking("dm_snapshot_delete", move || {
            let status = Command::new("dmsetup")
                .arg("remove")
                .arg(&snapshot_name_owned)
                .status()?;
            
            if !status.success() {
                return Err(io::Error::new(io::ErrorKind::Other, "dm_dev_remove failed"));
            }
            Ok(())
        }).await?
    }
}

pub struct MetadataDrive {
    path: PathBuf,
}

impl MetadataDrive {
    /// Creates an ephemeral ext4 metadata drive by populating it from a staging directory.
    /// Uses fallocate + mke2fs -d (declarative, no-mount).
    #[instrument]
    pub async fn create(path: &Path, staging_dir: &Path) -> io::Result<Self> {
        info!(?path, ?staging_dir, "Creating metadata drive");
        
        let path_owned = path.to_path_buf();
        let staging_owned = staging_dir.to_path_buf();

        SyscallAuditor::spawn_blocking("metadata_drive_create", move || {
            // 1. Create a 1MB sparse file
            let output = Command::new("fallocate")
                .arg("-l")
                .arg("1M")
                .arg(&path_owned)
                .output()?;
            
            if !output.status.success() {
                return Err(io::Error::new(io::ErrorKind::Other, 
                    format!("fallocate failed: {}", String::from_utf8_lossy(&output.stderr))));
            }

            // 2. Format and populate with mke2fs -d (requires e2fsprogs >= 1.43)
            let output = Command::new("mke2fs")
                .arg("-t")
                .arg("ext4")
                .arg("-L")
                .arg("METADATA")
                .arg("-d")
                .arg(&staging_owned)
                .arg(&path_owned)
                .output()?;

            if !output.status.success() {
                return Err(io::Error::new(io::ErrorKind::Other, 
                    format!("mke2fs failed: {}", String::from_utf8_lossy(&output.stderr))));
            }

            Ok(())
        }).await??;

        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
