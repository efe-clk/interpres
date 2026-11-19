//! `interpres-mcp`: stdio MCP server. Logs go to stderr; stdout carries the protocol.

use std::sync::Arc;

use anyhow::Result;
use interpres_core::{EngineError, EngineId, Store, secrets};
use interpres_engine_deepl::DeeplEngine;
use interpres_mcp::InterpresMcp;
use rmcp::ServiceExt;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let store = Arc::new(Store::open(&Store::default_path()?)?);
    let engine = Arc::new(|| {
        secrets::get_key(EngineId::Deepl)
            .map_err(|e| EngineError::Service(e.to_string()))?
            .map(DeeplEngine::new)
            .ok_or(EngineError::MissingKey)
    });

    InterpresMcp::new(store, engine)
        .serve(rmcp::transport::stdio())
        .await?
        .waiting()
        .await?;
    Ok(())
}
