---
name: fs-storage-local-json-backend
status: draft
author: Graham Losee
date: 2026-05-21
derives_from: fs-storage-tad
tags: [fs-storage, layer-store, local-backend, specs]
---

# Spec 001: LocalJsonBackend and InMemoryBackend

## Overview
Implements the `StorageBackend` trait using local filesystem primitives and flat JSON files, as well as an in-memory version for testing.

## LocalJsonBackend
### On-Disk Layout
```text
{store-root}/
  objects/
    {sha256[0:2]}/
      {sha256[2:]}.ext4.zst
      {sha256[2:]}.oci.tar.zst
  manifests/
    {manifest-hash}.json
  meta/
    layers/{content-hash}.json
    index/{input-hash}
    builds/{build-id}.json
  refs/
    tags/{tag-name}
  tmp/
    {uuid}
```

### Key Behaviors
1. **Atomicity**: All writes go to `tmp/{uuid}`, followed by `fsync`, then `rename` to the final path.
2. **tmp/ Cleanup**: On `open_local()`, any files in `tmp/` are unconditionally deleted.
3. **Deduplication**: `put_blob` checks if the final path exists. If it does, the write is skipped entirely.
4. **Metadata Keys**: The `key` in metadata ops is mapped directly to a file path under `meta/` (e.g., `layers/hash` -> `meta/layers/hash.json`). Extension handling must be consistent (e.g., strip extension for `list_meta`).
5. **Directories**: Created lazily on first use with `create_dir_all`. `ErrorKind::AlreadyExists` is ignored.

## InMemoryBackend
- Used exclusively for `#[cfg(test)]`.
- State backed by `Arc<RwLock<HashMap<(ContentHash, ArtifactFormat), Vec<u8>>>>` for blobs, and a standard `HashMap<String, Vec<u8>>` for metadata.
