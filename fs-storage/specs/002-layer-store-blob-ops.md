---
name: fs-storage-blob-ops
status: draft
author: Graham Losee
date: 2026-05-21
derives_from: fs-storage-tad
tags: [fs-storage, layer-store, blob-ops, specs]
---

# Spec 002: LayerStore Blob Operations & Index

## Overview
Defines the `LayerStore` typed API over `StorageBackend` for managing blobs and resolving cache hits via the input hash index.

## Input Hash Resolution
```rust
pub async fn resolve_input_hash(&self, input_hash: &InputHash) -> Result<Option<ContentHash>, StoreError>;
async fn register_input_hash(&self, input_hash: &InputHash, content_hash: &ContentHash) -> Result<(), StoreError>;
```
- Reads/writes plain text index files mapping `InputHash` -> `ContentHash`.
- Used heavily for cache-hit lookups (< 5ms target).
- Registration is idempotent.

## Blob Operations
```rust
pub async fn has_layer(&self, content_hash: &ContentHash) -> Result<bool, StoreError>;
```
- Checks for the `Deploy` artifact. (Both artifacts are guaranteed to exist together).

```rust
pub async fn pull_layer(&self, content_hash: &ContentHash, format: ArtifactFormat) -> Result<Box<dyn AsyncRead + Send + Unpin>, StoreError>;
```
- Returns a streaming reader from `get_blob`. Validating content hashes inline is NOT the store's responsibility on pull.

```rust
pub async fn publish_layer(
    &self,
    deploy_reader: &mut (dyn AsyncRead + Send + Unpin),
    cache_reader: &mut (dyn AsyncRead + Send + Unpin),
    meta: LayerMeta,
) -> Result<ContentHash, StoreError>;
```
- Checks if the blobs already exist at `meta.content_hash`. If so, skip writes (idempotency).
- If writing:
  - Streams `deploy_reader` and `cache_reader` to `put_blob`.
  - Computes file checksums (SHA-256 of compressed bytes) inline during the stream.
  - If uncompressed hashing was provided inline (or verified via some wrapping reader), it would throw `StoreError::HashCollision` if it doesn't match `meta.content_hash`.
  - Updates `meta` with computed `file_checksum`s.
  - Registers the `LayerMeta` as JSON via `put_meta`.
  - Calls `register_input_hash`.
