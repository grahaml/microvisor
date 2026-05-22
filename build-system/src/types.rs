use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Input hash: SHA-256 of stage definition + parent input hash.
/// Used for cache lookup in fs-storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct InputHash(pub String);

/// Output hash: SHA-256 of raw (uncompressed) ext4 artifact bytes.
/// Also used as the ContentHash in fs-storage for the deploy artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct OutputHash(pub String);

impl From<String> for InputHash {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<String> for OutputHash {
    fn from(s: String) -> Self {
        Self(s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerArtifacts {
    pub deploy_artifact: PathBuf,       // .ext4.zst
    pub build_cache_artifact: PathBuf,  // .oci.tar.zst
    pub output_hash: OutputHash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerMetadata {
    pub input_hash: InputHash,
    pub output_hash: OutputHash,
    pub parent_input_hash: Option<InputHash>,
    pub size_bytes: u64,
    pub size_compressed_bytes: u64,
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
pub struct BuildMetadata {
    pub dockerfile_hash: String,
    pub build_timestamp: u64,
    pub builder_version: String,
    pub build_host: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestLayer {
    pub name: String,
    pub order: u32,
    pub input_hash: InputHash,
    pub output_hash: OutputHash,
    pub size_bytes: u64,
    pub size_compressed_bytes: u64,
    pub parent_input_hash: Option<InputHash>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildGraph {
    pub stages: Vec<ParsedStage>,
    pub release_meta: ReleaseMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedStage {
    pub name: String,
    pub parent_name: Option<String>,
    pub layer_name: String,
    pub order: u32,
    pub instructions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseMetadata {
    pub layer_order: Vec<String>,
    pub workload_meta: HashMap<String, String>,
}
