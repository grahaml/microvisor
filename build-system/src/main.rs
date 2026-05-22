use anyhow::Result;
use build_system::buildkit::BuildctlEngine;
use build_system::converter::ZstdExt4OciFormat;
use build_system::hasher::RealHasher;
use build_system::orchestrator::BuildSystem;
use build_system::parser::RealDockerfileParser;
use build_system::store::GrpcStoreClient;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "steel-build")]
#[command(about = "Microvisor Build System", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, default_value = "http://[::1]:50051")]
    store_url: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Build a rootfs from a Dockerfile
    Build {
        #[arg(short, long)]
        file: PathBuf,
    },
    /// Tag a manifest
    Tag {
        manifest_hash: String,
        tag_name: String,
    },
    /// List tags
    ListTags,
    /// Diff two manifests
    Diff {
        a: String,
        b: String,
    },
    /// Garbage collect unreferenced layers
    Gc,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize dependencies
    let store_client = Arc::new(GrpcStoreClient::connect(cli.store_url).await?);

    let parser = Arc::new(RealDockerfileParser);
    let hasher = Arc::new(RealHasher);
    let engine = Arc::new(BuildctlEngine);
    let format = Arc::new(ZstdExt4OciFormat);

    let system = BuildSystem::new(parser, hasher, engine, format, store_client);

    match cli.command {
        Commands::Build { file } => {
            let manifest = system.build(&file).await?;
            println!("Build successful!");
            println!("Manifest ID: {}", manifest.manifest_id);
            println!("Layers: {}", manifest.layers.len());
        }
        Commands::Tag { manifest_hash, tag_name } => {
            // TODO: Implement tag logic in BuildSystem or call LayerStore directly
            println!("Tagging {} as {} (not fully implemented in CLI)", manifest_hash, tag_name);
        }
        Commands::ListTags => {
            println!("Listing tags (not fully implemented in CLI)");
        }
        Commands::Diff { a, b } => {
            println!("Diffing {} and {} (not fully implemented in CLI)", a, b);
        }
        Commands::Gc => {
            println!("Running GC (not fully implemented in CLI)");
        }
    }

    Ok(())
}
