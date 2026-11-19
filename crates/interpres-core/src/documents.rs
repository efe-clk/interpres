//! Document translation: the engine interface and a UI-independent job queue.
//!
//! Front-ends add files to a [`DocumentQueue`] and watch snapshots of the job list;
//! the queue translates one document at a time with whichever [`DocumentEngine`]
//! the job asks for.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, watch};
use tokio_util::sync::CancellationToken;

use crate::{EngineError, EngineId, Formality};

/// Options for translating one document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentRequest {
    pub source: Option<String>,
    pub target: String,
    #[serde(default)]
    pub formality: Formality,
    /// Terms to enforce. Engines use it only when `source` is set.
    #[serde(default)]
    pub glossary: Option<crate::glossary::Glossary>,
}

/// Progress reported by an engine while it works on a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocProgress {
    Uploading,
    Queued,
    Translating { seconds_remaining: Option<u64> },
    Downloading,
}

pub type ProgressFn<'a> = &'a (dyn Fn(DocProgress) + Send + Sync);

/// An engine that translates whole files, keeping their format.
#[async_trait]
pub trait DocumentEngine: Send + Sync {
    fn id(&self) -> EngineId;

    /// Whether the engine handles files with this (lowercase) extension.
    fn supports(&self, extension: &str) -> bool;

    /// Translates `input` and writes the result to `output`.
    /// Must return [`EngineError::Cancelled`] soon after `cancel` fires.
    async fn translate_document(
        &self,
        input: &Path,
        output: &Path,
        req: &DocumentRequest,
        progress: ProgressFn<'_>,
        cancel: &CancellationToken,
    ) -> Result<(), EngineError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Uploading,
    Translating,
    Downloading,
    Done,
    Error,
    Cancelled,
}

impl JobStatus {
    fn is_finished(self) -> bool {
        matches!(
            self,
            JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
        )
    }
}

/// One document in the queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentJob {
    pub id: u64,
    pub name: String,
    pub input: PathBuf,
    /// Set once the translated file has been written.
    pub output: Option<PathBuf>,
    pub engine: EngineId,
    pub request: DocumentRequest,
    pub glossary: Option<String>,
    pub status: JobStatus,
    /// 0–100.
    pub progress: u8,
    pub error: Option<EngineError>,
}

/// Returns the engine for a job, or an error such as [`EngineError::MissingKey`].
/// Called when the job starts, so keys added after queueing are picked up.
pub type EngineResolver =
    Box<dyn Fn(EngineId) -> Result<Arc<dyn DocumentEngine>, EngineError> + Send + Sync>;

struct Inner {
    jobs: Mutex<Vec<DocumentJob>>,
    tokens: Mutex<HashMap<u64, CancellationToken>>,
    tx: watch::Sender<Vec<DocumentJob>>,
    wake: Notify,
    next_id: AtomicU64,
    resolve: EngineResolver,
}

/// A queue that translates documents one at a time.
#[derive(Clone)]
pub struct DocumentQueue {
    inner: Arc<Inner>,
}

impl DocumentQueue {
    /// Creates the queue and the worker future that processes it. The caller spawns
    /// the worker on its runtime.
    pub fn new(
        resolve: EngineResolver,
    ) -> (Self, impl std::future::Future<Output = ()> + Send + 'static) {
        let (tx, _) = watch::channel(Vec::new());
        let queue = Self {
            inner: Arc::new(Inner {
                jobs: Mutex::new(Vec::new()),
                tokens: Mutex::new(HashMap::new()),
                tx,
                wake: Notify::new(),
                next_id: AtomicU64::new(1),
                resolve,
            }),
        };
        let worker = queue.clone().run();
        (queue, worker)
    }

    /// Current jobs, oldest first.
    pub fn jobs(&self) -> Vec<DocumentJob> {
        self.inner.jobs.lock().unwrap().clone()
    }

    /// Receives a snapshot of the job list on every change.
    pub fn subscribe(&self) -> watch::Receiver<Vec<DocumentJob>> {
        self.inner.tx.subscribe()
    }

    pub fn add(
        &self,
        input: PathBuf,
        engine: EngineId,
        request: DocumentRequest,
        glossary: Option<String>,
    ) -> u64 {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let name = input
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.update(|jobs| {
            jobs.push(DocumentJob {
                id,
                name,
                input,
                output: None,
                engine,
                request,
                glossary,
                status: JobStatus::Queued,
                progress: 0,
                error: None,
            })
        });
        self.inner.wake.notify_one();
        id
    }

    /// Stops a queued or running job.
    pub fn cancel(&self, id: u64) {
        if let Some(t) = self.inner.tokens.lock().unwrap().get(&id) {
            t.cancel();
        }
        self.update_job(id, |j| {
            if !j.status.is_finished() {
                j.status = JobStatus::Cancelled;
            }
        });
    }

    /// Cancels the job if needed and removes it from the list.
    pub fn remove(&self, id: u64) {
        self.cancel(id);
        self.update(|jobs| jobs.retain(|j| j.id != id));
    }

