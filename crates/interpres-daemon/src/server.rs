//! Daemon side: accepts connections and serves requests against one document queue.

use std::path::PathBuf;
use std::sync::Arc;

use interpres_core::{DocumentQueue, DocumentRequest, EngineError, EngineId, Store};
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::protocol::{Event, Request, RequestFrame, ServerFrame};
use crate::transport::{Endpoint, Listener, Stream};

/// Shared state of the daemon.
pub struct Daemon {
    pub queue: DocumentQueue,
    pub store: Arc<Store>,
}

/// Listens on `ep` until `shutdown` fires. Open connections close with it.
pub async fn serve(
    ep: &Endpoint,
    daemon: Arc<Daemon>,
    shutdown: CancellationToken,
) -> std::io::Result<()> {
    let mut listener = Listener::bind(ep)?;
    tracing::info!(endpoint = %ep.0.display(), "daemon listening");
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            conn = listener.accept() => {
                let stream = conn?;
                tokio::spawn(handle(stream, daemon.clone(), shutdown.clone()));
            }
        }
    }
    #[cfg(not(windows))]
    let _ = std::fs::remove_file(&ep.0);
    Ok(())
}

fn to_value(v: impl Serialize) -> Result<Value, EngineError> {
    serde_json::to_value(v).map_err(|e| EngineError::Service(e.to_string()))
}

fn frame_line(frame: &ServerFrame) -> Option<String> {
    serde_json::to_string(frame).ok().map(|s| s + "\n")
}

async fn handle(stream: Box<dyn Stream>, daemon: Arc<Daemon>, shutdown: CancellationToken) {
    let (read, mut write) = tokio::io::split(stream);
    let (out, mut out_rx) = mpsc::unbounded_channel::<String>();
    let conn_done = shutdown.child_token();

    let writer_done = conn_done.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = writer_done.cancelled() => break,
                line = out_rx.recv() => match line {
                    Some(line) if write.write_all(line.as_bytes()).await.is_ok() => {}
                    _ => break,
                },
            }
        }
        let _ = write.shutdown().await;
    });

    let mut lines = BufReader::new(read).lines();
    let mut subscribed = false;
    loop {
        let line = tokio::select! {
            _ = conn_done.cancelled() => break,
            line = lines.next_line() => match line {
                Ok(Some(line)) => line,
                _ => break,
            },
        };
        let frame: RequestFrame = match serde_json::from_str(&line) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(error = %e, "bad request");
                continue;
            }
        };
        if matches!(frame.request, Request::Subscribe) && !subscribed {
            subscribed = true;
            let mut rx = daemon.queue.subscribe();
            let out = out.clone();
            let done = conn_done.clone();
            tokio::spawn(async move {
                // Send the current list first, then every change.
                let mut jobs = rx.borrow_and_update().clone();
                loop {
                    let event = ServerFrame::Event {
                        event: Event::Documents(jobs),
                    };
                    if frame_line(&event).is_none_or(|l| out.send(l).is_err()) {
                        break;
                    }
                    tokio::select! {
                        _ = done.cancelled() => break,
                        changed = rx.changed() => {
                            if changed.is_err() { break; }
                            jobs = rx.borrow_and_update().clone();
                        }
                    }
                }
            });
        }
        let result = respond(&daemon, frame.request);
        if let Some(line) = frame_line(&ServerFrame::Response {
            id: frame.id,
            result,
        }) {
            let _ = out.send(line);
        }
    }
    conn_done.cancel();
}

fn respond(daemon: &Daemon, request: Request) -> Result<Value, EngineError> {
    match request {
        Request::Ping | Request::Subscribe => to_value(()),
        Request::DocumentsList => to_value(daemon.queue.jobs()),
        Request::DocumentsAdd {
            paths,
            engine,
            request,
            use_glossary,
        } => to_value(add(daemon, paths, engine, request, use_glossary)?),
        Request::DocumentsCancel { id } => {
            daemon.queue.cancel(id);
            to_value(())
        }
        Request::DocumentsRemove { id } => {
            daemon.queue.remove(id);
            to_value(())
        }
    }
}

fn add(
    daemon: &Daemon,
    paths: Vec<PathBuf>,
    engine: EngineId,
    mut request: DocumentRequest,
    use_glossary: bool,
) -> Result<Vec<u64>, EngineError> {
    // Glossaries need an explicit source language.
    if use_glossary && let Some(src) = &request.source {
        request.glossary = daemon
            .store
            .glossary(src, &request.target)
            .map_err(|e| EngineError::Service(e.to_string()))?;
    }
    let label = request
        .glossary
        .as_ref()
        .map(|g| format!("{} → {}", g.source_lang, g.target_lang));
    Ok(paths
        .into_iter()
        .map(|p| daemon.queue.add(p, engine, request.clone(), label.clone()))
        .collect())
}
