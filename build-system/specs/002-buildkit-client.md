---
name: build-system-buildkit-client
status: draft
author: Graham Losee
date: 2026-05-21
derives_from: build-system-tad
tags: [build-system, buildkit, grpc, specs]
---

# Spec 002: BuildKit Integration

## Overview
The `BuildEngine` abstraction is implemented via a `BuildKitClient` that communicates with the `buildkitd` daemon over gRPC to execute Dockerfile stages in isolation.

## API & Abstraction
```rust
pub trait BuildEngine {
    fn solve_stage(&self, req: SolveRequest) -> Result<PathBuf, BuildError>;
}

pub struct SolveRequest {
    pub stage_name: String,
    pub dockerfile_path: PathBuf,
    pub context_path: PathBuf,
    pub parent_cache_override: Option<(String, PathBuf)>, // (parent_stage_name, oci_layout_dir)
    pub export_dir: PathBuf,
}
```

## gRPC Integration Details
1. **Solve Request**: The client issues a `Solve` RPC call targeting `req.stage_name`.
2. **Context Injection**: If `parent_cache_override` is provided, it passes `--opt context:<parent_stage>=oci-layout://<path>` to BuildKit. This instructs BuildKit to use the unpacked OCI layout directory instead of building the parent stage, preventing redundant builds.
3. **Local Exporter**: The request specifies the `"local"` exporter, causing BuildKit to write the resulting filesystem directly to `req.export_dir` on the build host.

## Architectural Considerations
- The client relies on an external `buildkitd` instance; lifecycle management of the daemon is out of scope.
- `BuildKitClient` executes stages synchronously (blocking until completion) for the POC.
