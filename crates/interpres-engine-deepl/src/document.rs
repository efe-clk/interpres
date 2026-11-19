//! `/v2/document`: upload, poll status, download.

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use interpres_core::documents::{DocProgress, ProgressFn};
use interpres_core::{DocumentEngine, DocumentRequest, EngineError, EngineId};
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::{DeeplEngine, formality_param, parse};

/// File types sent to DeepL's document API.
const SUPPORTED: &[&str] = &[
    "docx", "pptx", "xlsx", "pdf", "txt", "srt", "md", "html", "htm",
];

#[derive(Deserialize)]
struct Uploaded {
    document_id: String,
    document_key: String,
}

#[derive(Serialize)]
struct KeyBody<'a> {
    document_key: &'a str,
}

#[derive(Deserialize)]
struct Status {
    status: String,
    seconds_remaining: Option<u64>,
    error_message: Option<String>,
}

fn io_error(e: std::io::Error) -> EngineError {
    EngineError::InvalidRequest(e.to_string())
}

impl DeeplEngine {
    async fn upload(&self, input: &Path, req: &DocumentRequest) -> Result<Uploaded, EngineError> {
        let bytes = tokio::fs::read(input).await.map_err(io_error)?;
        let name = input
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "document".into());
        let mut form = Form::new()
            .text("target_lang", req.target.clone())
            .part("file", Part::bytes(bytes).file_name(name));
        if let Some(src) = &req.source {
            form = form.text("source_lang", src.clone());
        }
        if let Some(f) = formality_param(req.formality) {
            form = form.text("formality", f);
        }
        let resp = self
            .send(self.http.post(self.url("/v2/document")).multipart(form))
            .await?;
        parse(resp).await
    }

    async fn status(&self, doc: &Uploaded) -> Result<Status, EngineError> {
        let rb = self
            .http
            .post(self.url(&format!("/v2/document/{}", doc.document_id)))
            .json(&KeyBody {
                document_key: &doc.document_key,
            });
        parse(self.send(rb).await?).await
    }

    async fn download(&self, doc: &Uploaded) -> Result<Vec<u8>, EngineError> {
        let rb = self
            .http
            .post(self.url(&format!("/v2/document/{}/result", doc.document_id)))
            .json(&KeyBody {
                document_key: &doc.document_key,
            });
        let resp = self.send(rb).await?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| EngineError::Network(e.to_string()))?;
        Ok(bytes.to_vec())
    }

    /// Waits between status checks: about half the remaining time DeepL reports,
    /// between 1x and 5x the poll interval.
    fn wait_for(&self, seconds_remaining: Option<u64>) -> Duration {
        let hint = seconds_remaining
            .map(|s| Duration::from_secs(s / 2))
            .unwrap_or(self.poll_interval);
        hint.clamp(self.poll_interval, self.poll_interval * 5)
    }
}

/// Runs `fut` unless `cancel` fires first.
async fn or_cancel<T>(
    cancel: &CancellationToken,
    fut: impl Future<Output = Result<T, EngineError>>,
) -> Result<T, EngineError> {
    tokio::select! {
        _ = cancel.cancelled() => Err(EngineError::Cancelled),
        r = fut => r,
    }
}

#[async_trait]
impl DocumentEngine for DeeplEngine {
    fn id(&self) -> EngineId {
        EngineId::Deepl
    }

    fn supports(&self, extension: &str) -> bool {
        SUPPORTED.contains(&extension)
    }

    async fn translate_document(
        &self,
        input: &Path,
        output: &Path,
        req: &DocumentRequest,
        progress: ProgressFn<'_>,
        cancel: &CancellationToken,
    ) -> Result<(), EngineError> {
        progress(DocProgress::Uploading);
        let doc = or_cancel(cancel, self.upload(input, req)).await?;

        loop {
            let st = or_cancel(cancel, self.status(&doc)).await?;
            match st.status.as_str() {
                "done" => break,
                "error" => {
                    return Err(EngineError::Service(
                        st.error_message
                            .unwrap_or_else(|| "document translation failed".into()),
                    ));
                }
                "queued" => progress(DocProgress::Queued),
                _ => progress(DocProgress::Translating {
                    seconds_remaining: st.seconds_remaining,
                }),
            }
            or_cancel(cancel, async {
                tokio::time::sleep(self.wait_for(st.seconds_remaining)).await;
                Ok(())
            })
            .await?;
        }

        progress(DocProgress::Downloading);
        let bytes = or_cancel(cancel, self.download(&doc)).await?;
        tokio::fs::write(output, bytes).await.map_err(io_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_is_clamped() {
        let e = DeeplEngine::with_base_url("k", "http://x");
        assert_eq!(e.wait_for(None), Duration::from_secs(1));
        assert_eq!(e.wait_for(Some(0)), Duration::from_secs(1));
        assert_eq!(e.wait_for(Some(6)), Duration::from_secs(3));
        assert_eq!(e.wait_for(Some(600)), Duration::from_secs(5));
    }
}