    pub fn job(&self, id: u64) -> Option<DocumentJob> {
        self.inner
            .jobs
            .lock()
            .unwrap()
            .iter()
            .find(|j| j.id == id)
            .cloned()
    }

    fn update(&self, f: impl FnOnce(&mut Vec<DocumentJob>)) {
        let snapshot = {
            let mut jobs = self.inner.jobs.lock().unwrap();
            f(&mut jobs);
            jobs.clone()
        };
        self.inner.tx.send_replace(snapshot);
    }

    fn update_job(&self, id: u64, f: impl FnOnce(&mut DocumentJob)) {
        self.update(|jobs| {
            if let Some(j) = jobs.iter_mut().find(|j| j.id == id) {
                f(j)
            }
        });
    }

    fn next_queued(&self) -> Option<DocumentJob> {
        self.inner
            .jobs
            .lock()
            .unwrap()
            .iter()
            .find(|j| j.status == JobStatus::Queued)
            .cloned()
    }

    async fn run(self) {
        loop {
            match self.next_queued() {
                Some(job) => self.process(job).await,
                None => self.inner.wake.notified().await,
            }
        }
    }

    async fn process(&self, job: DocumentJob) {
        let id = job.id;
        let token = CancellationToken::new();
        self.inner.tokens.lock().unwrap().insert(id, token.clone());
        // The job may have been cancelled or removed since it was picked.
        let mut started = false;
        self.update_job(id, |j| {
            if j.status == JobStatus::Queued {
                j.status = JobStatus::Uploading;
                j.progress = 2;
                started = true;
            }
        });
        if !started {
            self.inner.tokens.lock().unwrap().remove(&id);
            return;
        }

        let output = unique_output_path(&job.input, &job.request.target);
        // Largest `seconds_remaining` seen, to turn the countdown into a percentage.
        let max_remaining = Mutex::new(0u64);
        let progress = |p: DocProgress| {
            let (status, pct) = match p {
                DocProgress::Uploading => (JobStatus::Uploading, 5),
                DocProgress::Queued => (JobStatus::Translating, 10),
                DocProgress::Translating { seconds_remaining } => {
                    let pct = match seconds_remaining {
                        Some(rem) => {
                            let mut max = max_remaining.lock().unwrap();
                            *max = (*max).max(rem);
                            (80 * (*max - rem))
                                .checked_div(*max)
                                .map_or(90, |done| 10 + done as u8)
                        }
                        None => 50,
                    };
                    (JobStatus::Translating, pct)
                }
                DocProgress::Downloading => (JobStatus::Downloading, 95),
            };
            self.update_job(id, |j| {
                if !j.status.is_finished() {
                    j.status = status;
                    j.progress = j.progress.max(pct);
                }
            });
        };

        let result = match (self.inner.resolve)(job.engine) {
            Ok(engine) => {
                engine
                    .translate_document(&job.input, &output, &job.request, &progress, &token)
                    .await
            }
            Err(e) => Err(e),
        };

        self.inner.tokens.lock().unwrap().remove(&id);
        self.update_job(id, |j| match result {
            Ok(()) => {
                j.status = JobStatus::Done;
                j.progress = 100;
                j.output = Some(output);
            }
            Err(EngineError::Cancelled) => j.status = JobStatus::Cancelled,
            Err(e) => {
                tracing::warn!(job = id, error = %e, "document translation failed");
                j.status = JobStatus::Error;
                j.error = Some(e);
            }
        });
    }
}

