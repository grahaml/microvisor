---
name: build-system-api-and-cli
status: draft
author: Graham Losee
date: 2026-05-21
derives_from: build-system-tad
tags: [build-system, api, cli, specs]
---

# Spec 007: API & CLI Interface

## Overview
The Build System exposes a programmatic API that drives a thin CLI. The CLI provides a user-friendly interface for operators and automation pipelines.

## Programmatic API
```rust
pub struct BuildSystem {
    // Initializes the build pipeline with dependencies
    pub fn new(store: Arc<dyn StoreClient>, engine: Arc<dyn BuildEngine>) -> Self;
    
    // Core API methods
    pub fn build(&self, dockerfile: &Path) -> Result<ReleaseManifest, Error>;
    pub fn diff(&self, manifest_a: &str, manifest_b: &str) -> Result<ManifestDiff, Error>;
}
```

## CLI Commands
- `steel-build build -f <Dockerfile>`: Executes a build. Outputs the `manifest_id`.
- `steel-build tag <manifest_id> <tag_name>`: Creates a named pointer (e.g., `production`).
- `steel-build diff <tag1> <tag2>`: Prints a structured diff of layers that changed, were added, or removed.
- `steel-build list-tags`: Lists all tags and their target manifests.
- `steel-build gc`: Invokes the garbage collection routine via the store client to prune unreferenced layers.

## Implementation Details
The CLI uses `clap` for command-line parsing and forwards all calls directly to the `BuildSystem` library methods. It formats the resulting data structures (like `ManifestDiff`) into human-readable terminal output.
