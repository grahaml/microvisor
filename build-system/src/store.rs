use crate::traits::{StoreClient, ArtifactFormat};
use crate::types::*;
use async_trait::async_trait;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_stream::StreamExt;
use tonic::transport::Channel;

pub mod fs_storage {
    tonic::include_proto!("fs_storage.v1");
}

use fs_storage::layer_store_service_client::LayerStoreServiceClient;
use fs_storage as proto;

pub struct GrpcStoreClient {
    client: LayerStoreServiceClient<Channel>,
}

impl GrpcStoreClient {
    pub async fn connect(dst: String) -> anyhow::Result<Self> {
        let client = LayerStoreServiceClient::connect(dst).await?;
        Ok(Self { client })
    }
}

#[async_trait]
impl StoreClient for GrpcStoreClient {
    async fn resolve_input_hash(&self, hash: &InputHash) -> anyhow::Result<Option<OutputHash>> {
        let mut client = self.client.clone();
        let req = proto::ResolveInputHashRequest {
            input_hash: hash.0.clone(),
        };
        let resp = client.resolve_input_hash(req).await?.into_inner();
        Ok(resp.content_hash.map(OutputHash))
    }

    async fn pull_layer(
        &self,
        content_hash: &OutputHash,
        format: ArtifactFormat,
        target: &Path,
    ) -> anyhow::Result<LayerMetadata> {
        let mut client = self.client.clone();
        let proto_format = match format {
            ArtifactFormat::Deploy => proto::ArtifactFormat::FormatDeploy,
            ArtifactFormat::BuildCache => proto::ArtifactFormat::FormatBuildCache,
        };

        let req = proto::PullLayerRequest {
            content_hash: content_hash.0.clone(),
            format: proto_format as i32,
        };

        let mut stream = client.pull_layer(req).await?.into_inner();
        let mut file = tokio::fs::File::create(target).await?;

        while let Some(resp) = stream.next().await {
            let resp = resp?;
            file.write_all(&resp.chunk).await?;
        }
        file.flush().await?;

        Ok(LayerMetadata {
            input_hash: InputHash("".into()),
            output_hash: content_hash.clone(),
            parent_input_hash: None,
            size_bytes: 0,
            size_compressed_bytes: 0,
            build_timestamp: 0,
            builder_version: "".into(),
            build_host: "".into(),
        })
    }

    async fn publish_layer(
        &self,
        artifacts: &LayerArtifacts,
        meta: &LayerMetadata,
    ) -> anyhow::Result<()> {
        let mut client = self.client.clone();

        // Use output_hash as placeholder for other mandatory hash fields to pass server validation
        let placeholder_hash = meta.output_hash.0.clone();

        let meta_msg = proto::LayerMeta {
            input_hash: meta.input_hash.0.clone(),
            content_hash: meta.output_hash.0.clone(),
            parent_input_hash: meta.parent_input_hash.as_ref().map(|h| h.0.clone()),
            uncompressed_size: meta.size_bytes,
            compressed_size_deploy: meta.size_compressed_bytes,
            compressed_size_cache: 0,
            builder_version: meta.builder_version.clone(),
            build_timestamp: meta.build_timestamp,
            build_host: meta.build_host.clone(),
            stage_definition_hash: placeholder_hash.clone(),
            dockerfile_hash: placeholder_hash,
        };

        let (tx, rx) = tokio::sync::mpsc::channel(10);
        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);

        let deploy_path = artifacts.deploy_artifact.clone();
        let cache_path = artifacts.build_cache_artifact.clone();

        let tx_task = tx.clone();
        tokio::spawn(async move {
            println!("   [client] Sending layer metadata...");
            if let Err(e) = tx_task.send(proto::PublishLayerRequest {
                payload: Some(proto::publish_layer_request::Payload::Meta(meta_msg)),
            }).await {
                eprintln!("   [client] ERROR: Failed to send metadata: {}", e);
                return;
            }

            println!("   [client] Streaming deploy artifact: {}...", deploy_path.display());
            if let Ok(mut file) = tokio::fs::File::open(&deploy_path).await {
                let mut buf = vec![0u8; 65536]; // Use larger buffer
                let mut sent = 0;
                while let Ok(n) = file.read(&mut buf).await {
                    if n == 0 { break; }
                    if let Err(e) = tx_task.send(proto::PublishLayerRequest {
                        payload: Some(proto::publish_layer_request::Payload::DeployChunk(buf[..n].to_vec())),
                    }).await {
                        eprintln!("   [client] ERROR: Failed to send deploy chunk: {}", e);
                        return;
                    }
                    sent += n;
                }
                println!("   [client] Sent {} bytes of deploy artifact.", sent);
            } else {
                eprintln!("   [client] ERROR: Failed to open deploy artifact: {}", deploy_path.display());
            }

            println!("   [client] Streaming cache artifact: {}...", cache_path.display());
            if let Ok(mut file) = tokio::fs::File::open(&cache_path).await {
                let mut buf = vec![0u8; 65536];
                let mut sent = 0;
                while let Ok(n) = file.read(&mut buf).await {
                    if n == 0 { break; }
                    if let Err(e) = tx_task.send(proto::PublishLayerRequest {
                        payload: Some(proto::publish_layer_request::Payload::CacheChunk(buf[..n].to_vec())),
                    }).await {
                        eprintln!("   [client] ERROR: Failed to send cache chunk: {}", e);
                        return;
                    }
                    sent += n;
                }
                println!("   [client] Sent {} bytes of cache artifact.", sent);
            } else {
                eprintln!("   [client] ERROR: Failed to open cache artifact: {}", cache_path.display());
            }
            println!("   [client] Finished sending all chunks.");
        });

        drop(tx); 

println!("   [client] Waiting for server response...");
client.publish_layer(stream).await?;
println!("   [client] Layer published successfully.");
Ok(())
}

    async fn publish_manifest(&self, manifest: &ReleaseManifest) -> anyhow::Result<String> {
        let mut client = self.client.clone();
        let req = proto::PublishManifestRequest {
            manifest: Some(proto::ReleaseManifest {
                schema_version: manifest.schema_version.clone(),
                manifest_id: manifest.manifest_id.clone(),
                build_metadata: Some(proto::BuildMetadata {
                    dockerfile_hash: manifest.build_metadata.dockerfile_hash.clone(),
                    build_timestamp: manifest.build_metadata.build_timestamp,
                    builder_version: manifest.build_metadata.builder_version.clone(),
                    build_host: manifest.build_metadata.build_host.clone(),
                }),
                layers: manifest.layers.iter().map(|l| proto::ManifestLayer {
                    name: l.name.clone(),
                    order: l.order,
                    input_hash: l.input_hash.0.clone(),
                    content_hash: l.output_hash.0.clone(),
                    size_bytes: l.size_bytes,
                    size_compressed_bytes: l.size_compressed_bytes,
                    parent_input_hash: l.parent_input_hash.as_ref().map(|h| h.0.clone()),
                }).collect(),
                workload_metadata: manifest.workload_metadata.clone(),
            }),
        };

        let resp = client.publish_manifest(req).await?.into_inner();
        Ok(resp.manifest_hash)
    }
}