/// `dir/report.docx` + `EN-US` → `dir/report.EN-US.docx`, adding ` (2)`, ` (3)`…
/// if that file already exists.
pub fn unique_output_path(input: &Path, target: &str) -> PathBuf {
    let dir = input.parent().unwrap_or(Path::new(""));
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = input
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let mut n = 1;
    loop {
        let suffix = if n == 1 {
            String::new()
        } else {
            format!(" ({n})")
        };
        let candidate = dir.join(format!("{stem}.{target}{suffix}{ext}"));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Writes "translated" after reporting progress; `fail` makes it return an error,
    /// `block` makes it wait until cancelled.
    struct Fake {
        fail: Option<EngineError>,
        block: bool,
    }

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
            progress(DocProgress::Translating {
                seconds_remaining: Some(10),
            });
            if self.block {
                cancel.cancelled().await;
                return Err(EngineError::Cancelled);
            }
            if let Some(e) = &self.fail {
                return Err(e.clone());
            }
            progress(DocProgress::Downloading);
            tokio::fs::write(output, "translated").await.unwrap();
            Ok(())
        }
    }

    fn queue_with(fake: Fake) -> DocumentQueue {
        let engine: Arc<dyn DocumentEngine> = Arc::new(fake);
        let (q, worker) = DocumentQueue::new(Box::new(move |_| Ok(engine.clone())));
        tokio::spawn(worker);
        q
    }

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("interpres-docs-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn req() -> DocumentRequest {
        DocumentRequest {
            source: None,
            target: "EN-US".into(),
            formality: Formality::Default,
            glossary: None,
        }
    }

    async fn wait_until(
        q: &DocumentQueue,
        id: u64,
        f: impl Fn(&DocumentJob) -> bool,
    ) -> DocumentJob {
        let mut rx = q.subscribe();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(j) = q.job(id).filter(|j| f(j)) {
                    return j;
                }
                rx.changed().await.unwrap();
            }
        })
        .await
        .expect("timed out")
    }

    #[test]
    fn output_path_adds_target_and_counter() {
        let dir = temp_dir("naming");
        let input = dir.join("rapor.docx");
        assert_eq!(
            unique_output_path(&input, "EN-US"),
            dir.join("rapor.EN-US.docx")
        );
        std::fs::write(dir.join("rapor.EN-US.docx"), "").unwrap();
        assert_eq!(
            unique_output_path(&input, "EN-US"),
            dir.join("rapor.EN-US (2).docx")
        );
    }

    #[tokio::test]
    async fn translates_and_writes_output() {
        let dir = temp_dir("ok");
        let q = queue_with(Fake {
            fail: None,
            block: false,
        });
        let id = q.add(dir.join("a.txt"), EngineId::Deepl, req(), None);
        let job = wait_until(&q, id, |j| j.status.is_finished()).await;
        assert_eq!(job.status, JobStatus::Done);
        assert_eq!(job.progress, 100);
        let out = job.output.unwrap();
        assert_eq!(out, dir.join("a.EN-US.txt"));
        assert_eq!(std::fs::read_to_string(out).unwrap(), "translated");
    }

    #[tokio::test]
    async fn records_engine_error() {
        let dir = temp_dir("err");
        let q = queue_with(Fake {
            fail: Some(EngineError::QuotaExceeded),
            block: false,
        });
        let id = q.add(dir.join("a.txt"), EngineId::Deepl, req(), None);
        let job = wait_until(&q, id, |j| j.status.is_finished()).await;
        assert_eq!(job.status, JobStatus::Error);
        assert_eq!(job.error, Some(EngineError::QuotaExceeded));
        assert_eq!(job.output, None);
    }

    #[tokio::test]
    async fn missing_engine_is_an_error() {
        let (q, worker) = DocumentQueue::new(Box::new(|_| Err(EngineError::MissingKey)));
        tokio::spawn(worker);
        let id = q.add(
            temp_dir("nokey").join("a.txt"),
            EngineId::Deepl,
            req(),
            None,
        );
        let job = wait_until(&q, id, |j| j.status.is_finished()).await;
        assert_eq!(job.error, Some(EngineError::MissingKey));
    }

    #[tokio::test]
    async fn cancels_running_job_and_continues() {
        let dir = temp_dir("cancel");
        let engine: Arc<dyn DocumentEngine> = Arc::new(Fake {
            fail: None,
            block: true,
        });
        let ok: Arc<dyn DocumentEngine> = Arc::new(Fake {
            fail: None,
            block: false,
        });
        let calls = Arc::new(AtomicU64::new(0));
        let c = calls.clone();
        let (q, worker) = DocumentQueue::new(Box::new(move |_| {
            // First job blocks until cancelled, later ones succeed.
            Ok(if c.fetch_add(1, Ordering::SeqCst) == 0 {
                engine.clone()
            } else {
                ok.clone()
            })
        }));
        tokio::spawn(worker);

        let first = q.add(dir.join("a.txt"), EngineId::Deepl, req(), None);
        let second = q.add(dir.join("b.txt"), EngineId::Deepl, req(), None);
        wait_until(&q, first, |j| j.status == JobStatus::Translating).await;
        assert_eq!(q.job(second).unwrap().status, JobStatus::Queued);

        q.cancel(first);
        assert_eq!(
            wait_until(&q, first, |j| j.status.is_finished())
                .await
                .status,
            JobStatus::Cancelled
        );
        assert_eq!(
            wait_until(&q, second, |j| j.status.is_finished())
                .await
                .status,
            JobStatus::Done
        );
    }

    #[tokio::test]
    async fn cancelled_queued_job_is_skipped() {
        let dir = temp_dir("skip");
        let (q, worker) = DocumentQueue::new(Box::new(|_| Err(EngineError::MissingKey)));
        // Worker not started yet: the job stays queued.
        let id = q.add(dir.join("a.txt"), EngineId::Deepl, req(), None);
        q.cancel(id);
        tokio::spawn(worker);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(q.job(id).unwrap().status, JobStatus::Cancelled);
    }

    #[tokio::test]
    async fn remove_drops_job() {
        let q = queue_with(Fake {
            fail: None,
            block: true,
        });
        let id = q.add(temp_dir("rm").join("a.txt"), EngineId::Deepl, req(), None);
        q.remove(id);
        assert!(q.job(id).is_none());
        assert!(q.jobs().is_empty());
    }
}
