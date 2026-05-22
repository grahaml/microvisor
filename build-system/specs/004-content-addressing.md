---
name: build-system-content-addressing
status: draft
author: Graham Losee
date: 2026-05-21
derives_from: build-system-tad
tags: [build-system, content-addressing, hashing, specs]
---

# Spec 004: Content Addressing

## Overview
The `Hasher` component calculates deterministic `InputHash` values for cache lookups and `OutputHash` values for integrity verification.

## Data Structures
```rust
pub type InputHash = String;
pub type OutputHash = String;

pub struct Hasher;
impl Hasher {
    pub fn compute_input_hash(stage: &ParsedStage, parent_hash: Option<&str>) -> InputHash;
    pub fn compute_output_hash(file_path: &Path) -> Result<OutputHash, io::Error>;
}
```

## Hash Logic
### Input Hash
The input hash is a SHA-256 digest of:
1. `parent_hash` (or an empty string if base layer).
2. A null byte separator `\0`.
3. The normalized stage definition.

**Normalization Rules**:
- Trim leading/trailing whitespace and compress multiple spaces.
- Strip comments (`#`).
- Resolve `ARG` values.
- For base layers, resolve the `FROM <image>` reference to its exact registry digest (e.g., `sha256:abc...`).

### Output Hash
The output hash is the SHA-256 digest of the uncompressed `ext4` image bytes. This is computed during the layer conversion pipeline (Spec 003) prior to `zstd` compression.
