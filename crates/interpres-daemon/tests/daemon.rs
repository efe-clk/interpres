use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use interpres_core::documents::{DocProgress, JobStatus, ProgressFn};
use interpres_core::{
    DocumentEngine, DocumentQueue, DocumentRequest, EngineError, EngineId, Formality, Store,
};
use interpres_daemon::server::{Daemon, serve};
use interpres_daemon::{ClientError, DaemonClient, Endpoint};
use tokio_util::sync::CancellationToken;

/// Writes "translated" after a short delay.
struct Fake;

#[async_trait]
impl DocumentEngine for Fake {
    fn id(&self) -> EngineId {
        EngineId::Deepl
    }
    fn supports(&self, _: &str) -> bool {
        true
    }
    async fn translate_document(
        &self,
        _input: &Path,
        output: &Path,
        _req: &DocumentRequest,
        progress: ProgressFn<'_>,
        cancel: &CancellationToken,
    ) -> Result<(), EngineError> {
        progress(DocProgress::Uploading);
        tokio::select! {
            _ = cancel.cancelled() => return Err(EngineError::Cancelled),
            _ = tokio::time::sleep(Duration::from_millis(50)) => {}
        }
        tokio::fs::write(output, "translated").await.unwrap();
        Ok(())
    }
}

fn endpoint(name: &str) -> Endpoint {
    let id = format!("interpres-test-{}-{name}", std::process::id());
    if cfg!(windows) {
        Endpoint(PathBuf::from(format!(r"\\.\pipe\{id}")))
    } else {
        Endpoint(std::env::temp_dir().join(format!("{id}.sock")))
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("interpres-daemon-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Starts a daemon with the fake engine. Cancel the token to stop it.
async fn start(ep: &Endpoint) -> (CancellationToken, Arc<Store>) {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let (queue, worker) = DocumentQueue::new(Box::new(|_| Ok(Arc::new(Fake))));
    tokio::spawn(worker);
    let shutdown = CancellationToken::new();
    let daemon = Arc::new(Daemon {
        queue,
        store: store.clone(),
    });
    let (ep2, sd) = (ep.clone(), shutdown.clone());
    tokio::spawn(async move { serve(&ep2, daemon, sd).await.unwrap() });
    // Wait until it accepts connections.
    for _ in 0..100 {
        if DaemonClient::connect(ep).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    (shutdown, store)
}

fn req() -> DocumentRequest {
    DocumentRequest {
        source: Some("TR".into()),
        target: "EN-US".into(),
        formality: Formality::Default,
        glossary: None,
    }
}

#[tokio::test]
async fn two_clients_share_the_queue() {
    let ep = endpoint("share");
    let (shutdown, store) = start(&ep).await;
    store
        .upsert_term(None, "TR", "EN", "sunucu", "server")
        .unwrap();
    let dir = temp_dir("share");
    let input = dir.join("a.txt");
    std::fs::write(&input, "Merhaba").unwrap();

    let watcher = DaemonClient::connect(&ep).await.unwrap();
    watcher.ping().await.unwrap();
    let mut events = watcher.subscribe().await.unwrap();

    let cli = DaemonClient::connect(&ep).await.unwrap();
    let out = cli
        .translate_document(&input, req(), true, |_| {})
        .await
        .unwrap();
    assert_eq!(out, dir.join("a.EN-US.txt"));
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "translated");

    // The other client saw the same job finish, with the glossary applied.
    let job = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let jobs = events.recv().await.unwrap();
            if let Some(j) = jobs.into_iter().find(|j| j.status == JobStatus::Done) {
                return j;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(job.glossary.as_deref(), Some("TR → EN"));
    assert_eq!(watcher.documents_list().await.unwrap().len(), 1);

    let id = job.id;
    watcher.documents_remove(id).await.unwrap();
    assert!(cli.documents_list().await.unwrap().is_empty());
    shutdown.cancel();
}

#[tokio::test]
async fn cancel_through_the_daemon() {
    let ep = endpoint("cancel");
    let (shutdown, _) = start(&ep).await;
    let dir = temp_dir("cancel");
    let input = dir.join("a.txt");
    std::fs::write(&input, "x").unwrap();

    let a = Arc::new(DaemonClient::connect(&ep).await.unwrap());
    let b = DaemonClient::connect(&ep).await.unwrap();
    let a2 = a.clone();
    let wait =
        tokio::spawn(async move { a2.translate_document(&input, req(), false, |_| {}).await });
    // Cancel as soon as the job shows up.
    let mut events = b.subscribe().await.unwrap();
    let id = loop {
        if let Some(j) = events.recv().await.unwrap().first() {
            break j.id;
        }
    };
    b.documents_cancel(id).await.unwrap();
    assert_eq!(
        wait.await.unwrap(),
        Err(ClientError::Engine(EngineError::Cancelled))
    );
    shutdown.cancel();
}

#[tokio::test]
async fn shutdown_disconnects_clients() {
    let ep = endpoint("down");
    let (shutdown, _) = start(&ep).await;
    let client = DaemonClient::connect(&ep).await.unwrap();
    let mut events = client.subscribe().await.unwrap();
    shutdown.cancel();

    // Subscriptions end and later calls fail instead of hanging.
    let ended = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if events.recv().await.is_err() {
                break;
            }
        }
    })
    .await;
    assert!(ended.is_ok(), "subscription did not end");
    assert_eq!(client.ping().await, Err(ClientError::Disconnected));
}

#[tokio::test]
async fn no_daemon_means_connect_fails() {
    assert!(DaemonClient::connect(&endpoint("none")).await.is_err());
}
