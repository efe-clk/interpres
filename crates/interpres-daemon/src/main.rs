//! `interpres-daemon`: runs the shared document queue until stopped (Ctrl+C).

use std::sync::Arc;

use anyhow::{Context, Result};
use interpres_core::{DocumentEngine, DocumentQueue, EngineError, EngineId, Store, secrets};
use interpres_daemon::server::{Daemon, serve};
use interpres_daemon::{DaemonClient, default_endpoint};
use interpres_engine_deepl::DeeplEngine;
use tokio_util::sync::CancellationToken;

fn document_engine(engine: EngineId) -> Result<Arc<dyn DocumentEngine>, EngineError> {
    match engine {
        EngineId::Deepl => {
            let key = secrets::get_key(EngineId::Deepl)
                .map_err(|e| EngineError::Service(e.to_string()))?
                .ok_or(EngineError::MissingKey)?;
            Ok(Arc::new(DeeplEngine::new(key)))
        }
        other => Err(EngineError::InvalidRequest(format!(
            "{other} cannot translate documents yet"
        ))),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let ep = default_endpoint();
    if let Ok(client) = DaemonClient::connect(&ep).await
        && client.ping().await.is_ok()
    {
        eprintln!("interpres-daemon is already running ({})", ep.0.display());
        return Ok(());
    }

    let store = Arc::new(Store::open(&Store::default_path()?).context("opening the database")?);
    let (queue, worker) = DocumentQueue::new(Box::new(document_engine));
    tokio::spawn(worker);

    let shutdown = CancellationToken::new();
    let stop = shutdown.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        stop.cancel();
    });

    serve(&ep, Arc::new(Daemon { queue, store }), shutdown).await?;
    Ok(())
}
