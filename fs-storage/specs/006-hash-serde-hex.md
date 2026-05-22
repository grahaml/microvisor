---
name: fs-storage-hash-serde-hex
status: draft
author: Graham Losee
date: 2026-05-21
derives_from: fs-storage-core-types
tags: [fs-storage, layer-store, types, serde, bugfix, specs]
---

# Spec 006: Hex Serialization for Hash Types

## Problem
`ContentHash([u8; 32])` and `InputHash([u8; 32])` derive `Serialize`/`Deserialize` via serde's default, which serializes `[u8; 32]` as a JSON array of 32 integers:

```json
{ "content_hash": [82, 116, 100, 19, 17, ...] }
```

This is:
1. **Inconsistent** with the gRPC layer, which passes hashes as hex strings.
2. **Inconsistent** with `Display`/`FromStr`, which already use hex.
3. **Unreadable** in on-disk JSON metadata files (`meta/layers/*.json`, `manifests/*.json`, `meta/builds/*.json`).
4. **Fragile** — any consumer expecting hex strings (CLI tools, debugging scripts, cross-service APIs) will fail to parse the array form.

## Fix
Replace derived `Serialize`/`Deserialize` on both `ContentHash` and `InputHash` with custom implementations that serialize as hex strings, reusing the existing `Display` (for serialization) and `FromStr` (for deserialization) logic.

### ContentHash
```rust
impl Serialize for ContentHash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        ContentHash::from_str(&s).map_err(serde::de::Error::custom)
    }
}
```

### InputHash
Same pattern — serialize via `Display` (hex), deserialize via `FromStr`.

## On-Disk Result
After the fix, JSON metadata files will contain:
```json
{ "content_hash": "527464671311ecfa3abfd3733c31e903a98674cd3113289517ec78a3077d2cb1" }
```

## Migration
This is a breaking change to the on-disk JSON format. Existing metadata written with the array form will fail to deserialize under the new code. Since the store is ephemeral at this stage (POC), no migration is needed — wipe and rebuild.

## Scope
- **Changes:** `fs-storage/src/types.rs` — `ContentHash` and `InputHash` serde impls only.
- **No changes** to `Display`, `FromStr`, `StorageBackend`, gRPC layer, or blob storage (blobs are binary, not JSON).
