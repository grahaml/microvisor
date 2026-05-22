---
name: build-system-orchestrator
status: draft
author: Graham Losee
date: 2026-05-21
derives_from: build-system-tad
tags: [build-system, orchestrator, specs]
---

# Spec 000: Build Orchestrator

## Overview
The `BuildOrchestrator` is the central coordination component of the Microvisor Build System. It drives the end-to-end build pipeline by interacting with the parser, the build engine (BuildKit), the layer conversion pipeline, and the store client.

## Data Structures
```rust
pub struct BuildPlan {
    pub graph: BuildGraph,
    pub stage_states: HashMap<String, StageState>,
}

pub enum StageState {
    Pending,
    CacheHit {
        content_hash: String,
        metadata: LayerMetadata,
    },
    CacheMiss,
    Built {
        artifacts: LayerArtifacts,
        metadata: LayerMetadata,
    },
    Failed(String),
}
```

## Pipeline Execution Flow
1. **Parse**: Invoke `DockerfileParser::parse` to produce the `BuildGraph`. Abort on validation errors.
2. **Plan**: Iterate through `BuildGraph` stages in dependency order.
   - Compute input hash for the stage via `Hasher::compute_input_hash`.
   - Query `StoreClient::resolve_input_hash`.
   - Mark stage as `CacheHit` or `CacheMiss`.
3. **Build & Convert**: Iterate through stages in dependency order.
   - If `CacheHit`, skip.
   - If `CacheMiss`:
     - If the parent stage was a `CacheHit`, pull the parent's `BuildCache` artifact (OCI layout) from `fs-storage` and inject it into BuildKit as a named context.
     - Call `BuildEngine::solve_stage` to build via BuildKit, exporting to a local scratch directory.
     - Call `LayerFormat::pack` on the output directory to produce `ext4.zst` and `oci.tar.zst` artifacts.
     - Publish artifacts and metadata via `StoreClient::publish_layer`.
     - Update state to `Built`.
4. **Manifest**:
   - Collect metadata for all layers in the `steel.release.layers` list.
   - Construct the `ReleaseManifest`.
   - Publish via `StoreClient::publish_manifest`.
