pub mod buildkit;
pub mod converter;
pub mod hasher;
pub mod orchestrator;
pub mod parser;
pub mod store;
pub mod traits;
pub mod types;

#[cfg(test)]
pub mod mocks;

#[cfg(test)]
mod tests {
    use super::mocks::*;
    use super::orchestrator::BuildOrchestrator;
    use super::types::*;
    use std::collections::HashMap;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use tempfile::{NamedTempFile, tempdir};

    #[tokio::test]
    async fn test_orchestrator_build_flow() {
        let scratch = tempdir().unwrap();
        let solve_count = Arc::new(Mutex::new(0));
        let engine = Arc::new(MockBuildEngine {
            solve_count: Arc::clone(&solve_count),
        });
        let format = Arc::new(MockLayerFormat);
        let store = Arc::new(MockStoreClient {
            input_to_output: Arc::new(Mutex::new(HashMap::new())),
            layers: Arc::new(Mutex::new(HashMap::new())),
        });
        let hasher = Arc::new(MockHasher);

        let graph = BuildGraph {
            stages: vec![
                ParsedStage {
                    name: "base".to_string(),
                    parent_name: None,
                    layer_name: "base-os".to_string(),
                    order: 0,
                    instructions: vec!["FROM debian".to_string()],
                },
                ParsedStage {
                    name: "app".to_string(),
                    parent_name: Some("base".to_string()),
                    layer_name: "app-code".to_string(),
                    order: 1,
                    instructions: vec!["COPY . .".to_string()],
                },
            ],
            release_meta: ReleaseMetadata {
                layer_order: vec!["base-os".to_string(), "app-code".to_string()],
                workload_meta: HashMap::new(),
            },
        };
        let parser = Arc::new(MockParser { graph });

        let orchestrator = BuildOrchestrator::with_scratch_dir(
            parser, hasher, engine, format, store, scratch.path().to_path_buf(),
        );

        let mut alloy = NamedTempFile::new().unwrap();
        writeln!(alloy, "FROM debian AS base\nFROM base AS app").unwrap();

        let manifest = orchestrator.build(alloy.path()).await.unwrap();

        assert_eq!(manifest.layers.len(), 2);
        assert_eq!(manifest.layers[0].name, "base-os");
        assert_eq!(manifest.layers[1].name, "app-code");
        assert_eq!(*solve_count.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn test_orchestrator_cache_hit() {
        let solve_count = Arc::new(Mutex::new(0));
        let engine = Arc::new(MockBuildEngine {
            solve_count: Arc::clone(&solve_count),
        });
        let format = Arc::new(MockLayerFormat);
        
        let input_hash = InputHash("mock_input_hash_base-os".to_string());
        let output_hash = OutputHash("pre-built-output".to_string());
        
        let mut input_to_output = HashMap::new();
        input_to_output.insert(input_hash.clone(), output_hash.clone());
        
        let mut layers = HashMap::new();
        layers.insert(output_hash.clone(), LayerMetadata {
            input_hash: input_hash.clone(),
            output_hash: output_hash.clone(),
            parent_input_hash: None,
            size_bytes: 100,
            size_compressed_bytes: 50,
            build_timestamp: 12345,
            builder_version: "0.1.0".to_string(),
            build_host: "remote-builder".to_string(),
        });

        let store = Arc::new(MockStoreClient {
            input_to_output: Arc::new(Mutex::new(input_to_output)),
            layers: Arc::new(Mutex::new(layers)),
        });
        let hasher = Arc::new(MockHasher);

        let graph = BuildGraph {
            stages: vec![
                ParsedStage {
                    name: "base".to_string(),
                    parent_name: None,
                    layer_name: "base-os".to_string(),
                    order: 0,
                    instructions: vec!["FROM debian".to_string()],
                },
            ],
            release_meta: ReleaseMetadata {
                layer_order: vec!["base-os".to_string()],
                workload_meta: HashMap::new(),
            },
        };
        let parser = Arc::new(MockParser { graph });

        let orchestrator = BuildOrchestrator::new(parser, hasher, engine, format, store);

        let mut alloy = NamedTempFile::new().unwrap();
        writeln!(alloy, "FROM debian AS base").unwrap();

        let manifest = orchestrator.build(alloy.path()).await.unwrap();

        assert_eq!(manifest.layers.len(), 1);
        assert_eq!(manifest.layers[0].output_hash, output_hash);
        assert_eq!(*solve_count.lock().unwrap(), 0); // Should be a cache hit
    }
}
