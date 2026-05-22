---
name: build-system-store-client
status: draft
author: Graham Losee
date: 2026-05-21
derives_from: build-system-tad
tags: [build-system, store-client, fs-storage, specs]
---

# Spec 005: Store Client

## Overview
The `StoreClient` provides an abstraction over `fs-storage`, managing the two-level addressing system (input hash -> content hash) and handling the publication and retrieval of layer artifacts.

## Data Structures
```rust
pub enum ArtifactFormat {
    Deploy,
    BuildCache,
}

pub trait StoreClient {
    fn resolve_input_hash(&self, hash: &str) -> Result<Option<String>, StoreError>;
    fn pull_layer(&self, content_hash: &str, format: ArtifactFormat, target: &Path) -> Result<LayerMetadata, StoreError>;
    fn publish_layer(&self, artifacts: &LayerArtifacts, meta: &LayerMetadata) -> Result<(), StoreError>;
    fn publish_manifest(&self, manifest: &ReleaseManifest) -> Result<String, StoreError>;
    fn pull_manifest(&self, id_or_tag: &str) -> Result<ReleaseManifest, StoreError>;
}
```

## Architectural Considerations
- **Idempotency**: `publish_layer` should check if the `content_hash` already exists in `fs-storage`. If it does, the operation is a no-op.
- **Dual Formats**: The store holds both the deploy and build-cache artifacts under the same content hash. `pull_layer` only retrieves the requested format to save network/disk I/O.
- **Local vs Remote**: The initial implementation targets a local filesystem API, but the trait boundary enables a seamless switch to a remote HTTP/gRPC client in the future.
