use crate::store::SharedStore;
use super::api::build_router;

pub async fn run(addr: &str, store: SharedStore, password: Option<String>, smtp_hint: String, safe_mode: bool) -> anyhow::Result<()> {
    let app = build_router(store, password, smtp_hint, safe_mode);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("HTTP listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
