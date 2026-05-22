use crate::traits::*;
use crate::types::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use sha2::Digest;

pub struct BuildOrchestrator {
    parser: Arc<dyn DockerfileParser>,
    hasher: Arc<dyn Hasher>,
    engine: Arc<dyn BuildEngine>,
    format: Arc<dyn LayerFormat>,
    store: Arc<dyn StoreClient>,
    scratch_dir: PathBuf,
}

impl BuildOrchestrator {
    pub fn new(
        parser: Arc<dyn DockerfileParser>,
        hasher: Arc<dyn Hasher>,
        engine: Arc<dyn BuildEngine>,
        format: Arc<dyn LayerFormat>,
        store: Arc<dyn StoreClient>,
    ) -> Self {
        let scratch_dir = std::env::temp_dir().join(format!("microvisor-build-{}", std::process::id()));
        Self::with_scratch_dir(parser, hasher, engine, format, store, scratch_dir)
    }

    pub fn with_scratch_dir(
        parser: Arc<dyn DockerfileParser>,
        hasher: Arc<dyn Hasher>,
        engine: Arc<dyn BuildEngine>,
        format: Arc<dyn LayerFormat>,
        store: Arc<dyn StoreClient>,
        scratch_dir: PathBuf,
    ) -> Self {
        Self {
            parser,
            hasher,
            engine,
            format,
            store,
            scratch_dir,
        }
    }

