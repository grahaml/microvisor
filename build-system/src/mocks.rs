use crate::traits::*;
use crate::types::*;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct MockBuildEngine {
    pub solve_count: Arc<Mutex<usize>>,
}

#[async_trait]
impl BuildEngine for MockBuildEngine {
    async fn solve_stage(&self, req: SolveRequest) -> anyhow::Result<PathBuf> {
        let mut count = self.solve_count.lock().unwrap();
        *count += 1;
        Ok(req.export_dir)
    }
}

pub struct MockLayerFormat;

impl LayerFormat for MockLayerFormat {
    fn pack(&self, _source_dir: &Path, output_dir: &Path) -> anyhow::Result<LayerArtifacts> {
        let deploy = output_dir.join("layer.ext4.zst");
        let cache = output_dir.join("layer.oci.tar.zst");
        std::fs::create_dir_all(output_dir)?;
        std::fs::File::create(&deploy)?;
        std::fs::File::create(&cache)?;
        Ok(LayerArtifacts {
            deploy_artifact: deploy,
            build_cache_artifact: cache,
            output_hash: OutputHash("mock_output_hash".to_string()),
        })
    }

    fn unpack_for_build(&self, _artifact: &Path, _target: &Path) -> anyhow::Result<()> {
        Ok(())
    }
}

pub struct MockStoreClient {
    pub input_to_output: Arc<Mutex<HashMap<InputHash, OutputHash>>>,
    pub layers: Arc<Mutex<HashMap<OutputHash, LayerMetadata>>>,
}

#[async_trait]
impl StoreClient for MockStoreClient {
    async fn resolve_input_hash(&self, hash: &InputHash) -> anyhow::Result<Option<OutputHash>> {
        let map = self.input_to_output.lock().unwrap();
        Ok(map.get(hash).cloned())
    }

    async fn pull_layer(
        &self,
        content_hash: &OutputHash,
        _format: ArtifactFormat,
        _target: &Path,
    ) -> anyhow::Result<LayerMetadata> {
        let layers = self.layers.lock().unwrap();
        layers
            .get(content_hash)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Layer not found"))
    }

    async fn publish_layer(
        &self,
        _artifacts: &LayerArtifacts,
        meta: &LayerMetadata,
    ) -> anyhow::Result<()> {
        let mut map = self.input_to_output.lock().unwrap();
        map.insert(meta.input_hash.clone(), meta.output_hash.clone());

        let mut layers = self.layers.lock().unwrap();
        layers.insert(meta.output_hash.clone(), meta.clone());
        Ok(())
    }

    async fn publish_manifest(&self, _manifest: &ReleaseManifest) -> anyhow::Result<String> {
        Ok("mock_manifest_id".to_string())
    }
}

pub struct MockParser {
    pub graph: BuildGraph,
}

impl DockerfileParser for MockParser {
    fn parse(&self, _dockerfile: &Path) -> anyhow::Result<BuildGraph> {
        Ok(self.graph.clone())
    }
}

pub struct MockHasher;

impl Hasher for MockHasher {
    fn compute_input_hash(&self, stage: &ParsedStage, _parent_hash: Option<&InputHash>) -> InputHash {
        InputHash(format!("mock_input_hash_{}", stage.layer_name))
    }
}
