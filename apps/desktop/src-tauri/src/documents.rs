//! The document queue behind the Documents screen: the shared queue of a running
//! `interpres-daemon` when there is one, otherwise an in-process queue. If the daemon
//! goes away the app switches to the in-process queue.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use interpres_core::documents::DocumentJob;
use interpres_core::{DocumentQueue, DocumentRequest, EngineError, EngineId, Store};
use interpres_daemon::{ClientError, DaemonClient};
use tauri::{AppHandle, Emitter};

pub struct Documents {
    local: DocumentQueue,
    store: Arc<Store>,
    remote: Mutex<Option<Arc<DaemonClient>>>,
    app: AppHandle,
}

pub const EVENT: &str = "documents-changed";

impl Documents {
    pub fn new(local: DocumentQueue, store: Arc<Store>, app: AppHandle) -> Self {
        Self {
            local,
            store,
            remote: Mutex::new(None),
            app,
        }
    }

    /// Uses the daemon from now on and forwards its queue to the UI.
    pub async fn attach(self: &Arc<Self>, client: DaemonClient) {
        let client = Arc::new(client);
        let Ok(mut rx) = client.subscribe().await else {
            return;
        };
        *self.remote.lock().unwrap() = Some(client);
        tracing::info!("using interpres-daemon for documents");
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(jobs) => {
                        let _ = this.app.emit(EVENT, jobs);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            this.detach();
        });
    }

    /// Falls back to the in-process queue.
    fn detach(&self) {
        if self.remote.lock().unwrap().take().is_some() {
            tracing::warn!("interpres-daemon disconnected; using the local queue");
            let _ = self.app.emit(EVENT, self.local.jobs());
        }
    }

    fn remote(&self) -> Option<Arc<DaemonClient>> {
        self.remote.lock().unwrap().clone()
    }

    /// Runs `f` on the daemon. `None` means: no daemon (any more), use the local queue.
    async fn on_remote<T, F, Fut>(&self, f: F) -> Option<Result<T, EngineError>>
    where
        F: FnOnce(Arc<DaemonClient>) -> Fut,
        Fut: Future<Output = Result<T, ClientError>>,
    {
        let client = self.remote()?;
        match f(client).await {
            Ok(v) => Some(Ok(v)),
            Err(ClientError::Engine(e)) => Some(Err(e)),
            Err(ClientError::Disconnected) => {
                self.detach();
                None
            }
            Err(e) => Some(Err(EngineError::Service(e.to_string()))),
        }
    }

    pub async fn list(&self) -> Result<Vec<DocumentJob>, EngineError> {
        if let Some(r) = self
            .on_remote(|c| async move { c.documents_list().await })
            .await
        {
            return r;
        }
        Ok(self.local.jobs())
    }

    pub async fn add(
        &self,
        paths: Vec<PathBuf>,
        engine: EngineId,
        mut request: DocumentRequest,
        use_glossary: bool,
    ) -> Result<Vec<u64>, EngineError> {
        let (p, r) = (paths.clone(), request.clone());
        if let Some(res) = self
            .on_remote(|c| async move { c.documents_add(p, engine, r, use_glossary).await })
            .await
        {
            return res;
        }
        // Glossaries need an explicit source language.
        if use_glossary && let Some(src) = &request.source {
            request.glossary = self
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
            .map(|p| self.local.add(p, engine, request.clone(), label.clone()))
            .collect())
    }

    pub async fn cancel(&self, id: u64) -> Result<(), EngineError> {
        if let Some(r) = self
            .on_remote(|c| async move { c.documents_cancel(id).await })
            .await
        {
            return r;
        }
        self.local.cancel(id);
        Ok(())
    }

    pub async fn remove(&self, id: u64) -> Result<(), EngineError> {
        if let Some(r) = self
            .on_remote(|c| async move { c.documents_remove(id).await })
            .await
        {
            return r;
        }
        self.local.remove(id);
        Ok(())
    }

    /// The translated file of a finished job.
    pub async fn output(&self, id: u64) -> Result<PathBuf, EngineError> {
        self.list()
            .await?
            .into_iter()
            .find(|j| j.id == id)
            .and_then(|j| j.output)
            .ok_or_else(|| EngineError::InvalidRequest("document is not translated yet".into()))
    }
}
