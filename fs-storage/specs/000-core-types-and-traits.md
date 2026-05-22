---
name: fs-storage-core-types
status: draft
author: Graham Losee
date: 2026-05-21
derives_from: fs-storage-tad
tags: [fs-storage, layer-store, types, traits, specs]
---

# Spec 000: Core Types and StorageBackend Trait

## Overview
Defines the foundational data types, error handling, and the abstraction boundary (`StorageBackend`) for the `fs-storage` crate.

## Hash Types
Two distinct hash spaces are used to prevent accidental substitution:
```rust
pub struct ContentHash([u8; 32]);
pub struct InputHash([u8; 32]);
```
- Both display as lowercase hex.
- `ContentHash` addresses blobs (output hash, source of truth is raw ext4 bytes).
- `InputHash` addresses cache lookup keys.

## Formats
```rust
pub enum ArtifactFormat {
    Deploy,     // .ext4.zst
    BuildCache, // .oci.tar.zst
}
```

## Data Structures
- `BuildId(String)`: UUID v4 prefixed with timestamp (e.g., `20260521T143022-{uuid4}`).
- `LayerMeta`: Provenance and hash info (input_hash, content_hash, parent_input_hash, sizes, file_checksums, build details).
- `ReleaseManifest`: Schema version, manifest_id, BuildMetadata, list of ManifestLayer, workload metadata.
- `ManifestLayer`: Name, order, input_hash, content_hash, sizes, parent.
- `BuildCatalogEntry`: build_id, release_name, manifest_hash, timestamp, spec_hash, layers.
- `BuildFilter`: Query parameters (tag, name prefix, layer hash, time range).

## Errors
```rust
pub enum StoreError {
    NotFound,
    HashCollision { hash: ContentHash },
    InvalidTagName(String),
    InvalidHash(String),
    Io(std::io::Error),
    Serialization(serde_json::Error),
}
```

## StorageBackend Trait
```rust
#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn has_blob(&self, hash: &ContentHash, format: ArtifactFormat) -> Result<bool, StoreError>;
    async fn put_blob(&self, hash: &ContentHash, format: ArtifactFormat, reader: &mut (dyn AsyncRead + Send + Unpin)) -> Result<(), StoreError>;
    async fn get_blob(&self, hash: &ContentHash, format: ArtifactFormat) -> Result<Box<dyn AsyncRead + Send + Unpin>, StoreError>;
    async fn delete_blob(&self, hash: &ContentHash, format: ArtifactFormat) -> Result<(), StoreError>;
    
    async fn put_meta(&self, key: &str, value: &[u8]) -> Result<(), StoreError>;
    async fn get_meta(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError>;
    async fn list_meta(&self, prefix: &str) -> Result<Vec<String>, StoreError>;
    async fn delete_meta(&self, key: &str) -> Result<(), StoreError>;
}
```
