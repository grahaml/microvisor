use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::fs::{self, File};
use tokio::io::{self, AsyncRead, AsyncWriteExt};
use uuid::Uuid;

use crate::traits::StorageBackend;
use crate::types::{ArtifactFormat, ContentHash, StoreError};

pub struct LocalJsonBackend {
    root: PathBuf,
}

impl LocalJsonBackend {
    pub async fn new(root: impl AsRef<Path>) -> Result<Self, StoreError> {
        let root = root.as_ref().to_path_buf();
        
        let dirs = [
            root.join("objects"),
            root.join("manifests"),
            root.join("meta").join("layers"),
            root.join("meta").join("index"),
            root.join("meta").join("builds"),
            root.join("refs").join("tags"),
            root.join("tmp"),
        ];
        
        for dir in &dirs {
            fs::create_dir_all(dir).await?;
        }

        let tmp_dir = root.join("tmp");
        if let Ok(mut entries) = fs::read_dir(&tmp_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Ok(ft) = entry.file_type().await {
                    if ft.is_file() {
                        let _ = fs::remove_file(entry.path()).await;
                    }
                }
            }
        }

        Ok(Self { root })
    }

    fn blob_path(&self, hash: &ContentHash, format: ArtifactFormat) -> PathBuf {
        let hash_str = hash.to_string();
        let prefix = &hash_str[0..2];
        let suffix = &hash_str[2..];
        self.root.join("objects").join(prefix).join(format!("{}.{}", suffix, format.extension()))
    }
    
    fn get_meta_file_path(&self, key: &str) -> PathBuf {
        if let Some(name) = key.strip_prefix("manifests/") {
            self.root.join("manifests").join(format!("{}.json", name))
        } else if let Some(name) = key.strip_prefix("tags/") {
            self.root.join("refs").join("tags").join(name)
        } else if let Some(name) = key.strip_prefix("layers/") {
            self.root.join("meta").join("layers").join(format!("{}.json", name))
        } else if let Some(name) = key.strip_prefix("builds/") {
            self.root.join("meta").join("builds").join(format!("{}.json", name))
        } else if let Some(name) = key.strip_prefix("index/") {
            self.root.join("meta").join("index").join(name)
        } else {
            self.root.join("meta").join(key)
        }
    }
}

#[async_trait]
impl StorageBackend for LocalJsonBackend {
    async fn has_blob(&self, hash: &ContentHash, format: ArtifactFormat) -> Result<bool, StoreError> {
        let path = self.blob_path(hash, format);
        match fs::metadata(&path).await {
            Ok(m) => Ok(m.is_file()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(StoreError::Io(e)),
        }
    }

    async fn put_blob(
        &self,
        hash: &ContentHash,
        format: ArtifactFormat,
        reader: &mut (dyn AsyncRead + Send + Unpin),
    ) -> Result<(), StoreError> {
        let final_path = self.blob_path(hash, format);
        if final_path.exists() {
            return Ok(());
        }

        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let tmp_path = self.root.join("tmp").join(Uuid::new_v4().to_string());
        let mut tmp_file = File::create(&tmp_path).await?;
        
        io::copy(reader, &mut tmp_file).await?;
        tmp_file.sync_all().await?;
        drop(tmp_file);

        match fs::rename(&tmp_path, &final_path).await {
            Ok(_) => Ok(()),
            Err(e) => {
                let _ = fs::remove_file(&tmp_path).await;
                Err(StoreError::Io(e))
            }
        }
    }

    async fn get_blob(
        &self,
        hash: &ContentHash,
        format: ArtifactFormat,
    ) -> Result<Box<dyn AsyncRead + Send + Unpin>, StoreError> {
        let path = self.blob_path(hash, format);
        let file = File::open(&path).await.map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                StoreError::NotFound
            } else {
                StoreError::Io(e)
            }
        })?;
        Ok(Box::new(file))
    }

    async fn delete_blob(&self, hash: &ContentHash, format: ArtifactFormat) -> Result<(), StoreError> {
        let path = self.blob_path(hash, format);
        match fs::remove_file(&path).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StoreError::Io(e)),
        }
    }

    async fn put_meta(&self, key: &str, value: &[u8]) -> Result<(), StoreError> {
        let final_path = self.get_meta_file_path(key);
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        
        let tmp_path = self.root.join("tmp").join(Uuid::new_v4().to_string());
        let mut tmp_file = File::create(&tmp_path).await?;
        tmp_file.write_all(value).await?;
        tmp_file.sync_all().await?;
        drop(tmp_file);
        
        match fs::rename(&tmp_path, &final_path).await {
            Ok(_) => Ok(()),
            Err(e) => {
                let _ = fs::remove_file(&tmp_path).await;
                Err(StoreError::Io(e))
            }
        }
    }

    async fn get_meta(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let path = self.get_meta_file_path(key);
        match fs::read(&path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(StoreError::Io(e)),
        }
    }

    async fn list_meta(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        let dir_path = if prefix == "manifests" || prefix.starts_with("manifests/") {
            self.root.join("manifests")
        } else if prefix == "tags" || prefix.starts_with("tags/") {
            self.root.join("refs").join("tags")
        } else {
            self.root.join("meta").join(prefix)
        };

        let mut keys = Vec::new();
        let mut entries = match fs::read_dir(&dir_path).await {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(keys),
            Err(e) => return Err(StoreError::Io(e)),
        };

        let base_prefix = prefix.trim_end_matches('/');

        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_file() {
                let file_name = entry.file_name().to_string_lossy().to_string();
                
                let name = if file_name.ends_with(".json") {
                    file_name.strip_suffix(".json").unwrap().to_string()
                } else {
                    file_name
                };

                let key = if prefix.starts_with("manifests") {
                    format!("manifests/{}", name)
                } else if prefix.starts_with("tags") {
                    format!("tags/{}", name)
                } else {
                    format!("{}/{}", base_prefix, name)
                };

                keys.push(key);
            }
        }
        Ok(keys)
    }

    async fn delete_meta(&self, key: &str) -> Result<(), StoreError> {
        let path = self.get_meta_file_path(key);
        match fs::remove_file(&path).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StoreError::Io(e)),
        }
    }
}
