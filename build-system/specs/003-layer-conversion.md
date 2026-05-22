---
name: build-system-layer-conversion
status: draft
author: Graham Losee
date: 2026-05-21
derives_from: build-system-tad
tags: [build-system, layer-conversion, ext4, oci, specs]
---

# Spec 003: Layer Conversion Pipeline

## Overview
The `LayerFormat` subsystem converts BuildKit export directories into content-addressed dual artifacts: a deploy artifact (`ext4.zst`) for Microvisor, and a build-cache artifact (`oci.tar.zst`) for BuildKit.

## Data Structures
```rust
pub struct LayerArtifacts {
    pub deploy_artifact: PathBuf,       // Path to .ext4.zst
    pub build_cache_artifact: PathBuf,  // Path to .oci.tar.zst
    pub output_hash: String,            // SHA-256 of raw ext4 bytes
}

pub trait LayerFormat {
    fn pack(source_dir: &Path, output_dir: &Path) -> Result<LayerArtifacts, ConvertError>;
    fn unpack_for_build(&self, artifact: &Path, target: &Path) -> Result<(), ConvertError>;
}
```

## Conversion Pipeline (`ZstdExt4OciFormat`)
When `pack` is called, two parallel tasks are spawned:

1. **Deploy Artifact Pipeline (`ext4.zst`)**:
   - Size calculation: `du -sb <dir>` + 10% headroom. Round up to 4MB boundary.
   - `mke2fs -t ext4 -d <source_dir> <output_dir>/<stage>.ext4` (no root/mount needed).
   - Compute `output_hash` = SHA-256 of `<stage>.ext4`.
   - `zstd -19 <stage>.ext4 -o <stage>.ext4.zst`.

2. **Build-Cache Artifact Pipeline (`oci.tar.zst`)**:
   - Create OCI layout JSON files (`index.json`, `oci-layout`, image manifest).
   - `tar -cf <stage>.oci.tar <source_dir> <oci_metadata>`.
   - `zstd -19 <stage>.oci.tar -o <stage>.oci.tar.zst`.

## Parallelism
The dual format conversion is highly parallelizable and should utilize multi-threading (e.g., Tokio spawn) to minimize build time overhead.
