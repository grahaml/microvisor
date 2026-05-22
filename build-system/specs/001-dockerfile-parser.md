---
name: build-system-dockerfile-parser
status: draft
author: Graham Losee
date: 2026-05-21
derives_from: build-system-tad
tags: [build-system, dockerfile, parser, specs]
---

# Spec 001: Dockerfile Parser

## Overview
The `DockerfileParser` parses standard multi-stage Dockerfiles, extracts `steel.*` label annotations, validates the schema, and constructs a `BuildGraph` representing the dependency chain.

## Label Schema Definition
- `steel.layer` (Required on layer stages): Unique name for the layer (e.g., `base-os`).
- `steel.layer.order` (Required on layer stages): Integer denoting the position in the stack (0 is base).
- `steel.release` (Required on exactly one stage): Marks the release entry point. Value must be `"true"`.
- `steel.release.layers` (Required on release stage): Comma-separated list of layer names in order.
- `steel.meta.*` (Optional): Passthrough workload metadata (e.g., `steel.meta.vcpus="4"`).

## Validation Rules
1. Exactly one stage must have `steel.release="true"`.
2. The `steel_release` stage must be `FROM scratch`.
3. `steel.layer.order` must be contiguous starting from 0.
4. Layer names must be unique.
5. `steel.release.layers` must exactly match the defined `steel.layer` stages.

## Data Structures
```rust
pub struct BuildGraph {
    pub stages: Vec<ParsedStage>,
    pub release_meta: ReleaseMetadata,
}

pub struct ParsedStage {
    pub name: String,
    pub parent_name: Option<String>,
    pub layer_name: String,
    pub order: u32,
    pub instructions: Vec<String>, // Raw instructions for hashing
}

pub struct ReleaseMetadata {
    pub layer_order: Vec<String>,
    pub workload_meta: HashMap<String, String>,
}
```

## Parsing Logic
The parser reads the Dockerfile, segments it by `FROM` boundaries, and parses `LABEL` instructions. It does not evaluate `RUN` or `COPY` instructions but captures their raw text for input hash computation.
