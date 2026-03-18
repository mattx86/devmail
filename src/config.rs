use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "devmail",
    version,
    about = "Local SMTP sink + webmail UI for development",
    long_about = None
)]
pub struct Config {
    /// Enable disk storage in mbox format
    #[arg(long)]
    pub store: bool,

    /// Directory for mbox storage [default: system temp dir]
    #[arg(long, value_name = "PATH")]
    pub path: Option<PathBuf>,

    /// SMTP listen address
    #[arg(long, default_value = "127.0.0.1:1025")]
    pub smtp_addr: String,

    /// HTTP listen address
    #[arg(long, default_value = "127.0.0.1:8085")]
    pub http_addr: String,

    /// Password to protect the webmail UI (also set via DEVMAIL_PASS env var)
    #[arg(long, env = "DEVMAIL_PASS", value_name = "PASSWORD")]
    pub pass: Option<String>,
}

impl Config {
    /// Returns the storage path: --path value, or the system temp directory.
    /// std::env::temp_dir() returns %TEMP% on Windows, /tmp on Linux/macOS.
    pub fn storage_path(&self) -> PathBuf {
        self.path.clone().unwrap_or_else(std::env::temp_dir)
    }
}
