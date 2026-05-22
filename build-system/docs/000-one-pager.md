---
name: build-system-one-pager
status: draft
author: Graham Losee
derives_from: null
tags: [build-system, rootfs, layers, content-addressing, dockerfile]
depends_on: [fs-storage]
---

# Project One-Pager: Microvisor Build System

## The Problem
We build large VM root filesystems for Steel workloads. Today, a small app or dependency change requires rebuilding and redistributing an entire `rootfs.ext4`. This is slow and wasteful because most bytes are unchanged between releases:

- **Base OS** (Debian, Alpine, Wolfi) rarely changes.
- **Platform binaries** like `steel-init` (our PID 1 supervisor) change occasionally.
- **Heavy dependencies** (Chrome, ffmpeg) change independently of each other.
- **App code** changes frequently.

A monolithic rootfs conflates all four concerns into one opaque blob. There is no way to skip unchanged work, share common layers across workload types, or know which bytes actually changed between two releases.

## The Solution: Layered, Content-Addressed RootFS Builds
The build system treats a rootfs as an ordered stack of independently-built, content-addressed layers. Each layer is a filesystem delta that is built once and reused until its inputs change — Docker's layer model, applied to actual rootfs images for Firecracker microVMs. The build spec is a standard multi-stage Dockerfile, so operators can `docker build && docker run` locally for testing and the same file drives production builds.

### How It Works
1. **Dockerfile as Build Spec:** A build is defined by a multi-stage Dockerfile. Each stage annotated with Steel labels (`LABEL steel.layer="..."`) maps to a layer in the build system. A `steel_release` stage (`LABEL steel.release="true"`) declares the ordered layer list and produces the release manifest.
2. **Docker/BuildKit as Execution Engine:** Stage execution is delegated to Docker/BuildKit, inheriting its per-instruction caching, container isolation, and existing tooling ecosystem. The build system orchestrates, extracts per-stage results, and computes stage-level content addresses.
3. **Content Addressing:** Each built layer is identified by a hash of its inputs (parent layer hash + stage definition hash). If inputs haven't changed, the existing artifact is reused — no rebuild required.
4. **Layer Artifacts:** Each layer produces a filesystem delta, a content-address hash, and metadata (size, creation time, parent hash, input manifest hash).
5. **Release Manifest:** The `steel_release` stage produces a JSON manifest listing the ordered layer stack, each layer's content address, and workload/runtime metadata. This manifest is the single artifact Microvisor needs to assemble a rootfs.

### Example Layer Stack
```
 Layer 0: debian-bookworm-minimal     (base OS — rebuilt ~monthly)
 Layer 1: steel-platform              (steel-init, agents — rebuilt on platform release)
 Layer 2: chrome-headless             (browser dep — rebuilt on upstream Chrome update)
 Layer 3: steel-browser-app           (app code — rebuilt on every deploy)
```

Changing the app code rebuilds only Layer 3. Updating Chrome rebuilds only Layer 2 and Layer 3 (since Layer 3 stacks on top of Layer 2). The base OS and platform layers are untouched.

## Design Constraints

### Content Addressing is the Core Invariant
A layer's identity is derived from its content, not its name or build timestamp. Two builds with identical inputs must produce the same content address. This enables:
- **Deduplication** across workload types that share base layers.
- **Deterministic builds** — same inputs, same output, verifiable.
- **Efficient distribution** — Microvisor only pulls layers it doesn't already have.

### Dockerfile Must Include `steel_release` Stage
Every Dockerfile must include a stage labeled `LABEL steel.release="true"` that declares the ordered layer list. This stage is the build's entry point and produces the release manifest. Other stages annotated with `LABEL steel.layer="..."` define the individual layers. The Dockerfile remains a valid Docker build spec — Steel labels are additive metadata, not a fork of the format.

### Output Artifacts
A completed build produces:
| Artifact | Description |
|---|---|
| Layer deltas | One filesystem delta per layer (format depends on assembly strategy — see open question below). |
| Layer metadata | Per-layer JSON: content hash, size, parent hash, input manifest hash. |
| Release manifest | Ordered layer list with content addresses, workload metadata, runtime assembly instructions. |

### Open Design Question: Layer Format and Assembly Strategy

The build system needs to produce layered artifacts. The runtime (Microvisor) needs to assemble them into a rootfs. The open question is **what format layers take**, which determines how both the build and runtime work.

**Option A: File-level layers (tarballs), flattened at deploy time**
Each layer is a tarball of filesystem changes (like Docker). At deploy time, layers are unpacked sequentially into a staging directory, then written into an ext4 block device image.
- *Pro:* Simple to build, inspect, and diff. Familiar model.
- *Con:* Assembly requires unpacking + `mkfs.ext4` — adds latency to the deploy path. Doesn't leverage dm-thin until the final image is snapshotted for per-VM ephemerality.

**Option B: File-level layers (tarballs), composed via overlayfs at deploy time**
Each layer is a tarball unpacked into its own directory. At deploy time, overlayfs stacks the directories, and the merged view backs a loop device or file-backed block device for Firecracker.
- *Pro:* Native layer composition — no flattening step. Adding/removing a layer doesn't require rebuilding the rootfs.
- *Con:* overlayfs operates through the host page cache (conflicts with `O_DIRECT` mandate). Loop device backing re-introduces the global lock contention dm-thin was chosen to avoid. May need to relax `O_DIRECT` for the rootfs path, or accept that overlayfs is build-time only.

**Option C: Block-level layers (dm-thin snapshots)**
Each layer is a dm-thin snapshot delta. The build system creates a thin volume for Layer 0, snapshots it for Layer 1's build, and so on. At deploy time, Microvisor activates the final snapshot and creates a per-VM CoW snapshot on top for ephemerality.
- *Pro:* Zero-copy assembly. Layers *are* the dm-thin stack — no conversion step. Natively compatible with Microvisor's existing dm-thin runtime. `O_DIRECT` works naturally.
- *Con:* Build host must have dm-thin infrastructure. Layers are block-level (harder to inspect individual files). Content addressing must work at the block level or hash the resulting filesystem.

**Current leaning:** Option C aligns most naturally with Microvisor's existing dm-thin storage subsystem and `O_DIRECT` mandate. Option A is the simplest starting point if we want build-time simplicity and accept a flatten step. Option B has the cleanest layer composition model but conflicts with `O_DIRECT`. This decision will be locked in the TAD.

### Layer Store (External Module: `fs-storage`)
The build system depends on `fs-storage`, a separate module that provides content-addressed layer storage, build cataloging, diffing, and garbage collection. The build system publishes to and pulls from `fs-storage` via its API. See the `fs-storage` docs for full requirements.

### Runtime Assembly via Microvisor
Microvisor reads the release manifest, pulls any layers not already cached locally, and assembles the rootfs using its existing dm-thin CoW snapshot infrastructure. Per-VM ephemerality is handled the same way it is today — a CoW snapshot on top of the assembled rootfs base. The build system's contract: produce content-addressed layers and a release manifest that Microvisor can consume.

## What This Is Not
- **Not a container runtime.** Layers are a build-time and storage concern. Microvisor gets a block device, not a union mount.
- **Not a package manager.** Layers can use `apt`, `apk`, or plain file copies internally, but the build system manages filesystem deltas, not individual packages.
- **Not the layer store.** Storage, cataloging, diffing, and GC are the responsibility of the `fs-storage` module. The build system is a producer and consumer of that store.

## Status
This document is the initial design proposal. Next steps: PRD (requirements + operator stories), TAD (technical architecture), then implementation.
