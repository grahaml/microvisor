use fs_storage::{LocalJsonBackend, StorageBackend, ContentHash, ArtifactFormat};
use tempfile::TempDir;
use tokio::io::AsyncReadExt;
use std::io::Cursor;
use std::str::FromStr;

#[tokio::test]
async fn test_local_json_blob_ops() {
    let temp_dir = TempDir::new().unwrap();
    let backend = LocalJsonBackend::new(temp_dir.path()).await.unwrap();

    let hash_str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    let hash = ContentHash::from_str(hash_str).unwrap();

    // Verify blob doesn't exist
    assert!(!backend.has_blob(&hash, ArtifactFormat::Deploy).await.unwrap());

    // Write a blob
    let content = b"hello ext4 world";
    let mut reader = Cursor::new(content.to_vec());
    backend.put_blob(&hash, ArtifactFormat::Deploy, &mut reader).await.unwrap();

    // Verify blob exists
    assert!(backend.has_blob(&hash, ArtifactFormat::Deploy).await.unwrap());

    // Read blob
    let mut reader = backend.get_blob(&hash, ArtifactFormat::Deploy).await.unwrap();
    let mut read_content = Vec::new();
    reader.read_to_end(&mut read_content).await.unwrap();
    assert_eq!(&read_content, content);

    // Delete blob
    backend.delete_blob(&hash, ArtifactFormat::Deploy).await.unwrap();
    assert!(!backend.has_blob(&hash, ArtifactFormat::Deploy).await.unwrap());
}

#[tokio::test]
async fn test_local_json_meta_ops() {
    let temp_dir = TempDir::new().unwrap();
    let backend = LocalJsonBackend::new(temp_dir.path()).await.unwrap();

    let key_layer = "layers/abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    let key_tag = "tags/production";
    let key_index = "index/1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";

    // Write metadata
    backend.put_meta(key_layer, b"layer metadata").await.unwrap();
    backend.put_meta(key_tag, b"tag metadata").await.unwrap();
    backend.put_meta(key_index, b"index metadata").await.unwrap();

    // Read metadata
    let data = backend.get_meta(key_tag).await.unwrap().unwrap();
    assert_eq!(data, b"tag metadata");

    // List metadata
    let mut layer_keys = backend.list_meta("layers/").await.unwrap();
    layer_keys.sort();
    assert_eq!(layer_keys, vec![key_layer.to_string()]);

    let mut tag_keys = backend.list_meta("tags/").await.unwrap();
    tag_keys.sort();
    assert_eq!(tag_keys, vec![key_tag.to_string()]);

    let mut index_keys = backend.list_meta("index/").await.unwrap();
    index_keys.sort();
    assert_eq!(index_keys, vec![key_index.to_string()]);

    // Delete metadata
    backend.delete_meta(key_tag).await.unwrap();
    assert!(backend.get_meta(key_tag).await.unwrap().is_none());
}
