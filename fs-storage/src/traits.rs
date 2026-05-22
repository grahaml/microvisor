use async_trait::async_trait;
use tokio::io::AsyncRead;

use crate::types::{ArtifactFormat, ContentHash, StoreError};

#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Blob operations — format-aware, content-hash addressed
    async fn has_blob(&self, hash: &ContentHash, format: ArtifactFormat) -> Result<bool, StoreError>;
    
    async fn put_blob(
        &self, 
        hash: &ContentHash, 
        format: ArtifactFormat, 
        reader: &mut (dyn AsyncRead + Send + Unpin)
    ) -> Result<(), StoreError>;
    
    async fn get_blob(
        &self, 
        hash: &ContentHash, 
        format: ArtifactFormat
    ) -> Result<Box<dyn AsyncRead + Send + Unpin>, StoreError>;
    
    async fn delete_blob(&self, hash: &ContentHash, format: ArtifactFormat) -> Result<(), StoreError>;

    /// Metadata operations — string-keyed, opaque bytes
    async fn put_meta(&self, key: &str, value: &[u8]) -> Result<(), StoreError>;
    
    async fn get_meta(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError>;
    
    async fn list_meta(&self, prefix: &str) -> Result<Vec<String>, StoreError>;
    
    async fn delete_meta(&self, key: &str) -> Result<(), StoreError>;
}
