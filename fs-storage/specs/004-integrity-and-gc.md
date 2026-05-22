---
name: fs-storage-integrity-and-gc
status: draft
author: Graham Losee
date: 2026-05-21
derives_from: fs-storage-tad
tags: [fs-storage, layer-store, garbage-collection, integrity, specs]
---

# Spec 004: Integrity Verification and Garbage Collection

## Overview
Defines the offline/explicit integrity checks and garbage collection routines in `LayerStore`.

## Integrity
```rust
pub struct CorruptEntry {
    pub path: std::path::PathBuf,
    pub expected_hash: ContentHash,
    pub actual_hash: ContentHash,
}

pub struct VerificationReport {
    pub valid: Vec<ContentHash>,
    pub corrupt: Vec<CorruptEntry>,
}

pub async fn verify_store(&self) -> Result<VerificationReport, StoreError>;
```
- Iterates recursively through `objects/`.
- Recomputes the SHA-256 of the compressed files on disk.
- Compares against the file path derived hash.
- Returns a structured report but does not delete corrupt files.

## Garbage Collection
```rust
pub struct GcCandidate {
    pub content_hash: ContentHash,
    pub size_deploy_bytes: u64,
    pub size_cache_bytes: u64,
}

pub struct GcReport {
    pub candidates: Vec<GcCandidate>,
    pub total_reclaimable_bytes: u64,
}

pub struct GcSummary {
    pub deleted_blobs: u64,
    pub deleted_bytes: u64,
}

pub async fn gc_candidates(&self) -> Result<GcReport, StoreError>;
pub async fn gc_execute(&self, report: &GcReport) -> Result<GcSummary, StoreError>;
```
- **Mark Phase:** Walks all tags to find referenced manifests. Then walks all manifests (tagged and untagged) to collect every `ContentHash` in their layer lists. This is the live set.
- **Sweep Phase:** Scans `objects/` directory. Any blob hash not in the live set becomes a candidate.
- `gc_candidates` performs the scan but does not delete.
- `gc_execute` deletes both `.ext4.zst` and `.oci.tar.zst` blobs, the corresponding `meta/layers/{hash}.json`, and any orphaned index entries mapping to that `ContentHash`. Manifests and tags are never deleted.
