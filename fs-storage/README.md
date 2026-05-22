# fs-storage

Content-addressed layer store for Microvisor rootfs artifacts. Persists build outputs (ext4.zst deploy images, OCI cache tarballs) and serves them over gRPC.

> Linux only. Has not been tested on macOS.

## Prerequisites

- Rust (edition 2024, tested with rustc 1.95+)
- `protoc` (protobuf compiler) — `sudo apt install protobuf-compiler`

## Build

```bash
cargo build
```

## Run

```bash
cargo run -- --port 8080 --root ./storage
```

This starts the gRPC server on port 8080 with the storage directory at `./storage`. The directory structure is created automatically on first run.

## Tests

```bash
cargo test
```

7 integration tests covering blob ops, metadata ops, GC, and both backends (local filesystem, in-memory).

## Storage Layout

```
storage/
  objects/{hash[0:2]}/{hash[2:]}.ext4.zst   # deploy artifacts
  objects/{hash[0:2]}/{hash[2:]}.oci.tar.zst # build cache artifacts
  manifests/{hash}.json                       # release manifests
  meta/layers/{hash}.json                     # layer metadata
  meta/index/{input-hash}                     # input→content hash index
  meta/builds/{build-id}.json                 # build catalog entries
  refs/tags/{name}                            # mutable tag pointers
  tmp/                                        # atomic write staging
```

To reset the store, stop the server and delete the `storage/` directory.
