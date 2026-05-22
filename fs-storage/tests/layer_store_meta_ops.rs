use fs_storage::{
    LayerStore, InMemoryBackend, ReleaseManifest, BuildMetadata, ContentHash, BuildFilter, 
    ManifestLayer, InputHash
};
use std::sync::Arc;
use std::str::FromStr;
use std::collections::HashMap;

#[tokio::test]
async fn test_layer_store_manifest_and_tags() {
    let backend = Arc::new(InMemoryBackend::new());
    let store = LayerStore::new(backend.clone());

    let dockerfile_hash = ContentHash::from_str("1111111111111111111111111111111111111111111111111111111111111111").unwrap();
    let layer_content_hash = ContentHash::from_str("2222222222222222222222222222222222222222222222222222222222222222").unwrap();
    let layer_input_hash = InputHash::from_str("3333333333333333333333333333333333333333333333333333333333333333").unwrap();

    let manifest = ReleaseManifest {
        schema_version: "1.0".to_string(),
        manifest_id: "test-release".to_string(),
        build_metadata: BuildMetadata {
            dockerfile_hash: dockerfile_hash.clone(),
            build_timestamp: 1000,
            builder_version: "1.0.0".to_string(),
            build_host: "test-host".to_string(),
        },
        layers: vec![
            ManifestLayer {
                name: "base".to_string(),
                order: 0,
                input_hash: layer_input_hash.clone(),
                content_hash: layer_content_hash.clone(),
                size_bytes: 100,
                size_compressed_bytes: 50,
                parent_input_hash: None,
            }
        ],
        workload_metadata: HashMap::new(),
    };

    // 1. Publish manifest
    let manifest_hash = store.publish_manifest(manifest.clone()).await.unwrap();

    // 2. Get manifest
    let retrieved = store.get_manifest(&manifest_hash).await.unwrap();
    assert_eq!(retrieved.manifest_id, "test-release");

    // 3. List builds
    let builds = store.list_builds(BuildFilter::default()).await.unwrap();
    assert_eq!(builds.len(), 1);
    assert_eq!(builds[0].release_name, "test-release");
    assert_eq!(builds[0].manifest_hash, manifest_hash);

    // 4. Tags
    let tag_name = "latest";
    store.create_tag(tag_name, &manifest_hash).await.unwrap();
    
    let resolved_hash = store.get_tag(tag_name).await.unwrap();
    assert_eq!(resolved_hash, manifest_hash);

    let tags = store.list_tags().await.unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].0, "latest");
    assert_eq!(tags[0].1, manifest_hash);

    store.delete_tag(tag_name).await.unwrap();
    assert!(store.get_tag(tag_name).await.is_err());
}
