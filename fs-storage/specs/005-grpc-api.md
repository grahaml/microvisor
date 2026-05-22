---
name: fs-storage-grpc-api
status: draft
author: Graham Losee
date: 2026-05-21
derives_from: fs-storage-prd
tags: [fs-storage, grpc, api, tonic, specs]
---

# Spec 005: gRPC API Layer (Tonic)

## Overview
To decouple the `build-system` from `fs-storage` and allow them to run as independent services, `fs-storage` will expose its `LayerStore` capabilities over a gRPC API. We will use the `tonic` crate, which is the industry standard for gRPC in the Rust `tokio` ecosystem.

## 1. Architecture
- **Transport:** HTTP/2 over TCP (or Unix Domain Sockets for local IPC).
- **Serialization:** Protocol Buffers (protobuf).
- **Server Framework:** `tonic` (backed by `hyper` and `tokio`).
- **Integration:** The `fs-storage` crate will expose a `fs_storage::server::run` entrypoint that wraps the existing `LayerStore` struct in a generated Tonic gRPC service.

## 2. Protobuf Definition (`fs_storage.proto`)
The API mirrors the `LayerStore` struct, with adaptations for gRPC streaming limitations.

```protobuf
syntax = "proto3";
package fs_storage.v1;

service LayerStoreService {
    // Input Hash Index
    rpc ResolveInputHash(ResolveInputHashRequest) returns (ResolveInputHashResponse);
    
    // Blob Operations
    rpc HasLayer(HasLayerRequest) returns (HasLayerResponse);
    
    // Server streaming for efficient zero-memory-buffer artifact pulls
    rpc PullLayer(PullLayerRequest) returns (stream PullLayerResponse);
    
    // Client streaming for artifact ingestion.
    // Client multiplexes metadata, deploy chunks, and cache chunks into one stream.
    rpc PublishLayer(stream PublishLayerRequest) returns (PublishLayerResponse);

    // Manifests
    rpc PublishManifest(PublishManifestRequest) returns (PublishManifestResponse);
    rpc GetManifest(GetManifestRequest) returns (GetManifestResponse);

    // Catalog & Tags
    rpc ListBuilds(ListBuildsRequest) returns (ListBuildsResponse);
    rpc CreateTag(CreateTagRequest) returns (CreateTagResponse);
    rpc GetTag(GetTagRequest) returns (GetTagResponse);
}

// -- Common Types --
enum ArtifactFormat {
    FORMAT_UNSPECIFIED = 0;
    FORMAT_DEPLOY = 1;      // ext4.zst
    FORMAT_BUILD_CACHE = 2; // oci.tar.zst
}

message LayerMeta {
    string input_hash = 1;
    string content_hash = 2;
    optional string parent_input_hash = 3;
    uint64 uncompressed_size = 4;
    uint64 compressed_size_deploy = 5;
    uint64 compressed_size_cache = 6;
    string builder_version = 7;
    uint64 build_timestamp = 8;
    string build_host = 9;
    string stage_definition_hash = 10;
    string dockerfile_hash = 11;
}

// -- Request/Response Messages --
message ResolveInputHashRequest { string input_hash = 1; }
message ResolveInputHashResponse { optional string content_hash = 1; }

message HasLayerRequest { string content_hash = 1; }
message HasLayerResponse { bool exists = 1; }

message PullLayerRequest {
    string content_hash = 1;
    ArtifactFormat format = 2;
}
message PullLayerResponse {
    bytes chunk = 1; // Streamed byte chunks
}

message PublishLayerRequest {
    oneof payload {
        LayerMeta meta = 1;          // Must be the first message in the stream
        bytes deploy_chunk = 2;      // Subsequent chunks for ext4.zst
        bytes cache_chunk = 3;       // Subsequent chunks for oci.tar.zst
    }
}
message PublishLayerResponse {
    string content_hash = 1;
}

// (Manifest and Tag definitions map 1:1 to Rust structs, omitted for brevity)
```

## 3. Multiplexed Streaming for `PublishLayer`
Because a gRPC client can only open one stream per RPC, and `LayerStore::publish_layer` expects two concurrent readers (one for deploy, one for cache), the `PublishLayerRequest` uses a `oneof` payload. 

**Server-Side Handling:**
1. The server receives the `PublishLayerRequest` stream.
2. It extracts the `LayerMeta` from the first message.
3. It creates two `tokio::sync::mpsc` channels (one for deploy chunks, one for cache chunks).
4. It spawns a background `tokio::task` to iterate the incoming gRPC stream, routing `deploy_chunk` bytes to the deploy channel and `cache_chunk` bytes to the cache channel.
5. It wraps the receiving ends of those channels in custom `AsyncRead` adapters.
6. It passes those adapters directly into the existing `LayerStore::publish_layer` method.

This perfectly bridges the gRPC streaming model with the dual-reader `LayerStore` implementation without needing to refactor the core storage logic or buffer files in memory.

## 4. Error Mapping
Internal `StoreError` variants will be mapped to standard gRPC `tonic::Status` codes:
- `StoreError::NotFound` -> `Status::not_found`
- `StoreError::HashCollision` -> `Status::already_exists` or `Status::failed_precondition`
- `StoreError::InvalidHash` / `InvalidTagName` -> `Status::invalid_argument`
- `StoreError::Io` -> `Status::internal`

## 5. Build System Integration
The `build-system` will utilize `tonic-build` to generate a lightweight gRPC client. This client will implement the exact same `StoreClient` trait specified in `build-system/specs/005-store-client.md`, making the network boundary entirely transparent to the core build orchestrator.
