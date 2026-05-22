use fs_storage::{LayerStore, InMemoryBackend, InputHash, ContentHash, LayerMeta, ArtifactFormat};
use std::sync::Arc;
use std::str::FromStr;
use tokio::io::AsyncReadExt;
use std::io::Cursor;

#[tokio::test]
async fn test_layer_store_blob_ops() {
    let backend = Arc::new(InMemoryBackend::new());
    let store = LayerStore::new(backend.clone());

    let input_hash = InputHash::from_str("1111111111111111111111111111111111111111111111111111111111111111").unwrap();
    let content_hash = ContentHash::from_str("2222222222222222222222222222222222222222222222222222222222222222").unwrap();
    
    // Check initial state
    assert!(!store.has_layer(&content_hash).await.unwrap());
    assert!(store.resolve_input_hash(&input_hash).await.unwrap().is_none());

    let meta = LayerMeta {
        input_hash: input_hash.clone(),
        content_hash: content_hash.clone(),
        parent_input_hash: None,
        uncompressed_size: 100,
        compressed_size_deploy: 50,
        compressed_size_cache: 50,
        file_checksum_deploy: content_hash.clone(), // Placeholder, updated inline
        file_checksum_cache: content_hash.clone(),  // Placeholder, updated inline
        builder_version: "1.0".to_string(),
        build_timestamp: 123456789,
        build_host: "test-host".to_string(),
        stage_definition_hash: content_hash.clone(),
        dockerfile_hash: content_hash.clone(),
    };

    let deploy_content = b"deploy data";
    let cache_content = b"cache data";

    let mut deploy_reader = Cursor::new(deploy_content.to_vec());
    let mut cache_reader = Cursor::new(cache_content.to_vec());

    // Publish layer
    let returned_hash = store.publish_layer(&mut deploy_reader, &mut cache_reader, meta).await.unwrap();
    assert_eq!(returned_hash, content_hash);

    // Verify it exists
    assert!(store.has_layer(&content_hash).await.unwrap());

    // Verify input hash resolves to content hash
    let resolved = store.resolve_input_hash(&input_hash).await.unwrap().unwrap();
    assert_eq!(resolved, content_hash);

    // Pull layer
    let mut reader = store.pull_layer(&content_hash, ArtifactFormat::Deploy).await.unwrap();
    let mut read_data = Vec::new();
    reader.read_to_end(&mut read_data).await.unwrap();
    assert_eq!(read_data, deploy_content);

    let mut reader = store.pull_layer(&content_hash, ArtifactFormat::BuildCache).await.unwrap();
    let mut read_data = Vec::new();
    reader.read_to_end(&mut read_data).await.unwrap();
    assert_eq!(read_data, cache_content);
}
