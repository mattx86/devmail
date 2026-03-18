use crate::store::SharedStore;
use super::session::SmtpSession;
use tokio::net::TcpListener;

pub async fn run(addr: &str, store: SharedStore) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("SMTP listening on {addr}");
    loop {
        let (stream, peer) = listener.accept().await?;
        let store = store.clone();
        tokio::spawn(async move {
            let session = SmtpSession::new(stream, store, peer);
            if let Err(e) = session.run().await {
                tracing::debug!("SMTP session from {peer} ended: {e}");
            }
        });
    }
}
