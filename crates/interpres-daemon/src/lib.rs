//! Interpres daemon: one background process that owns the document queue, so the
//! desktop app, the CLI and the MCP server share it.
//!
//! The glossary, cache and history live in the SQLite file, which every process
//! already shares safely, so only the queue moves here.
//!
//! Transport: a Unix socket on Linux/macOS, a named pipe on Windows. Messages are
//! newline-delimited JSON (see [`protocol`]).

pub mod client;
pub mod protocol;
pub mod server;
mod transport;

pub use client::{ClientError, DaemonClient};
pub use transport::{Endpoint, default_endpoint};

/// Connects to the running daemon at the default endpoint, or returns `None` when
/// there is none (callers then work without it).
pub async fn try_connect() -> Option<DaemonClient> {
    let client = DaemonClient::connect(&default_endpoint()).await.ok()?;
    match tokio::time::timeout(std::time::Duration::from_secs(1), client.ping()).await {
        Ok(Ok(())) => Some(client),
        _ => None,
    }
}
