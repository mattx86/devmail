mod config;
mod http;
mod mime;
mod model;
mod smtp;
mod store;

use clap::Parser;
use config::Config;
use store::EmailStore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("devmail=info".parse()?),
        )
        .init();

    let config = Config::parse();

    let shared_store = if config.store {
        let path = config.storage_path();
        std::fs::create_dir_all(&path)?;
        tracing::info!("Disk storage enabled: {}", path.display());
        EmailStore::new_disk(path)?
    } else {
        tracing::info!("Using in-memory storage (no --store flag)");
        EmailStore::new_memory()
    };

    println!("devmail v{} listening", env!("CARGO_PKG_VERSION"));
    println!("  SMTP : smtp://{}  (no auth, no TLS)", config.smtp_addr);
    println!("  Web  : http://{}", config.http_addr);
    if config.store {
        let mbox = config.storage_path().join("devmail.mbox");
        println!("  mbox : {}", mbox.display());
        let n = shared_store.read().await.len();
        if n > 0 {
            println!("  Restored: {} stored email(s)", n);
        }
    }
    if config.pass.is_some() {
        println!("  Auth : password required");
    }
    println!("  Press Ctrl+C to stop.");

    tokio::select! {
        r = smtp::run(&config.smtp_addr, shared_store.clone()) => {
            tracing::error!("SMTP server exited: {:?}", r);
        }
        r = http::run(&config.http_addr, shared_store.clone(), config.pass.clone()) => {
            tracing::error!("HTTP server exited: {:?}", r);
        }
        _ = tokio::signal::ctrl_c() => {
            println!("\nShutting down.");
        }
    }

    Ok(())
}
