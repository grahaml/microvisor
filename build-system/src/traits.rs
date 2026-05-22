use crate::types::*;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

#[async_trait]
pub trait BuildEngine: Send + Sync {
    async fn solve_stage(&self, req: SolveRequest) -> anyhow::Result<PathBuf>;
}

pub struct SolveRequest {
    pub stage_name: String,
    pub dockerfile_path: PathBuf,
    pub context_path: PathBuf,
    pub parent_cache_override: Option<(String, PathBuf)>, // (parent_stage_name, oci_layout_dir)
    pub export_dir: PathBuf,
}

pub trait LayerFormat: Send + Sync {
    fn pack(&self, source_dir: &Path, output_dir: &Path) -> anyhow::Result<LayerArtifacts>;
    fn unpack_for_build(&self, artifact: &Path, target: &Path) -> anyhow::Result<()>;
}

pub enum ArtifactFormat {
    Deploy,
    BuildCache,
}

#[async_trait]
pub trait StoreClient: Send + Sync {
    async fn resolve_input_hash(&self, hash: &InputHash) -> anyhow::Result<Option<OutputHash>>;
    async fn pull_layer(
        &self,
        content_hash: &OutputHash,
        format: ArtifactFormat,
        target: &Path,
    ) -> anyhow::Result<LayerMetadata>;
    async fn publish_layer(
        &self,
        artifacts: &LayerArtifacts,
        meta: &LayerMetadata,
    ) -> anyhow::Result<()>;
    async fn publish_manifest(&self, manifest: &ReleaseManifest) -> anyhow::Result<String>;
}

pub trait DockerfileParser: Send + Sync {
    fn parse(&self, dockerfile: &Path) -> anyhow::Result<BuildGraph>;
}

pub trait Hasher: Send + Sync {
    fn compute_input_hash(&self, stage: &ParsedStage, parent_hash: Option<&InputHash>) -> InputHash;
}
