//! Client side: used by the desktop app, the CLI and the MCP server.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use interpres_core::documents::{DocumentJob, JobStatus};
use interpres_core::{DocumentRequest, EngineError, EngineId};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::protocol::{Event, Request, RequestFrame, ServerFrame};
use crate::transport::{Endpoint, connect};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClientError {
    /// The daemon is gone; callers fall back to working without it.
    #[error("the daemon disconnected")]
    Disconnected,
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error("unexpected reply: {0}")]
    Protocol(String),
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, EngineError>>>>>;
/// `None` once the connection is closed; dropping the sender also ends every
/// subscription.
type Events = Arc<Mutex<Option<broadcast::Sender<Vec<DocumentJob>>>>>;

/// A connection to the daemon. Cheap to share behind an `Arc`.
pub struct DaemonClient {
    out: mpsc::UnboundedSender<String>,
    pending: Pending,
    next_id: AtomicU64,
    events: Events,
}

impl DaemonClient {
    /// Connects, or fails quickly if no daemon is running.
    pub async fn connect(ep: &Endpoint) -> std::io::Result<Self> {
        let stream = connect(ep).await?;
        let (read, mut write) = tokio::io::split(stream);
        let (out, mut out_rx) = mpsc::unbounded_channel::<String>();
        let pending: Pending = Arc::default();
        let events: Events = Arc::new(Mutex::new(Some(broadcast::channel(64).0)));

        tokio::spawn(async move {
            while let Some(line) = out_rx.recv().await {
                if write.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
            }
        });

        let pending_r = pending.clone();
        let events_r = events.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(read).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                match serde_json::from_str::<ServerFrame>(&line) {
                    Ok(ServerFrame::Response { id, result }) => {
                        if let Some(tx) = pending_r.lock().unwrap().remove(&id) {
                            let _ = tx.send(result);
                        }
                    }
                    Ok(ServerFrame::Event {
                        event: Event::Documents(jobs),
                    }) => {
                        if let Some(tx) = events_r.lock().unwrap().as_ref() {
                            let _ = tx.send(jobs);
                        }
                    }
                    Err(e) => tracing::warn!(error = %e, "bad frame from daemon"),
                }
            }
            // Connection closed: mark it first, then fail every waiting call.
            events_r.lock().unwrap().take();
            pending_r.lock().unwrap().clear();
        });

        Ok(Self {
            out,
            pending,
            next_id: AtomicU64::new(1),
            events,
        })
    }

    async fn call<T: DeserializeOwned>(&self, request: Request) -> Result<T, ClientError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        if self.events.lock().unwrap().is_none() {
            self.pending.lock().unwrap().remove(&id);
            return Err(ClientError::Disconnected);
        }
        let line = serde_json::to_string(&RequestFrame { id, request })
            .map_err(|e| ClientError::Protocol(e.to_string()))?
            + "\n";
        if self.out.send(line).is_err() {
            self.pending.lock().unwrap().remove(&id);
            return Err(ClientError::Disconnected);
        }
        let value = rx.await.map_err(|_| ClientError::Disconnected)??;
        serde_json::from_value(value).map_err(|e| ClientError::Protocol(e.to_string()))
    }

    pub async fn ping(&self) -> Result<(), ClientError> {
        self.call(Request::Ping).await
    }

    pub async fn documents_list(&self) -> Result<Vec<DocumentJob>, ClientError> {
        self.call(Request::DocumentsList).await
    }

    pub async fn documents_add(
        &self,
        paths: Vec<PathBuf>,
        engine: EngineId,
        request: DocumentRequest,
        use_glossary: bool,
    ) -> Result<Vec<u64>, ClientError> {
        self.call(Request::DocumentsAdd {
            paths,
            engine,
            request,
            use_glossary,
        })
        .await
    }

    pub async fn documents_cancel(&self, id: u64) -> Result<(), ClientError> {
        self.call(Request::DocumentsCancel { id }).await
    }

    pub async fn documents_remove(&self, id: u64) -> Result<(), ClientError> {
        self.call(Request::DocumentsRemove { id }).await
    }

    /// Receives the job list on every change in the daemon's queue.
    pub async fn subscribe(&self) -> Result<broadcast::Receiver<Vec<DocumentJob>>, ClientError> {
        let rx = self
            .events
            .lock()
            .unwrap()
            .as_ref()
            .ok_or(ClientError::Disconnected)?
            .subscribe();
        self.call::<()>(Request::Subscribe).await?;
        Ok(rx)
    }

    /// Queues one document in the daemon and waits until it is finished. Calls
    /// `progress` with the job on every change. Returns the output path.
    pub async fn translate_document(
        &self,
        path: &Path,
        request: DocumentRequest,
        use_glossary: bool,
        progress: impl Fn(&DocumentJob),
    ) -> Result<PathBuf, ClientError> {
        let mut rx = self.subscribe().await?;
        let ids = self
            .documents_add(
                vec![path.to_path_buf()],
                EngineId::Deepl,
                request,
                use_glossary,
            )
            .await?;
        let id = *ids
            .first()
            .ok_or_else(|| ClientError::Protocol("no job id".into()))?;
        // Snapshots sent before the add do not contain the job yet.
        let mut seen = false;
        loop {
            let jobs = match rx.recv().await {
                Ok(jobs) => jobs,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return Err(ClientError::Disconnected),
            };
            let Some(job) = jobs.into_iter().find(|j| j.id == id) else {
                if seen {
                    // Removed from the queue by another client.
                    return Err(EngineError::Cancelled.into());
                }
                continue;
            };
            seen = true;
            progress(&job);
            match job.status {
                JobStatus::Done => {
                    return job
                        .output
                        .ok_or_else(|| ClientError::Protocol("done without output".into()));
                }
                JobStatus::Error => {
                    return Err(job
                        .error
                        .unwrap_or(EngineError::Service("failed".into()))
                        .into());
                }
                JobStatus::Cancelled => return Err(EngineError::Cancelled.into()),
                _ => {}
            }
        }
    }
}
