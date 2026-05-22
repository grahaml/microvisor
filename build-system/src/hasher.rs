use crate::traits::Hasher;
use crate::types::*;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

pub struct RealHasher;

impl Hasher for RealHasher {
    fn compute_input_hash(&self, stage: &ParsedStage, parent_hash: Option<&InputHash>) -> InputHash {
        let mut hasher = Sha256::new();

        // 1. Parent hash
        if let Some(h) = parent_hash {
            hasher.update(h.0.as_bytes());
        } else {
            hasher.update(b"");
        }

        // 2. Separator
        hasher.update(b"\0");

        // 3. Normalized instructions
        for instr in &stage.instructions {
            let normalized = normalize_instruction(instr);
            if !normalized.is_empty() {
                hasher.update(normalized.as_bytes());
                hasher.update(b"\n");
            }
        }

        InputHash(hex::encode(hasher.finalize()))
    }
}

impl RealHasher {
    pub fn compute_output_hash(&self, path: &Path) -> anyhow::Result<OutputHash> {
        let mut file = std::fs::File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0; 8192];

        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }

        Ok(OutputHash(hex::encode(hasher.finalize())))
    }
}

fn normalize_instruction(instr: &str) -> String {
    // 1. Strip comments
    let mut line = instr;
    if let Some(idx) = instr.find('#') {
        line = &instr[..idx];
    }

    // 2. Normalize whitespace
    let parts: Vec<&str> = line.split_whitespace().collect();
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_hash_determinism() {
        let hasher = RealHasher;
        let stage = ParsedStage {
            name: "test".to_string(),
            parent_name: None,
            layer_name: "test-layer".to_string(),
            order: 0,
            instructions: vec![
                "FROM debian".to_string(),
                "RUN  apt-get  update  ".to_string(),
                " # a comment ".to_string(),
            ],
        };

        let h1 = hasher.compute_input_hash(&stage, None);
        let h2 = hasher.compute_input_hash(&stage, None);

        assert_eq!(h1, h2);
        assert!(!h1.0.is_empty());
    }

    #[test]
    fn test_input_hash_sensitivity() {
        let hasher = RealHasher;
        let stage1 = ParsedStage {
            name: "test".to_string(),
            parent_name: None,
            layer_name: "test-layer".to_string(),
            order: 0,
            instructions: vec!["FROM debian".to_string()],
        };
        let stage2 = ParsedStage {
            name: "test".to_string(),
            parent_name: None,
            layer_name: "test-layer".to_string(),
            order: 0,
            instructions: vec!["FROM alpine".to_string()],
        };

        let h1 = hasher.compute_input_hash(&stage1, None);
        let h2 = hasher.compute_input_hash(&stage2, None);

        assert_ne!(h1, h2);
    }
}
