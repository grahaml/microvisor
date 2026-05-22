---
name: smelter-branding-rename
status: approved
author: Graham Losee
derives_from: build-system-branding
tags: [the-smelter, the-reserve, branding, rename]
depends_on: []
---

# Spec 008: Smelter Branding Rename

## Context
The build system and artifact store have been branded as **The Smelter** and **The Reserve**. A branding doc and logo exist at `build-system/docs/BRANDING.md`. This spec defines every rename needed to propagate the full lexicon through all code, docs, specs, proto definitions, and cross-references.

`steel.*` Dockerfile labels stay unchanged — they belong to the workload platform, not the build system.

## Lexicon (Source of Truth: BRANDING.md)

| Old | New | Scope |
|---|---|---|
| `build-system` / `build system` | `the-smelter` / `The Smelter` | Package, dirs, all refs |
| `fs-storage` / `fs_storage` | `the-reserve` / `the_reserve` | Package, dirs, proto, all refs |
| `Dockerfile` (as build spec concept) | `Alloy` | Trait names, params, docs (keep literal `Dockerfile` in buildctl CLI args — that's the file format name) |
| `build` (the act of building) | `charge` | Methods, CLI subcommand, metadata fields |
| `layer artifact` / per-layer output | `Ingot` | Struct names, method names, proto messages |
| `BuildMetadata` | `HeatMetadata` | Struct (a "Heat" = one build run) |
| `store` (when referring to The Reserve) | `reserve` | Trait names, field names, struct names |
| `ArtifactFormat::BuildCache` | `ArtifactFormat::ChargeCache` | Enum variant |
| `unpack_for_build` | `unpack_for_charge` | Method name |
| `build_cache_artifact` | `charge_cache_artifact` | Field name |

### Unchanged
- `steel.*` labels — workload platform namespace
- `ContentHash`, `InputHash`, `OutputHash` — hash types are abstract
- `Hasher` / `RealHasher` — abstract hash utility
- `StorageBackend` trait — internal storage abstraction
- `ParsedStage`, `ReleaseMetadata`, `BuildGraph` — internal parser data structures
- Internal temp file names (`layer.ext4`, `layer.oci.tar`)
- Buildctl CLI arguments (`"build"`, `"dockerfile"`, `"filename"`) — BuildKit's API
- On-disk metadata keys (`layers/`, `index/`, `manifests/`, `builds/`, `tags/`) — data migration concern, not branding

---

## Phase 1: Proto Rename

Both proto files (`proto/fs_storage.proto` and `fs-storage/proto/fs_storage.proto`) rename to `the_reserve.proto`.

| Old | New |
|---|---|
| `package fs_storage.v1` | `package the_reserve.v1` |
| `service LayerStoreService` | `service ReserveService` |
| `rpc HasLayer` | `rpc HasIngot` |
| `rpc PullLayer` | `rpc PullIngot` |
| `rpc PublishLayer` | `rpc PublishIngot` |
| `rpc ListBuilds` | `rpc ListHeats` |
| `message LayerMeta` | `message IngotMeta` |
| `message ManifestLayer` | `message ManifestIngot` |
| `message BuildMetadata` | `message HeatMetadata` |
| `message BuildCatalogEntry` | `message HeatCatalogEntry` |
| `message BuildFilter` | `message HeatFilter` |
| `FORMAT_BUILD_CACHE` | `FORMAT_CHARGE_CACHE` |
| Field `builder_version` | `smelter_version` |
| Field `build_timestamp` | `charge_timestamp` |
| Field `build_host` | `smelter_host` |
| Field `dockerfile_hash` | `alloy_hash` |
| Field `layers` (repeated) | `ingots` |
| Field `layer_hash` | `ingot_hash` |
| Field `build_id` | `heat_id` |
| Field `spec_hash` | `alloy_hash` |

Request/response message names follow their RPC names (`HasIngotRequest`, `PullIngotRequest`, etc.).

---

## Phase 2: fs-storage → the-reserve (Rust Crate)

### Cargo.toml
`name = "fs-storage"` → `name = "the-reserve"`

### build.rs
`"proto/fs_storage.proto"` → `"proto/the_reserve.proto"`

### src/lib.rs
- `tonic::include_proto!("fs_storage.v1")` → `tonic::include_proto!("the_reserve.v1")`

### src/types.rs
| Old | New |
|---|---|
| `ArtifactFormat::BuildCache` | `ArtifactFormat::ChargeCache` |
| `BuildId` | `HeatId` |
| `LayerMeta` | `IngotMeta` |
| `ManifestLayer` | `ManifestIngot` |
| `BuildMetadata` | `HeatMetadata` |
| `ReleaseManifest.layers` | `ReleaseManifest.ingots` |
| `ReleaseManifest.build_metadata` | `ReleaseManifest.heat_metadata` |
| `BuildCatalogEntry` | `HeatCatalogEntry` |
| `BuildFilter` | `HeatFilter` |
| `StoreError` | `ReserveError` |

Field renames within structs: `dockerfile_hash` → `alloy_hash`, `build_timestamp` → `charge_timestamp`, `builder_version` → `smelter_version`, `build_host` → `smelter_host`, `build_id` → `heat_id`, `spec_hash` → `alloy_hash`, `layer_hash` → `ingot_hash`, `layers` → `ingots`.

### src/traits.rs
`ArtifactFormat` usage follows the enum rename. `StorageBackend` stays.

### src/layer_store.rs
| Old | New |
|---|---|
| `LayerStore` | `Reserve` |
| `has_layer` | `has_ingot` |
| `pull_layer` | `pull_ingot` |
| `publish_layer` | `publish_ingot` |
| `list_builds` | `list_heats` |
| `get_build` | `get_heat` |
| `verify_store` | `verify_reserve` |

### src/server.rs
| Old | New |
|---|---|
| `LayerStoreServer` | `ReserveServer` |
| All proto-generated type refs | Follow proto renames |
| All method impls | Follow RPC renames |

### src/main.rs
- `use fs_storage::` → `use the_reserve::`
- `"fs-storage gRPC server listening"` → `"The Reserve gRPC server listening"`
- `fs_storage::server::run_server` → `the_reserve::server::run_server`

### src/backends/
No changes (internal, `StorageBackend` stays).

### tests/ (5 files)
- All `use fs_storage::` → `use the_reserve::`
- All type/method reference updates follow above renames
- Test function names: `test_layer_store_*` → `test_reserve_*`

---

## Phase 3: build-system → the-smelter (Rust Crate)

### Cargo.toml
`name = "build-system"` → `name = "the-smelter"`

### build.rs
`"../proto/fs_storage.proto"` → `"../proto/the_reserve.proto"`

### Dockerfile.test
No changes (`steel.*` labels stay).

### src/types.rs
| Old | New |
|---|---|
| `LayerArtifacts` | `IngotArtifacts` |
| `build_cache_artifact` | `charge_cache_artifact` |
| `LayerMetadata` | `IngotMetadata` |
| `BuildMetadata` | `HeatMetadata` |
| `ReleaseManifest.layers` | `ReleaseManifest.ingots` |
| `ReleaseManifest.build_metadata` | `ReleaseManifest.heat_metadata` |
| `ManifestLayer` | `ManifestIngot` |

Field renames: `build_timestamp` → `charge_timestamp`, `builder_version` → `smelter_version`, `build_host` → `smelter_host`, `dockerfile_hash` → `alloy_hash`. Comments: `fs-storage` → `The Reserve`.

### src/traits.rs
| Old | New |
|---|---|
| `BuildEngine` | `ChargeEngine` |
| `SolveRequest` | `ChargeRequest` |
| `solve_stage` | `charge_stage` |
| `SolveRequest.dockerfile_path` | `ChargeRequest.alloy_path` |
| `LayerFormat` | `IngotFormat` |
| `unpack_for_build` | `unpack_for_charge` |
| `ArtifactFormat::BuildCache` | `ArtifactFormat::ChargeCache` |
| `StoreClient` | `ReserveClient` |
| `pull_layer` | `pull_ingot` |
| `publish_layer` | `publish_ingot` |
| `DockerfileParser` | `AlloyParser` |
| param `dockerfile` | param `alloy` |

### src/orchestrator.rs
| Old | New |
|---|---|
| `BuildOrchestrator` | `ChargeOrchestrator` |
| field `store` | field `reserve` |
| `build()` | `charge()` |
| param `dockerfile` | param `alloy` |
| `layer_metas` | `ingot_metas` |
| `manifest_layers` | `manifest_ingots` |
| `BuildSystem` | `Smelter` |
| `/tmp/microvisor-build/` | `/tmp/smelter-charge/` |

### src/parser.rs
| Old | New |
|---|---|
| `RealDockerfileParser` | `RealAlloyParser` |
| `impl DockerfileParser` | `impl AlloyParser` |
| param `dockerfile` | param `alloy` |
| `test_parse_valid_dockerfile` | `test_parse_valid_alloy` |

All `steel.*` string literals, label matching, and error messages stay unchanged.

### src/buildkit.rs
| Old | New |
|---|---|
| `impl BuildEngine` | `impl ChargeEngine` |
| `solve_stage` | `charge_stage` |
| `SolveRequest` | `ChargeRequest` |
| `req.dockerfile_path` | `req.alloy_path` |

`BuildctlEngine` struct name stays (it's the actual tool name). Buildctl CLI args stay.

### src/converter.rs
| Old | New |
|---|---|
| `ZstdExt4OciFormat` | `ZstdExt4OciMold` |
| `impl LayerFormat` | `impl IngotFormat` |
| `LayerArtifacts` | `IngotArtifacts` |
| `unpack_for_build` | `unpack_for_charge` |

Internal temp file names (`layer.ext4`, `layer.oci.tar`) stay.

### src/store.rs
| Old | New |
|---|---|
| `mod fs_storage` | `mod the_reserve` |
| `include_proto!("fs_storage.v1")` | `include_proto!("the_reserve.v1")` |
| `LayerStoreServiceClient` | `ReserveServiceClient` |
| `GrpcStoreClient` | `GrpcReserveClient` |
| `impl StoreClient` | `impl ReserveClient` |
| `pull_layer` | `pull_ingot` |
| `publish_layer` | `publish_ingot` |

All proto message names follow proto renames.

### src/mocks.rs
| Old | New |
|---|---|
| `MockBuildEngine` | `MockChargeEngine` |
| `MockLayerFormat` | `MockIngotFormat` |
| `MockStoreClient` | `MockReserveClient` |
| field `layers` | field `ingots` |
| `MockParser` | `MockAlloyParser` |
| `impl DockerfileParser` | `impl AlloyParser` |

### src/lib.rs (tests)
- `BuildOrchestrator` → `ChargeOrchestrator`
- All mock type refs follow above
- `orchestrator.build(Path::new("Dockerfile"))` → `orchestrator.charge(Path::new("Alloy"))`
- `manifest.layers` → `manifest.ingots`

### src/main.rs
| Old | New |
|---|---|
| `use build_system::*` | `use the_smelter::*` |
| `#[command(name = "steel-build")]` | `#[command(name = "smelter")]` |
| `"Microvisor Build System"` | `"The Smelter — Microvisor Build System"` |
| `Commands::Build` | `Commands::Charge` |
| `"Build a rootfs from a Dockerfile"` | `"Charge an Alloy into Ingots"` |
| `system.build(&file)` | `system.charge(&file)` |
| `"Build successful!"` | `"Charge complete!"` |
| `manifest.layers.len()` | `manifest.ingots.len()` |
| `"Layers:"` | `"Ingots:"` |
| `BuildSystem::new(...)` | `Smelter::new(...)` |
| `GrpcStoreClient` | `GrpcReserveClient` |
| `RealDockerfileParser` | `RealAlloyParser` |
| `ZstdExt4OciFormat` | `ZstdExt4OciMold` |
| `store_url` | `reserve_url` |
| `"Garbage collect unreferenced layers"` | `"Garbage collect unreferenced ingots"` |

---

## Phase 4: Documentation (build-system/docs/)

**`000-one-pager.md`**, **`001-prd.md`**, **`002-tad.md`**: Full terminology rewrite — "build system" → "The Smelter", "Dockerfile" → "Alloy", "layer" → "ingot" (artifact context), "fs-storage" → "The Reserve", "build" (verb) → "charge". Steel labels stay. Frontmatter name/tags update. Code blocks updated to match new type/method names.

**`BRANDING.md`**: Already branded. Verify `fs-storage` parenthetical says The Reserve.

---

## Phase 5: Specs (build-system/specs/)

All 8 spec files (`000` through `007`): Frontmatter (name, tags, derives_from, depends_on), all prose, all code blocks. Same terminology mapping as Phase 4.

---

## Phase 6: Documentation (fs-storage/docs/)

All 3 doc files (`000`, `001`, `002`): "fs-storage" → "The Reserve", "layer store" → "Reserve", "build system" → "The Smelter", "Dockerfile" → "Alloy", "layer" → "ingot" (artifact context), "build" (verb) → "charge". Frontmatter.

---

## Phase 7: Specs (fs-storage/specs/)

All 6 spec files (`000` through `005`): Same pattern. Type names in code blocks match Rust renames.

---

## Phase 8: Cross-References

**`captains-log/260521-1430.md`**: `fs-storage` → `The Reserve`, `build-system` → `The Smelter`, `LayerStore` → `Reserve`.

**Diagrams (`.mmd` files)**:
- `004-fs-storage-architecture.mmd`: `LayerStore` → `Reserve`, `fs-storage` → `The Reserve`
- `005-build-system-architecture.mmd`: `fs-storage` → `The Reserve`, build system labels
- `006-build-storage-interaction.mmd`: `BuildSystem` → `Smelter`, `fs-storage` → `The Reserve`
- `007-microvisor-pull-flow.mmd`: `fs-storage` → `The Reserve`

**`build-system/docs/slideshow.html`**: Update code examples and architecture descriptions to use new type names.

---

## Phase 9: Directory Renames (Last)

- `build-system/` → `the-smelter/`
- `fs-storage/` → `the-reserve/`
- `proto/fs_storage.proto` → `proto/the_reserve.proto`
- `fs-storage/proto/fs_storage.proto` → `the-reserve/proto/the_reserve.proto`

---

## Phase 10: CLAUDE.md

Update `Project Structure` section references from `build-system` and `fs-storage` to `the-smelter` and `the-reserve`.

---

## Verification

1. `cargo check` in `the-smelter/` — all types, traits, imports resolve
2. `cargo check` in `the-reserve/` — all types, traits, imports resolve
3. `cargo test` in both crates — all tests pass
4. `grep -r` for stale identifiers across both crates, proto, diagrams, captains-log, CLAUDE.md — zero hits
5. Open `slideshow.html` in browser — code examples show new names
