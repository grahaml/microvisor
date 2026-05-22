---
name: fs-storage-one-pager
status: draft
author: Graham Losee
date: 2026-05-21
derives_from: build-system-prd
tags: [fs-storage, layer-store, content-addressing, storage-backend, build-system]
---

# Project One-Pager: fs-storage (Layer Store)

## The Problem
The build system produces content-addressed layer artifacts and release manifests. Something has to hold them. That "something" needs to answer three questions reliably and fast:

1. **Does this layer already exist?** (cache-hit lookup — must be < 5 ms)
2. **Give me this layer's bytes.** (pull by content address)
3. **What builds have been published?** (catalog query — list, filter, diff)

Today the answer is local disk. Tomorrow it might be S3, GCS, or a shared NAS as the fleet grows. If the build system talks directly to local filesystem primitives, every future storage migration requires rewriting the build system. The store and the builder must be decoupled from day one.

## The Solution: A Storage Abstraction with a File-Based Backing Implementation
`fs-storage` is a Rust crate that wraps a pluggable storage backend behind a clean API. The build system calls the API; it never touches a file path or a network socket directly. The local filesystem implementation ships first. Swapping in S3 means implementing one trait — the build system is untouched.

### Two Logical Concerns, One API
The store manages two distinct categories of data:

**Blob store** — raw layer artifacts, identified by content hash (output hash — the SHA-256 of the artifact's bytes). Immutable once written. Automatically deduplicated: storing an already-existing hash is a no-op. Each layer stores **two artifacts** in different formats: a deploy artifact (`ext4.zst` for Microvisor) and a build-cache artifact (`oci.tar.zst` for BuildKit cache injection). Both are stored under the same content hash with a format suffix.

**Metadata store** — release manifests, build catalog entries, layer provenance records, and the **input hash index** (maps input hashes to content hashes for cache-hit lookups). Structured JSON. Queryable by release ID, tag, content hash, input hash, or date range.

The API composes both behind a single `LayerStore` struct. Callers don't interact with blobs and metadata separately; they interact with layers and builds.

### On-Disk Layout (Local Implementation)
```
{store-root}/
  objects/
    {sha256[0:2]}/
      {sha256[2:]}.ext4.zst      ← deploy artifact (zstd-compressed ext4)
      {sha256[2:]}.oci.tar.zst   ← build-cache artifact (zstd-compressed OCI tar)
  manifests/
    {manifest-hash}.json  ← full release manifest, stored by its own content hash
  meta/
    layers/
      {sha256}.json       ← per-layer provenance: parent hash, input hash, build host, timestamp,
                             builder version, stage definition hash
    index/
      {input-hash}        ← plain text file: content hash (maps input hash → content hash)
    builds/
      {build-id}.json     ← build catalog entry: manifest hash, tag, timestamp, layer list
  refs/
    tags/
      {tag-name}          ← plain text file containing a manifest hash (mutable pointer)
```

This mirrors git's object store: the two-character prefix directory keeps directory entry counts manageable at scale. A layer with content hash `abcdef1234...` has its deploy artifact at `objects/ab/cdef1234....ext4.zst` and its build-cache artifact at `objects/ab/cdef1234....oci.tar.zst`.

The `meta/index/` directory provides the **input hash → content hash lookup** the build system needs for cache-hit decisions. The build system computes an input hash (hash of stage definition + parent hash) before building. It queries the index to find if a layer with that input hash already exists. If so, it resolves to the content hash and pulls the appropriate artifact format.

### The Backend Trait
The abstraction surface is minimal — just what the build system needs:

```rust
trait StorageBackend {
    // Blob operations (content-hash addressed, with format suffix)
    fn has_blob(&self, hash: &ContentHash, format: ArtifactFormat) -> Result<bool>;
    fn put_blob(&self, hash: &ContentHash, format: ArtifactFormat, data: &[u8]) -> Result<()>;
    fn get_blob(&self, hash: &ContentHash, format: ArtifactFormat) -> Result<Vec<u8>>;
    fn delete_blob(&self, hash: &ContentHash, format: ArtifactFormat) -> Result<()>;

    // Metadata operations
    fn put_meta(&self, key: &str, value: &[u8]) -> Result<()>;
    fn get_meta(&self, key: &str) -> Result<Option<Vec<u8>>>;
    fn list_meta(&self, prefix: &str) -> Result<Vec<String>>;
    fn delete_meta(&self, key: &str) -> Result<()>;
}
```

`LayerStore` is a typed layer on top: it composes a `StorageBackend` and exposes `publish_layer`, `pull_layer`, `resolve_input_hash`, `publish_manifest`, `list_builds`, `gc`, and so on — without any of those methods knowing whether they're talking to a disk or a bucket.

## Design Constraints

### Content Addressing is the Immutability Contract
A layer's content hash (output hash — SHA-256 of the artifact's bytes) is its identity. `put_blob` with a content hash that already exists is a no-op — the store never overwrites. Attempting to publish different bytes under an existing content hash is rejected: it signals either hash collision (impossible at SHA-256 scale) or tampering (reject and alert). The store enforces this; callers cannot bypass it.

