use crate::traits::DockerfileParser;
use crate::types::*;
use std::collections::HashMap;
use std::path::Path;

pub struct RealDockerfileParser;

impl DockerfileParser for RealDockerfileParser {
    fn parse(&self, dockerfile: &Path) -> anyhow::Result<BuildGraph> {
        let content = std::fs::read_to_string(dockerfile)?;
        let mut stages = Vec::new();
        let mut current_stage: Option<ParsedStage> = None;
        let mut steel_release_stage: Option<ReleaseMetadata> = None;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line.to_uppercase().starts_with("FROM") {
                if let Some(stage) = current_stage.take() {
                    stages.push(stage);
                }

                let parts: Vec<&str> = line.split_whitespace().collect();
                // FROM <image> [AS <name>]
                let name = if parts.len() >= 4 && parts[2].to_uppercase() == "AS" {
                    parts[3].to_string()
                } else {
                    format!("stage_{}", stages.len())
                };

                let parent_name = if stages.is_empty() {
                    None
                } else {
                    // This is a simplification; real Dockerfiles can reference any previous stage.
                    // For POC, we assume sequential stacking unless it's FROM scratch/image.
                    if parts[1].to_lowercase() == "scratch" {
                        None
                    } else {
                        // Check if parts[1] is a known stage name
                        stages.iter().find(|s| s.name == parts[1]).map(|s| s.name.clone())
                    }
                };

                current_stage = Some(ParsedStage {
                    name,
                    parent_name,
                    layer_name: String::new(),
                    order: 0,
                    instructions: vec![line.to_string()],
                });
            } else if line.to_uppercase().starts_with("LABEL") {
                if let Some(stage) = current_stage.as_mut() {
                    stage.instructions.push(line.to_string());
                    
                    // LABEL steel.layer="base-os"
                    if let Some((key, value)) = parse_label(line) {
                        match key.as_str() {
                            "steel.layer" => stage.layer_name = value,
                            "steel.layer.order" => stage.order = value.parse()?,
                            "steel.release" if value == "true" => {
                                // This stage will be the release stage
                            }
                            _ => {}
                        }
                    }
                }
                
                // Check if it's the release stage metadata
                if let Some((key, value)) = parse_label(line) {
                    if key.starts_with("steel.release") || key.starts_with("steel.meta") {
                        if steel_release_stage.is_none() {
                            steel_release_stage = Some(ReleaseMetadata {
                                layer_order: Vec::new(),
                                workload_meta: HashMap::new(),
                            });
                        }
                        
                        let meta = steel_release_stage.as_mut().unwrap();
                        if key == "steel.release.layers" {
                            meta.layer_order = value.split(',').map(|s| s.trim().to_string()).collect();
                        } else if key.starts_with("steel.meta.") {
                            let meta_key = key.trim_start_matches("steel.meta.").to_string();
                            meta.workload_meta.insert(meta_key, value);
                        }
                    }
                }
            } else if let Some(stage) = current_stage.as_mut() {
                stage.instructions.push(line.to_string());
            }
        }

        if let Some(stage) = current_stage {
            stages.push(stage);
        }

        let release_meta = steel_release_stage.ok_or_else(|| anyhow::anyhow!("Missing steel.release=\"true\" label"))?;

        // Validation
        validate_graph(&stages, &release_meta)?;

        Ok(BuildGraph {
            stages,
            release_meta,
        })
    }
}

fn parse_label(line: &str) -> Option<(String, String)> {
    // LABEL steel.layer="base-os"
    let line = line.trim_start_matches("LABEL").trim();
    if let Some(idx) = line.find('=') {
        let key = line[..idx].trim().to_string();
        let value = line[idx + 1..].trim().trim_matches('"').to_string();
        Some((key, value))
    } else {
        None
    }
}

fn validate_graph(stages: &[ParsedStage], meta: &ReleaseMetadata) -> anyhow::Result<()> {
    let layer_stages: HashMap<String, &ParsedStage> = stages
        .iter()
        .filter(|s| !s.layer_name.is_empty())
        .map(|s| (s.layer_name.clone(), s))
        .collect();

    for layer_name in &meta.layer_order {
        if !layer_stages.contains_key(layer_name) {
            return Err(anyhow::anyhow!("Layer {} referenced in release but not defined in any stage", layer_name));
        }
    }

    let mut orders: Vec<u32> = layer_stages.values().map(|s| s.order).collect();
    orders.sort_unstable();
    for (i, &order) in orders.iter().enumerate() {
        if order != i as u32 {
            return Err(anyhow::anyhow!("Layer orders must be contiguous starting from 0, found gap or wrong start at {}", order));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_valid_dockerfile() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"
FROM debian:bookworm-slim AS base_os
LABEL steel.layer="base-os"
LABEL steel.layer.order="0"
RUN apt-get update

FROM base_os AS platform
LABEL steel.layer="platform"
LABEL steel.layer.order="1"
COPY steel-init /usr/local/bin/

FROM scratch AS release
LABEL steel.release="true"
LABEL steel.release.layers="base-os,platform"
LABEL steel.meta.vcpus="4"
        "#).unwrap();

        let parser = RealDockerfileParser;
        let graph = parser.parse(file.path()).unwrap();

        assert_eq!(graph.stages.len(), 3);
        assert_eq!(graph.release_meta.layer_order, vec!["base-os", "platform"]);
        assert_eq!(graph.release_meta.workload_meta.get("vcpus").unwrap(), "4");
    }
}
