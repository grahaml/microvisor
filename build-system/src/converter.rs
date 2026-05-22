use crate::traits::LayerFormat;
use crate::types::*;
use crate::hasher::RealHasher;
use std::path::Path;
use std::process::Command;

pub struct ZstdExt4OciFormat;

impl LayerFormat for ZstdExt4OciFormat {
    fn pack(&self, source_dir: &Path, output_dir: &Path) -> anyhow::Result<LayerArtifacts> {
        // 1. ext4 artifact
        let ext4_path = output_dir.join("layer.ext4");
        let ext4_zst_path = output_dir.join("layer.ext4.zst");
        
        // Calculate size: du -sb + 10% + 4MB round up
        let size_bytes = get_dir_size(source_dir)?;
        let ext4_size_mb = ((size_bytes as f64 * 1.1) / (1024.0 * 1024.0)).ceil() as u64;
        let ext4_size_mb = ((ext4_size_mb + 3) / 4) * 4; // Round up to 4MB
        let ext4_size_mb = ext4_size_mb.max(64); // Min 64MB

        // mke2fs -t ext4 -d <source_dir> <ext4_path> <size>M
        let status = Command::new("mke2fs")
            .arg("-t").arg("ext4")
            .arg("-d").arg(source_dir)
            .arg(&ext4_path)
            .arg(format!("{}M", ext4_size_mb))
            .status()?;
        
        if !status.success() {
            return Err(anyhow::anyhow!("mke2fs failed"));
        }

        let hasher = RealHasher;
        let output_hash = hasher.compute_output_hash(&ext4_path)?;

        // zstd -19 <ext4_path> -o <ext4_zst_path>
        let status = Command::new("zstd")
            .arg("-19")
            .arg(&ext4_path)
            .arg("-o").arg(&ext4_zst_path)
            .status()?;
        
        if !status.success() {
            return Err(anyhow::anyhow!("zstd failed for ext4"));
        }

        // 2. OCI artifact
        let oci_tar_path = output_dir.join("layer.oci.tar");
        let oci_tar_zst_path = output_dir.join("layer.oci.tar.zst");

        // Simple tar for POC (real OCI needs manifest/config)
        let status = Command::new("tar")
            .arg("-cf").arg(&oci_tar_path)
            .arg("-C").arg(source_dir)
            .arg(".")
            .status()?;
        
        if !status.success() {
            return Err(anyhow::anyhow!("tar failed"));
        }

        // zstd -19 <oci_tar_path> -o <oci_tar_zst_path>
        let status = Command::new("zstd")
            .arg("-19")
            .arg(&oci_tar_path)
            .arg("-o").arg(&oci_tar_zst_path)
            .status()?;
        
        if !status.success() {
            return Err(anyhow::anyhow!("zstd failed for OCI"));
        }

        // Cleanup raw files
        let _ = std::fs::remove_file(ext4_path);
        let _ = std::fs::remove_file(oci_tar_path);

        Ok(LayerArtifacts {
            deploy_artifact: ext4_zst_path,
            build_cache_artifact: oci_tar_zst_path,
            output_hash,
        })
    }

    fn unpack_for_build(&self, artifact: &Path, target: &Path) -> anyhow::Result<()> {
        // zstd -d <artifact> -o <tmp_tar>
        let tmp_tar = target.join("tmp.tar");
        let status = Command::new("zstd")
            .arg("-d")
            .arg(artifact)
            .arg("-o").arg(&tmp_tar)
            .status()?;
        
        if !status.success() {
            return Err(anyhow::anyhow!("zstd decompress failed"));
        }

        // tar -xf <tmp_tar> -C <target>
        let status = Command::new("tar")
            .arg("-xf").arg(&tmp_tar)
            .arg("-C").arg(target)
            .status()?;
        
        if !status.success() {
            return Err(anyhow::anyhow!("tar unpack failed"));
        }

        let _ = std::fs::remove_file(tmp_tar);
        Ok(())
    }
}

pub fn get_dir_size(path: &Path) -> anyhow::Result<u64> {
    let mut total_size = 0;
    for entry in walkdir::WalkDir::new(path) {
        let entry = entry?;
        if entry.file_type().is_file() {
            total_size += entry.metadata()?.len();
        }
    }
    Ok(total_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs::File;
    use std::io::Write;

    #[test]
    #[ignore] // Requires mke2fs and zstd on the host
    fn test_pack_unpack() {
        let source = tempdir().unwrap();
        let output = tempdir().unwrap();
        let target = tempdir().unwrap();

        let file_path = source.path().join("test.txt");
        let mut file = File::create(file_path).unwrap();
        writeln!(file, "hello world").unwrap();

        let converter = ZstdExt4OciFormat;
        let artifacts = converter.pack(source.path(), output.path()).unwrap();

        assert!(artifacts.deploy_artifact.exists());
        assert!(artifacts.build_cache_artifact.exists());

        converter.unpack_for_build(&artifacts.build_cache_artifact, target.path()).unwrap();
        assert!(target.path().join("test.txt").exists());
    }
}