### Two Hash Spaces: Input Hash and Content Hash
The store tracks two distinct hash spaces. **Content hashes** (output hashes) address blobs — they are the SHA-256 of the artifact's bytes and serve as the immutability key. **Input hashes** are computed by the build system from stage definitions and serve as cache-lookup keys. The store maintains an index (`meta/index/`) mapping input hashes to content hashes. A cache-hit lookup is: `resolve_input_hash(input_hash) → Option<content_hash>`, then `pull_layer(content_hash, format)`.

### Metadata is Append-Friendly, Not Append-Only
Layer blobs are truly immutable once written. Metadata is different — tags can be moved, build catalog entries can be annotated, GC marks can be set. The metadata store is mutable but auditable: all writes are logged with a timestamp and caller identity. No silent overwrites.

### Local Implementation Must Not Leak Into the API
The `LocalBackend` implementation may use filesystem-specific optimizations (hardlinks for zero-copy dedup within the same store, `O_DIRECT` for large blob writes, `sendfile` for pulls). None of these may appear in the `StorageBackend` trait or in `LayerStore`'s public API. Backend details are backend details.

### GC is Manual at POC Scale
Garbage collection — identifying and deleting blobs unreferenced by any manifest — is triggered explicitly via the API. No background thread, no automatic eviction. **Trigger to automate:** store size exceeds a configurable threshold (default: 50 GB), or layer count makes manual GC impractical.

## What This Is Not
- **Not a filesystem.** `fs-storage` does not mount anything. It reads and writes files. The dm-thin assembly step that produces Firecracker block devices is the runtime's job, not the store's.
- **Not a registry.** No HTTP server, no pull-by-tag-over-the-wire. Distribution is out of scope. **Trigger to revisit:** when builds run on more than one machine, or when Microvisor nodes need to pull layers remotely.
- **Not a cache.** The store is the source of truth, not a cache in front of something else. There is no eviction policy at this layer.
- **Not a build executor.** The store holds artifacts; it does not build them. The builder calls the store API; the store does not know what a Dockerfile is.

## Upgrade Path
The swap from local to remote storage is a single implementation change:

| Today | Trigger | Change |
|---|---|---|
| `LocalBackend` (local disk, `fs::write`) | Multi-machine builds or remote pulls needed | Implement `S3Backend` (or `GcsBackend`) satisfying `StorageBackend`; swap at construction time |
| Manual GC via API | Store exceeds 50 GB or GC becomes impractical | Add background GC task in `LayerStore`; no change to `StorageBackend` trait |
| In-process API (library calls) | CLI or Microvisor runs on a different machine | Wrap `LayerStore` in a network transport (gRPC or HTTP); no change to the store internals |

Each upgrade is a drop-in at the interface boundary it crosses. Nothing above the trait needs to change.

## Status
This document is the initial design proposal for `fs-storage`. Next steps: PRD (full operator stories and requirements), then implementation starting with `LocalBackend` and the `LayerStore` API surface.
