use fs_storage::{InMemoryBackend, StorageBackend, ContentHash, ArtifactFormat};
use tokio::io::AsyncReadExt;
use std::io::Cursor;
use std::str::FromStr;

#[tokio::test]
async fn test_memory_blob_ops() {
    let backend = InMemoryBackend::new();

    let hash_str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    let hash = ContentHash::from_str(hash_str).unwrap();

    // Verify blob doesn't exist
    assert!(!backend.has_blob(&hash, ArtifactFormat::BuildCache).await.unwrap());

    // Write a blob
    let content = b"hello oci world";
    let mut reader = Cursor::new(content.to_vec());
    backend.put_blob(&hash, ArtifactFormat::BuildCache, &mut reader).await.unwrap();

    // Verify blob exists
    assert!(backend.has_blob(&hash, ArtifactFormat::BuildCache).await.unwrap());

    // Read blob
    let mut reader = backend.get_blob(&hash, ArtifactFormat::BuildCache).await.unwrap();
    let mut read_content = Vec::new();
    reader.read_to_end(&mut read_content).await.unwrap();
    assert_eq!(&read_content, content);

    // Delete blob
    backend.delete_blob(&hash, ArtifactFormat::BuildCache).await.unwrap();
    assert!(!backend.has_blob(&hash, ArtifactFormat::BuildCache).await.unwrap());
}

#[tokio::test]
async fn test_memory_meta_ops() {
    let backend = InMemoryBackend::new();

    let key_manifest = "manifests/abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

    // Write metadata
    backend.put_meta(key_manifest, b"manifest data").await.unwrap();

    // Read metadata
    let data = backend.get_meta(key_manifest).await.unwrap().unwrap();
    assert_eq!(data, b"manifest data");

    // List metadata
    let mut keys = backend.list_meta("manifests/").await.unwrap();
    keys.sort();
    assert_eq!(keys, vec![key_manifest.to_string()]);

    // Delete metadata
    backend.delete_meta(key_manifest).await.unwrap();
    assert!(backend.get_meta(key_manifest).await.unwrap().is_none());
}
