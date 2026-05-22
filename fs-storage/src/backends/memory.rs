use std::collections::HashMap;
use std::sync::Arc;
use std::io::Cursor;
use tokio::sync::RwLock;
use async_trait::async_trait;
use tokio::io::AsyncRead;

use crate::traits::StorageBackend;
use crate::types::{ArtifactFormat, ContentHash, StoreError};

#[derive(Clone)]
pub struct InMemoryBackend {
    blobs: Arc<RwLock<HashMap<(ContentHash, ArtifactFormat), Vec<u8>>>>,
    meta: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl InMemoryBackend {
    pub fn new() -> Self {
        Self {
            blobs: Arc::new(RwLock::new(HashMap::new())),
            meta: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StorageBackend for InMemoryBackend {
    async fn has_blob(&self, hash: &ContentHash, format: ArtifactFormat) -> Result<bool, StoreError> {
        let blobs = self.blobs.read().await;
        Ok(blobs.contains_key(&(hash.clone(), format)))
    }

    async fn put_blob(
        &self,
        hash: &ContentHash,
        format: ArtifactFormat,
        reader: &mut (dyn AsyncRead + Send + Unpin),
    ) -> Result<(), StoreError> {
        let mut data = Vec::new();
        tokio::io::copy(reader, &mut data).await?;
        let mut blobs = self.blobs.write().await;
        blobs.insert((hash.clone(), format), data);
        Ok(())
    }

    async fn get_blob(
        &self,
        hash: &ContentHash,
        format: ArtifactFormat,
    ) -> Result<Box<dyn AsyncRead + Send + Unpin>, StoreError> {
        let blobs = self.blobs.read().await;
        match blobs.get(&(hash.clone(), format)) {
            Some(data) => {
                let cursor = Cursor::new(data.clone());
                Ok(Box::new(cursor))
            }
            None => Err(StoreError::NotFound),
        }
    }

    async fn delete_blob(&self, hash: &ContentHash, format: ArtifactFormat) -> Result<(), StoreError> {
        let mut blobs = self.blobs.write().await;
        blobs.remove(&(hash.clone(), format));
        Ok(())
    }

    async fn put_meta(&self, key: &str, value: &[u8]) -> Result<(), StoreError> {
        let mut meta = self.meta.write().await;
        meta.insert(key.to_string(), value.to_vec());
        Ok(())
    }

    async fn get_meta(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let meta = self.meta.read().await;
        Ok(meta.get(key).cloned())
    }

    async fn list_meta(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        let meta = self.meta.read().await;
        let keys = meta
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        Ok(keys)
    }

    async fn delete_meta(&self, key: &str) -> Result<(), StoreError> {
        let mut meta = self.meta.write().await;
        meta.remove(key);
        Ok(())
    }
}
