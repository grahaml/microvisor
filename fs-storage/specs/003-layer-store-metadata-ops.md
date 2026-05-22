---
name: fs-storage-metadata-ops
status: draft
author: Graham Losee
date: 2026-05-21
derives_from: fs-storage-tad
tags: [fs-storage, layer-store, metadata-ops, specs]
---

# Spec 003: LayerStore Manifest & Catalog Operations

## Overview
Defines the manifest management, build catalog queries, and mutable tag references for `LayerStore`.

## Manifest Operations
```rust
pub async fn publish_manifest(&self, manifest: ReleaseManifest) -> Result<ContentHash, StoreError>;
pub async fn get_manifest(&self, hash: &ContentHash) -> Result<ReleaseManifest, StoreError>;
```
- **Canonical JSON:** Serialised via `serde_json` ensuring `BTreeMap` key ordering and no trailing whitespace.
- Computes `ContentHash` of the JSON bytes.
- Stores under `manifests/{hash}.json`.
- Writes a `BuildCatalogEntry` to `meta/builds/{build_id}.json`.

## Catalog Operations
```rust
pub async fn list_builds(&self, filter: BuildFilter) -> Result<Vec<BuildCatalogEntry>, StoreError>;
pub async fn get_build(&self, build_id: &BuildId) -> Result<BuildCatalogEntry, StoreError>;
```
- Scans `meta/builds/`, deserialises JSON entries, filters in-process.
- Sorts by `build_timestamp` descending.

## Tags
```rust
pub async fn create_tag(&self, name: &str, manifest_hash: &ContentHash) -> Result<(), StoreError>;
pub async fn update_tag(&self, name: &str, manifest_hash: &ContentHash) -> Result<(), StoreError>;
pub async fn get_tag(&self, name: &str) -> Result<ContentHash, StoreError>;
pub async fn list_tags(&self) -> Result<Vec<(String, ContentHash)>, StoreError>;
pub async fn delete_tag(&self, name: &str) -> Result<(), StoreError>;
```
- Point to `ContentHash` (manifest hash).
- Stored as plain text in `refs/tags/{tag-name}` without trailing newline.
- Names must match `[a-z0-9][a-z0-9._-]*` (enforced on write).
- `create_tag` and `update_tag` must verify that the `manifest_hash` actually exists in the store before saving the tag.
