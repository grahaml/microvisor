use std::{
    env, fs,
    path::PathBuf,
    process::{Command, ExitStatus},
};

use anyhow::{bail, Context, Result};

fn main() -> Result<()> {
    let task = env::args().nth(1);
    match task.as_deref() {
        Some("build-ebpf") => build_ebpf(false),
        Some("build-ebpf-release") => build_ebpf(true),
        _ => {
            eprintln!("Usage:");
            eprintln!("  cargo xtask build-ebpf          # debug build");
            eprintln!("  cargo xtask build-ebpf-release  # release build");
            std::process::exit(1);
        }
    }
}

fn build_ebpf(release: bool) -> Result<()> {
    // xtask lives at control-plane/xtask/; workspace root is its parent.
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("xtask has no parent directory")?
        .to_path_buf();

    let ebpf_dir = workspace_root.join("microvisor-ebpf");

    let profile = if release { "release" } else { "debug" };

    println!("[xtask] Building eBPF programs ({profile}) from {}", ebpf_dir.display());

    let mut cmd = Command::new("cargo");
    cmd.current_dir(&ebpf_dir)
        // The xtask runs under the stable cargo process, which inherits
        // RUSTUP_TOOLCHAIN=stable and overrides the rust-toolchain.toml in
        // microvisor-ebpf/. Unsetting it lets rustup read the toolchain file.
        .env_remove("RUSTUP_TOOLCHAIN")
        .args(["build", "--target", "bpfel-unknown-none", "-Z", "build-std=core"]);

    if release {
        cmd.arg("--release");
    }

    let status: ExitStatus = cmd
        .status()
        .context("failed to invoke cargo for eBPF build")?;

    if !status.success() {
        bail!("eBPF build failed with status: {status}");
    }

    let obj_src = ebpf_dir
        .join("target")
        .join("bpfel-unknown-none")
        .join(profile)
        .join("microvisor-ebpf");

    // Copy the compiled ELF into a location the control-plane build.rs can find.
    let obj_dst_dir = workspace_root.join("src").join("bpf");
    fs::create_dir_all(&obj_dst_dir)
        .context("failed to create src/bpf/ output directory")?;

    let obj_dst = obj_dst_dir.join("microvisor-ebpf");
    fs::copy(&obj_src, &obj_dst).with_context(|| {
        format!(
            "failed to copy eBPF object from {} to {}",
            obj_src.display(),
            obj_dst.display()
        )
    })?;

    println!(
        "[xtask] eBPF object written to {}",
        obj_dst.display()
    );

    Ok(())
}
