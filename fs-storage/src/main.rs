use clap::Parser;
use std::path::PathBuf;
use std::net::SocketAddr;
use std::sync::Arc;
use fs_storage::{LayerStore, LocalJsonBackend};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value_t = 50051)]
    port: u16,

    /// Root directory for storage
    #[arg(short, long, default_value = "./storage")]
    root: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    
    // 1. Initialize Backend
    let backend = Arc::new(LocalJsonBackend::new(&args.root).await?);
    
    // 2. Wrap in LayerStore
    let store = Arc::new(LayerStore::new(backend));
    
    // 3. Define address
    let addr: SocketAddr = format!("0.0.0.0:{}", args.port).parse()?;
    
    println!("fs-storage gRPC server listening on {}", addr);
    println!("Storage root: {:?}", args.root);

    // 4. Run Server
    fs_storage::server::run_server(store, addr).await?;

    Ok(())
}
