mod config;
mod http;
mod mime;
mod model;
mod smtp;
mod store;

use clap::Parser;
use config::Config;
use store::EmailStore;

/// Enforces email limits once per hour. Sleeps first so startup enforcement
/// (done in the store constructors) is not immediately repeated.
async fn periodic_cleanup(store: store::SharedStore) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        store.write().await.enforce_limits();
    }
}

/// Builds the SMTP address hint shown in the webmail empty state.
/// If bound to 0.0.0.0, expands to one `smtp://IP:port` line per IPv4 interface.
fn build_smtp_hint(smtp_addr: &str) -> String {
    if let Some(port) = smtp_addr.strip_prefix("0.0.0.0:") {
        if let Ok(ifaces) = if_addrs::get_if_addrs() {
            let lines: Vec<String> = ifaces
                .into_iter()
                .filter_map(|iface| match iface.addr {
                    if_addrs::IfAddr::V4(v4) => Some(format!("smtp://{}:{}", v4.ip, port)),
                    _ => None,
                })
                .collect();
            if !lines.is_empty() {
                return lines.join("<br>");
            }
        }
    }
    format!("smtp://{}", smtp_addr)
}

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
        EmailStore::new_disk(path, config.max_age, config.max_emails, config.max_size)?
    } else {
        tracing::info!("Using in-memory storage (no --store flag)");
        EmailStore::new_memory(config.max_age, config.max_emails, config.max_size)
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
    if config.max_age > 0 {
        println!("  Limit: max age {} hour(s)", config.max_age);
    }
    if config.max_emails > 0 {
        println!("  Limit: max {} email(s)", config.max_emails);
    }
    if config.max_size > 0 {
        println!("  Limit: max {} MB per email / total inbox", config.max_size);
    }
    if config.safe {
        println!("  Safe : rendering mode active (external resources blocked)");
    }
    println!("  Press Ctrl+C to stop.");

    let smtp_hint = build_smtp_hint(&config.smtp_addr);

    tokio::select! {
        r = smtp::run(&config.smtp_addr, shared_store.clone(), config.max_size * 1024 * 1024) => {
            tracing::error!("SMTP server exited: {:?}", r);
        }
        r = http::run(&config.http_addr, shared_store.clone(), config.pass.clone(), smtp_hint, config.safe) => {
            tracing::error!("HTTP server exited: {:?}", r);
        }
        _ = periodic_cleanup(shared_store.clone()) => {}
        _ = tokio::signal::ctrl_c() => {
            println!("\nShutting down.");
        }
    }

    Ok(())
}
