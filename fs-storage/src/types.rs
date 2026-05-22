use std::collections::HashMap;
use std::fmt;
use std::io;
use std::str::FromStr;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// Output hash: SHA-256 of the raw (uncompressed) ext4 artifact bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentHash([u8; 32]);

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

impl ContentHash {
    pub fn new(hash: [u8; 32]) -> Self {
        Self(hash)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl FromStr for ContentHash {
    type Err = StoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = hex::decode(s).map_err(|_| StoreError::InvalidHash(s.to_string()))?;
        if bytes.len() != 32 {
            return Err(StoreError::InvalidHash(s.to_string()));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }
}

/// Input hash: computed by the build system from the stage definition
/// and parent layer hash. Cache-lookup key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InputHash([u8; 32]);

impl Serialize for InputHash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for InputHash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        InputHash::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl InputHash {
    pub fn new(hash: [u8; 32]) -> Self {
        Self(hash)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for InputHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl FromStr for InputHash {
    type Err = StoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = hex::decode(s).map_err(|_| StoreError::InvalidHash(s.to_string()))?;
        if bytes.len() != 32 {
            return Err(StoreError::InvalidHash(s.to_string()));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArtifactFormat {
    /// Zstd-compressed ext4 image. Consumed by Microvisor → dm-thin.
    Deploy,
    /// Zstd-compressed OCI tar. Consumed by BuildKit cache injection.
    BuildCache,
}

impl ArtifactFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Deploy => "ext4.zst",
            Self::BuildCache => "oci.tar.zst",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BuildId(pub String);

impl fmt::Display for BuildId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerMeta {
    pub input_hash: InputHash,
    pub content_hash: ContentHash,
    pub parent_input_hash: Option<InputHash>,
    pub uncompressed_size: u64,
    pub compressed_size_deploy: u64,
    pub compressed_size_cache: u64,
    pub file_checksum_deploy: ContentHash,
    pub file_checksum_cache: ContentHash,
    // Provenance
    pub builder_version: String,
    pub build_timestamp: u64,
    pub build_host: String,
    pub stage_definition_hash: ContentHash,
    pub dockerfile_hash: ContentHash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestLayer {
    pub name: String,
    pub order: u32,
    pub input_hash: InputHash,
    pub content_hash: ContentHash,
    pub size_bytes: u64,
    pub size_compressed_bytes: u64,
    pub parent_input_hash: Option<InputHash>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildMetadata {
    pub dockerfile_hash: ContentHash,
    pub build_timestamp: u64,
    pub builder_version: String,
    pub build_host: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseManifest {
    pub schema_version: String,
    pub manifest_id: String,
    pub build_metadata: BuildMetadata,
    pub layers: Vec<ManifestLayer>,
    pub workload_metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildCatalogEntry {
    pub build_id: BuildId,
    pub release_name: String,
    pub manifest_hash: ContentHash,
    pub build_timestamp: u64,
    pub spec_hash: ContentHash,
    pub layers: Vec<ContentHash>,
}

#[derive(Debug, Clone, Default)]
pub struct BuildFilter {
    pub tag: Option<String>,
    pub release_name_prefix: Option<String>,
    pub layer_hash: Option<ContentHash>,
    pub from_timestamp: Option<u64>,
    pub to_timestamp: Option<u64>,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("Not found")]
    NotFound,
    #[error("Hash collision detected: {hash}")]
    HashCollision { hash: ContentHash },
    #[error("Invalid tag name: {0}")]
    InvalidTagName(String),
    #[error("Invalid hash: {0}")]
    InvalidHash(String),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct CorruptEntry {
    pub path: std::path::PathBuf,
    pub expected_hash: ContentHash,
    pub actual_hash: ContentHash,
}

#[derive(Debug, Clone)]
pub struct VerificationReport {
    pub valid: Vec<ContentHash>,
    pub corrupt: Vec<CorruptEntry>,
}

#[derive(Debug, Clone)]
pub struct GcCandidate {
    pub content_hash: ContentHash,
    pub size_deploy_bytes: u64,
    pub size_cache_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct GcReport {
    pub candidates: Vec<GcCandidate>,
    pub total_reclaimable_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct GcSummary {
    pub deleted_blobs: u64,
    pub deleted_bytes: u64,
}

