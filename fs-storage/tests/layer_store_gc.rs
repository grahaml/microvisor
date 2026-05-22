use fs_storage::{
    LayerStore, InMemoryBackend, ContentHash, LayerMeta, InputHash, ArtifactFormat
};
use std::sync::Arc;
use std::str::FromStr;
use std::io::Cursor;

#[tokio::test]
async fn test_layer_store_gc() {
    let backend = Arc::new(InMemoryBackend::new());
    let store = LayerStore::new(backend.clone());

    let input_hash1 = InputHash::from_str("1111111111111111111111111111111111111111111111111111111111111111").unwrap();
    let content_hash1 = ContentHash::from_str("2222222222222222222222222222222222222222222222222222222222222222").unwrap();

    let meta = LayerMeta {
        input_hash: input_hash1.clone(),
        content_hash: content_hash1.clone(),
        parent_input_hash: None,
        uncompressed_size: 100,
        compressed_size_deploy: 50,
        compressed_size_cache: 50,
        file_checksum_deploy: content_hash1.clone(),
        file_checksum_cache: content_hash1.clone(),
        builder_version: "1.0".to_string(),
        build_timestamp: 123456789,
        build_host: "test-host".to_string(),
        stage_definition_hash: content_hash1.clone(),
        dockerfile_hash: content_hash1.clone(),
    };

    let deploy_content = b"deploy data";
    let cache_content = b"cache data";

    let mut deploy_reader = Cursor::new(deploy_content.to_vec());
    let mut cache_reader = Cursor::new(cache_content.to_vec());

    // Publish layer (it won't be referenced by a manifest)
    store.publish_layer(&mut deploy_reader, &mut cache_reader, meta).await.unwrap();

    // Verify it exists
    assert!(store.has_layer(&content_hash1).await.unwrap());

    // Check GC candidates
    let report = store.gc_candidates().await.unwrap();
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.candidates[0].content_hash, content_hash1);
    assert_eq!(report.total_reclaimable_bytes, 100);

    // Execute GC
    let summary = store.gc_execute(&report).await.unwrap();
    assert_eq!(summary.deleted_blobs, 2);
    assert_eq!(summary.deleted_bytes, 100);

    // Verify it was deleted
    assert!(!store.has_layer(&content_hash1).await.unwrap());
    assert!(store.resolve_input_hash(&input_hash1).await.unwrap().is_none());
}
