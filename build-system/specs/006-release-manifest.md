---
name: build-system-release-manifest
status: draft
author: Graham Losee
date: 2026-05-21
derives_from: build-system-tad
tags: [build-system, release-manifest, specs]
---

# Spec 006: Release Manifest & Runtime Contract

## Overview
The `ReleaseManifest` is the formal contract between the Build System and Microvisor. It defines the completed layer stack and workload metadata required to instantiate a microVM.

## Schema
```rust
pub struct ReleaseManifest {
    pub schema_version: String,
    pub manifest_id: String,
    pub build_metadata: BuildMetadata,
    pub layers: Vec<ManifestLayer>,
    pub workload_metadata: HashMap<String, String>,
}

pub struct ManifestLayer {
    pub name: String,
    pub order: u32,
    pub input_hash: String,
    pub output_hash: String,
    pub size_bytes: u64,
    pub size_compressed_bytes: u64,
    pub parent_input_hash: Option<String>,
}
```

## Runtime Contract (Deploy Path)
Microvisor consumes the manifest with the following behavior:
1. Microvisor identifies the top layer (highest `order`) in the `layers` array.
2. It queries its local dm-thin pool. If a thin volume for `input_hash` does not exist, it pulls the deploy artifact (`.ext4.zst`) from `fs-storage` using the `content_hash` (which is `output_hash`).
3. Microvisor decompresses the artifact, verifies the SHA-256 matches `output_hash`, and writes it into a new dm-thin volume.
4. Intermediate layers (lower orders) are ignored at deploy time because the top layer is a complete, flat rootfs containing all filesystem state.
