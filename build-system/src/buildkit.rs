use crate::traits::*;
use async_trait::async_trait;
use std::path::PathBuf;
use std::process::Command;

pub struct BuildctlEngine;

#[async_trait]
impl BuildEngine for BuildctlEngine {
    async fn solve_stage(&self, req: SolveRequest) -> anyhow::Result<PathBuf> {
        let mut cmd = Command::new("buildctl");
        cmd.arg("build")
            .arg("--frontend").arg("dockerfile.v0")
            .arg("--local").arg(format!("context={}", req.context_path.display()))
            .arg("--local").arg(format!("dockerfile={}", req.dockerfile_path.parent().unwrap_or(&std::path::Path::new(".")).display()))
            .arg("--opt").arg(format!("filename={}", req.dockerfile_path.file_name().unwrap_or_default().to_string_lossy()))
            .arg("--opt").arg(format!("target={}", req.stage_name))
            .arg("--output").arg(format!("type=local,dest={}", req.export_dir.display()));

        if let Some((parent_stage, oci_dir)) = req.parent_cache_override {
            cmd.arg("--opt").arg(format!("context:{}=oci-layout://{}", parent_stage, oci_dir.display()));
        }

        let status = cmd.status()?;
        if !status.success() {
            return Err(anyhow::anyhow!("buildctl failed"));
        }

        Ok(req.export_dir)
    }
}