    pub async fn build(&self, dockerfile: &Path) -> anyhow::Result<ReleaseManifest> {
        // 1. Parse
        let graph = self.parser.parse(dockerfile)?;

        let dockerfile_content = std::fs::read(dockerfile)?;
        let dockerfile_hash = hex::encode(sha2::Sha256::digest(&dockerfile_content));

        // 2. Plan
        let mut stage_results: HashMap<String, StageResult> = HashMap::new();
        let mut layer_metas: HashMap<String, LayerMetadata> = HashMap::new();

        // 3. Execute in dependency order
        for stage in &graph.stages {
            let parent_input_hash = stage
                .parent_name
                .as_ref()
                .and_then(|name| stage_results.get(name))
                .map(|res| res.input_hash.clone());

            let input_hash = self
                .hasher
                .compute_input_hash(stage, parent_input_hash.as_ref());

            // Check cache
            if let Some(content_hash) = self.store.resolve_input_hash(&input_hash).await? {
                // Cache hit
                let meta = self
                    .store
                    .pull_layer(&content_hash, ArtifactFormat::BuildCache, Path::new("")) // Dummy path for meta
                    .await?;

                stage_results.insert(
                    stage.name.clone(),
                    StageResult {
                        input_hash: input_hash.clone(),
                        output_hash: content_hash,
                        was_cache_hit: true,
                    },
                );
                layer_metas.insert(stage.layer_name.clone(), meta);
            } else {
                // Cache miss
                let mut stage_dir = self.scratch_dir.join(&stage.name);
                if stage_dir.exists() {
                    if std::fs::remove_dir_all(&stage_dir).is_err() {
                        stage_dir = self.scratch_dir.join(format!(
                            "{}-{}",
                            stage.name,
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_millis()
                        ));
                    }
                }
                let rootfs_dir = stage_dir.join("rootfs");
                let artifact_dir = stage_dir.join("artifacts");
                std::fs::create_dir_all(&rootfs_dir)?;
                std::fs::create_dir_all(&artifact_dir)?;

                // Only inject the parent OCI context when the parent was a
                // cache hit (skipped build). If we just built the parent via
                // BuildKit in this session, BuildKit already has it cached.
                let parent_cache_override = if let Some(parent_name) = &stage.parent_name {
                    let parent_res = stage_results
                        .get(parent_name)
                        .ok_or_else(|| anyhow::anyhow!("Parent result missing"))?;

                    if parent_res.was_cache_hit {
                        let oci_dir = stage_dir.join("parent_oci");
                        std::fs::create_dir_all(&oci_dir)?;
                        self.store.pull_layer(
                            &parent_res.output_hash,
                            ArtifactFormat::BuildCache,
                            oci_dir.as_path(),
                        ).await?;
                        self.format.unpack_for_build(
                            &oci_dir.join("layer.oci.tar.zst"),
                            &oci_dir,
                        )?;
                        Some((parent_name.clone(), oci_dir))
                    } else {
                        None
                    }
                } else {
                    None
                };

                let solve_req = SolveRequest {
                    stage_name: stage.name.clone(),
                    dockerfile_path: dockerfile.to_path_buf(),
                    context_path: dockerfile.parent().unwrap_or(Path::new(".")).to_path_buf(),
                    parent_cache_override,
                    export_dir: rootfs_dir.clone(),
                };

                let _output_dir = self.engine.solve_stage(solve_req).await?;
                
                let artifacts = self.format.pack(&rootfs_dir, &artifact_dir)?;
                
                let deploy_size = std::fs::metadata(&artifacts.deploy_artifact)?.len();
                let uncompressed_size = crate::converter::get_dir_size(&rootfs_dir).unwrap_or(0);

                let meta = LayerMetadata {
                    input_hash: input_hash.clone(),
                    output_hash: artifacts.output_hash.clone(),
                    parent_input_hash: parent_input_hash.clone(),
                    size_bytes: uncompressed_size,
                    size_compressed_bytes: deploy_size,
                    build_timestamp: chrono::Utc::now().timestamp() as u64,
                    builder_version: env!("CARGO_PKG_VERSION").to_string(),
                    build_host: hostname::get()?.to_string_lossy().to_string(),
                };

                self.store.publish_layer(&artifacts, &meta).await?;

                stage_results.insert(
                    stage.name.clone(),
                    StageResult {
                        input_hash: input_hash.clone(),
                        output_hash: artifacts.output_hash.clone(),
                        was_cache_hit: false,
                    },
                );
                layer_metas.insert(stage.layer_name.clone(), meta);
            }
        }

        // 4. Manifest
        let mut manifest_layers = Vec::new();
        for (i, layer_name) in graph.release_meta.layer_order.iter().enumerate() {
            let meta = layer_metas
                .get(layer_name)
                .ok_or_else(|| anyhow::anyhow!("Missing metadata for layer {}", layer_name))?;
            
            manifest_layers.push(ManifestLayer {
                name: layer_name.clone(),
                order: i as u32,
                input_hash: meta.input_hash.clone(),
                output_hash: meta.output_hash.clone(),
                size_bytes: meta.size_bytes,
                size_compressed_bytes: meta.size_compressed_bytes,
                parent_input_hash: meta.parent_input_hash.clone(),
            });
        }

        let manifest = ReleaseManifest {
            schema_version: "1.0".to_string(),
            manifest_id: uuid::Uuid::new_v4().to_string(),
            build_metadata: BuildMetadata {
                dockerfile_hash: dockerfile_hash.clone(),
                build_timestamp: chrono::Utc::now().timestamp() as u64,
                builder_version: env!("CARGO_PKG_VERSION").to_string(),
                build_host: hostname::get()?.to_string_lossy().to_string(),
            },
            layers: manifest_layers,
            workload_metadata: graph.release_meta.workload_meta,
        };

        self.store.publish_manifest(&manifest).await?;

        Ok(manifest)
    }
}

pub struct BuildSystem {
    orchestrator: BuildOrchestrator,
}

impl BuildSystem {
    pub fn new(
        parser: Arc<dyn DockerfileParser>,
        hasher: Arc<dyn Hasher>,
        engine: Arc<dyn BuildEngine>,
        format: Arc<dyn LayerFormat>,
        store: Arc<dyn StoreClient>,
    ) -> Self {
        Self {
            orchestrator: BuildOrchestrator::new(parser, hasher, engine, format, store),
        }
    }

    pub async fn build(&self, dockerfile: &Path) -> anyhow::Result<ReleaseManifest> {
        self.orchestrator.build(dockerfile).await
    }
}

#[allow(dead_code)]
struct StageResult {
    input_hash: InputHash,
    output_hash: OutputHash,
    was_cache_hit: bool,
}
