use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::{Stream, StreamExt};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, transport::Server};
use crate::proto::layer_store_service_server::{LayerStoreService, LayerStoreServiceServer};
use crate::proto::*;
use crate::{LayerStore, ContentHash, InputHash, ReleaseManifest as RustReleaseManifest, LayerMeta as RustLayerMeta, StoreError};
use std::pin::Pin;
use tokio::io::{AsyncRead, ReadBuf, AsyncReadExt};
use std::task::{Context, Poll};
use std::io;
use std::net::SocketAddr;
use std::str::FromStr;

pub struct LayerStoreServer {
    store: Arc<LayerStore>,
}

impl LayerStoreServer {
    pub fn new(store: Arc<LayerStore>) -> Self {
        Self { store }
    }
}

pub async fn run_server(store: Arc<LayerStore>, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    let service = LayerStoreServer::new(store);
    
    Server::builder()
        .add_service(LayerStoreServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}

// Adapter to turn an mpsc::Receiver into an AsyncRead
struct StreamAdapter {
    receiver: mpsc::Receiver<Vec<u8>>,
    buffer: Option<Vec<u8>>,
    pos: usize,
}

impl StreamAdapter {
    fn new(receiver: mpsc::Receiver<Vec<u8>>) -> Self {
        Self {
            receiver,
            buffer: None,
            pos: 0,
        }
    }
}

impl AsyncRead for StreamAdapter {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            let (to_copy, buffer_done) = if let Some(ref data) = self.buffer {
                let remaining = data.len() - self.pos;
                let to_copy = std::cmp::min(remaining, buf.remaining());
                buf.put_slice(&data[self.pos..self.pos + to_copy]);
                (to_copy, self.pos + to_copy >= data.len())
            } else {
                (0, false)
            };

            if to_copy > 0 {
                self.pos += to_copy;
                if buffer_done {
                    self.buffer = None;
                    self.pos = 0;
                }
                return Poll::Ready(Ok(()));
            }

            match self.receiver.poll_recv(cx) {
                Poll::Ready(Some(data)) => {
                    self.buffer = Some(data);
                    self.pos = 0;
                    continue;
                }
                Poll::Ready(None) => return Poll::Ready(Ok(())), // EOF
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

#[tonic::async_trait]
impl LayerStoreService for LayerStoreServer {
    async fn resolve_input_hash(
        &self,
        request: Request<ResolveInputHashRequest>,
    ) -> Result<Response<ResolveInputHashResponse>, Status> {
        let req = request.into_inner();
        let input_hash = InputHash::from_str(&req.input_hash)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        
        let content_hash = self.store.resolve_input_hash(&input_hash).await
            .map_err(map_error)?;

        Ok(Response::new(ResolveInputHashResponse {
            content_hash: content_hash.map(|h| h.to_string()),
        }))
    }

    async fn has_layer(
        &self,
        request: Request<HasLayerRequest>,
    ) -> Result<Response<HasLayerResponse>, Status> {
        let req = request.into_inner();
        let content_hash = ContentHash::from_str(&req.content_hash)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        
        let exists = self.store.has_layer(&content_hash).await
            .map_err(map_error)?;

        Ok(Response::new(HasLayerResponse { exists }))
    }

    type PullLayerStream = Pin<Box<dyn Stream<Item = Result<PullLayerResponse, Status>> + Send>>;

    async fn pull_layer(
        &self,
        request: Request<PullLayerRequest>,
    ) -> Result<Response<Self::PullLayerStream>, Status> {
        let req = request.into_inner();
        let content_hash = ContentHash::from_str(&req.content_hash)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        
        let format = match req.format() {
            crate::proto::ArtifactFormat::FormatDeploy => crate::types::ArtifactFormat::Deploy,
            crate::proto::ArtifactFormat::FormatBuildCache => crate::types::ArtifactFormat::BuildCache,
            _ => return Err(Status::invalid_argument("Invalid format")),
        };

        let mut reader = self.store.pull_layer(&content_hash, format).await
            .map_err(map_error)?;

        let (tx, rx) = mpsc::channel(10);

        tokio::spawn(async move {
            let mut buf = vec![0u8; 64 * 1024];
            while let Ok(n) = reader.read(&mut buf).await {
                if n == 0 { break; }
                if tx.send(Ok(PullLayerResponse { chunk: buf[..n].to_vec() })).await.is_err() {
                    break;
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    async fn publish_layer(
        &self,
        request: Request<tonic::Streaming<PublishLayerRequest>>,
    ) -> Result<Response<PublishLayerResponse>, Status> {
        let mut stream = request.into_inner();
        
        let first = stream.next().await
            .ok_or_else(|| Status::invalid_argument("Empty stream"))??;
        
        let proto_meta = match first.payload {
            Some(publish_layer_request::Payload::Meta(m)) => m,
            _ => return Err(Status::invalid_argument("First message must be LayerMeta")),
        };

        let rust_meta = RustLayerMeta {
            input_hash: InputHash::from_str(&proto_meta.input_hash)
                .map_err(|e| Status::invalid_argument(e.to_string()))?,
            content_hash: ContentHash::from_str(&proto_meta.content_hash)
                .map_err(|e| Status::invalid_argument(e.to_string()))?,
            parent_input_hash: proto_meta.parent_input_hash.map(|h| InputHash::from_str(&h)).transpose()
                .map_err(|e| Status::invalid_argument(e.to_string()))?,
            uncompressed_size: proto_meta.uncompressed_size,
            compressed_size_deploy: proto_meta.compressed_size_deploy,
            compressed_size_cache: proto_meta.compressed_size_cache,
            file_checksum_deploy: ContentHash::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap(),
            file_checksum_cache: ContentHash::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap(),
            builder_version: proto_meta.builder_version,
            build_timestamp: proto_meta.build_timestamp,
            build_host: proto_meta.build_host,
            stage_definition_hash: ContentHash::from_str(&proto_meta.stage_definition_hash)
                .map_err(|e| Status::invalid_argument(e.to_string()))?,
            dockerfile_hash: ContentHash::from_str(&proto_meta.dockerfile_hash)
                .map_err(|e| Status::invalid_argument(e.to_string()))?,
        };

        let (deploy_tx, deploy_rx) = mpsc::channel(32);
        let (cache_tx, cache_rx) = mpsc::channel(32);

        let mut deploy_reader = StreamAdapter::new(deploy_rx);
        let mut cache_reader = StreamAdapter::new(cache_rx);

        tokio::spawn(async move {
            while let Ok(Some(msg)) = stream.message().await {
                match msg.payload {
                    Some(publish_layer_request::Payload::DeployChunk(chunk)) => {
                        if deploy_tx.send(chunk).await.is_err() { break; }
                    }
                    Some(publish_layer_request::Payload::CacheChunk(chunk)) => {
                        if cache_tx.send(chunk).await.is_err() { break; }
                    }
                    _ => {}
                }
            }
        });

        let content_hash = self.store.publish_layer_concurrent(&mut deploy_reader, &mut cache_reader, rust_meta).await
            .map_err(map_error)?;

        Ok(Response::new(PublishLayerResponse {
            content_hash: content_hash.to_string(),
        }))
    }

    async fn publish_manifest(
        &self,
        request: Request<PublishManifestRequest>,
    ) -> Result<Response<PublishManifestResponse>, Status> {
        let req = request.into_inner();
        let proto_manifest = req.manifest.ok_or_else(|| Status::invalid_argument("Missing manifest"))?;
        
        let rust_manifest = RustReleaseManifest {
            schema_version: proto_manifest.schema_version,
            manifest_id: proto_manifest.manifest_id,
            build_metadata: crate::types::BuildMetadata {
                dockerfile_hash: ContentHash::from_str(&proto_manifest.build_metadata.as_ref().unwrap().dockerfile_hash)
                    .map_err(|e| Status::invalid_argument(e.to_string()))?,
                build_timestamp: proto_manifest.build_metadata.as_ref().unwrap().build_timestamp,
                builder_version: proto_manifest.build_metadata.as_ref().unwrap().builder_version.clone(),
                build_host: proto_manifest.build_metadata.as_ref().unwrap().build_host.clone(),
            },
            layers: proto_manifest.layers.into_iter().map(|l| {
                crate::types::ManifestLayer {
                    name: l.name,
                    order: l.order,
                    input_hash: InputHash::from_str(&l.input_hash).unwrap(),
                    content_hash: ContentHash::from_str(&l.content_hash).unwrap(),
                    size_bytes: l.size_bytes,
                    size_compressed_bytes: l.size_compressed_bytes,
                    parent_input_hash: l.parent_input_hash.map(|h| InputHash::from_str(&h).unwrap()),
                }
            }).collect(),
            workload_metadata: proto_manifest.workload_metadata,
        };

        let hash = self.store.publish_manifest(rust_manifest).await
            .map_err(map_error)?;

        Ok(Response::new(PublishManifestResponse {
            content_hash: hash.to_string(),
        }))
    }

    async fn get_manifest(
        &self,
        request: Request<GetManifestRequest>,
    ) -> Result<Response<GetManifestResponse>, Status> {
        let req = request.into_inner();
        let hash = ContentHash::from_str(&req.content_hash)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        
        let manifest = self.store.get_manifest(&hash).await
            .map_err(map_error)?;

        Ok(Response::new(GetManifestResponse {
            manifest: Some(ReleaseManifest {
                schema_version: manifest.schema_version,
                manifest_id: manifest.manifest_id,
                build_metadata: Some(BuildMetadata {
                    dockerfile_hash: manifest.build_metadata.dockerfile_hash.to_string(),
                    build_timestamp: manifest.build_metadata.build_timestamp,
                    builder_version: manifest.build_metadata.builder_version,
                    build_host: manifest.build_metadata.build_host,
                }),
                layers: manifest.layers.into_iter().map(|l| {
                    ManifestLayer {
                        name: l.name,
                        order: l.order,
                        input_hash: l.input_hash.to_string(),
                        content_hash: l.content_hash.to_string(),
                        size_bytes: l.size_bytes,
                        size_compressed_bytes: l.size_compressed_bytes,
                        parent_input_hash: l.parent_input_hash.map(|h| h.to_string()),
                    }
                }).collect(),
                workload_metadata: manifest.workload_metadata,
            })
        }))
    }

    async fn list_builds(
        &self,
        request: Request<ListBuildsRequest>,
    ) -> Result<Response<ListBuildsResponse>, Status> {
        let req = request.into_inner();
        let filter = match req.filter {
            Some(f) => crate::types::BuildFilter {
                tag: f.tag,
                release_name_prefix: f.release_name_prefix,
                layer_hash: f.layer_hash.map(|h| ContentHash::from_str(&h).unwrap()),
                from_timestamp: f.from_timestamp,
                to_timestamp: f.to_timestamp,
            },
            None => crate::types::BuildFilter::default(),
        };

        let builds = self.store.list_builds(filter).await
            .map_err(map_error)?;

        Ok(Response::new(ListBuildsResponse {
            builds: builds.into_iter().map(|b| BuildCatalogEntry {
                build_id: b.build_id.to_string(),
                release_name: b.release_name,
                manifest_hash: b.manifest_hash.to_string(),
                build_timestamp: b.build_timestamp,
                spec_hash: b.spec_hash.to_string(),
                layers: b.layers.into_iter().map(|h| h.to_string()).collect(),
            }).collect()
        }))
    }

    async fn create_tag(
        &self,
        request: Request<CreateTagRequest>,
    ) -> Result<Response<CreateTagResponse>, Status> {
        let req = request.into_inner();
        let hash = ContentHash::from_str(&req.manifest_hash)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        
        self.store.create_tag(&req.name, &hash).await
            .map_err(map_error)?;

        Ok(Response::new(CreateTagResponse {}))
    }

    async fn get_tag(
        &self,
        request: Request<GetTagRequest>,
    ) -> Result<Response<GetTagResponse>, Status> {
        let req = request.into_inner();
        let hash = self.store.get_tag(&req.name).await
            .map_err(map_error)?;

        Ok(Response::new(GetTagResponse {
            manifest_hash: hash.to_string(),
        }))
    }
}

fn map_error(e: StoreError) -> Status {
    match e {
        StoreError::NotFound => Status::not_found(e.to_string()),
        StoreError::HashCollision { .. } => Status::already_exists(e.to_string()),
        StoreError::InvalidHash(_) | StoreError::InvalidTagName(_) => Status::invalid_argument(e.to_string()),
        StoreError::Io(_) | StoreError::Serialization(_) => Status::internal(e.to_string()),
    }
}
