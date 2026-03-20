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

    /// Delete emails older than N hours on each check (0 = disabled)
    #[arg(long, default_value_t = 8, value_name = "HOURS")]
    pub max_age: u64,

    /// Keep only the N most recent emails (0 = disabled)
    #[arg(long, default_value_t = 50, value_name = "COUNT")]
    pub max_emails: usize,

    /// Maximum size of a single email and total inbox in MB (0 = disabled)
    #[arg(long, default_value_t = 32, value_name = "MB")]
    pub max_size: usize,

    /// Enable safe rendering mode: blocks external images, links, and CSS (for security research)
    #[arg(long)]
    pub safe: bool,
}

impl Config {
    /// Returns the storage path: --path value, or the system temp directory.
    /// std::env::temp_dir() returns %TEMP% on Windows, /tmp on Linux/macOS.
    pub fn storage_path(&self) -> PathBuf {
        self.path.clone().unwrap_or_else(std::env::temp_dir)
    }
}
