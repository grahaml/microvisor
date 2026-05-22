# build-system

Dockerfile-to-rootfs build pipeline for Microvisor. Parses Steel-labelled Dockerfiles, drives BuildKit stage-by-stage, and produces content-addressed ext4.zst + OCI cache artifacts published to fs-storage over gRPC.

> Linux only. Has not been tested on macOS.

## Prerequisites

- Rust (edition 2021, tested with rustc 1.95+)
- `protoc` (protobuf compiler) — `sudo apt install protobuf-compiler`
- `buildkitd` + `buildctl` — [BuildKit releases](https://github.com/moby/buildkit/releases)
- `mke2fs` — `sudo apt install e2fsprogs`
- `zstd` — `sudo apt install zstd`
- `tar`
- A running [fs-storage](../fs-storage/) instance

## Build

```bash
cargo build
```

## Run

You need three things running:

**Terminal 1 — buildkitd** (must run as root):
```bash
sudo buildkitd
```

**Terminal 2 — fs-storage**:
```bash
cd ../fs-storage
cargo run -- --port 8080 --root ./storage
```

**Terminal 3 — build**:
```bash
sudo ./target/debug/build-system --store-url http://localhost:8080 build --file Dockerfile.test
```

The build parses the Dockerfile, executes each stage through BuildKit, converts the output to ext4.zst and OCI cache artifacts, and publishes them to fs-storage.

## Tests

```bash
cargo test
```

The Dockerfile parser test runs without external dependencies. The `test_pack_unpack` test in `converter.rs` is `#[ignore]`'d by default since it requires `mke2fs` and `zstd` on the host — run it with `cargo test -- --ignored`.
