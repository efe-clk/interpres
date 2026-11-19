use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use interpres_core::documents::DocProgress;
use interpres_core::{DocumentEngine, DocumentRequest, EngineError, Formality};
use interpres_engine_deepl::DeeplEngine;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{body_json, body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "test-key:fx";

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("interpres-deepl-doc-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn req() -> DocumentRequest {
    DocumentRequest {
        source: Some("TR".into()),
        target: "DE".into(),
        formality: Formality::More,
    }
}

fn engine(server: &MockServer) -> DeeplEngine {
    DeeplEngine::with_base_url(KEY, server.uri()).with_poll_interval(Duration::from_millis(1))
}

async fn mock_upload(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/v2/document"))
        .and(header("Authorization", "DeepL-Auth-Key test-key:fx"))
        .and(body_string_contains("name=\"target_lang\"\r\n\r\nDE"))
        .and(body_string_contains("name=\"source_lang\"\r\n\r\nTR"))
        .and(body_string_contains("name=\"formality\"\r\n\r\nmore"))
        .and(body_string_contains("filename=\"rapor.txt\""))
        .and(body_string_contains("Merhaba"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "document_id": "DOC1", "document_key": "KEY1"
        })))
        .expect(1)
        .mount(server)
        .await;
}

/// Mounts status responses that are returned in order, the last one repeatedly.
async fn mock_statuses(server: &MockServer, bodies: &[serde_json::Value]) {
    for (i, body) in bodies.iter().enumerate() {
        let mut m = Mock::given(method("POST"))
            .and(path("/v2/document/DOC1"))
            .and(body_json(json!({"document_key": "KEY1"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(body.clone()))
            .with_priority(i as u8 + 1);
        if i + 1 < bodies.len() {
            m = m.up_to_n_times(1);
        }
        m.mount(server).await;
    }
}

fn input(dir: &PathBuf) -> PathBuf {
    let p = dir.join("rapor.txt");
    std::fs::write(&p, "Merhaba dünya").unwrap();
    p
}

#[tokio::test]
async fn uploads_polls_and_downloads() {
    let server = MockServer::start().await;
    mock_upload(&server).await;
    mock_statuses(
        &server,
        &[
            json!({"document_id": "DOC1", "status": "queued"}),
            json!({"document_id": "DOC1", "status": "translating", "seconds_remaining": 4}),
            json!({"document_id": "DOC1", "status": "done", "billed_characters": 13}),
        ],
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/v2/document/DOC1/result"))
        .and(body_json(json!({"document_key": "KEY1"})))
        .respond_with(ResponseTemplate::new(200).set_body_string("Hallo Welt"))
        .expect(1)
        .mount(&server)
        .await;

    let dir = temp_dir("ok");
    let out = dir.join("rapor.DE.txt");
    let seen = Mutex::new(Vec::new());
    engine(&server)
        .translate_document(
            &input(&dir),
            &out,
            &req(),
            &|p| seen.lock().unwrap().push(p),
            &CancellationToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(std::fs::read_to_string(&out).unwrap(), "Hallo Welt");
    assert_eq!(
        seen.into_inner().unwrap(),
        vec![
            DocProgress::Uploading,
            DocProgress::Queued,
            DocProgress::Translating {
                seconds_remaining: Some(4)
            },
            DocProgress::Downloading,
        ]
    );
}

#[tokio::test]
async fn error_status_carries_message() {
    let server = MockServer::start().await;
    mock_upload(&server).await;
    mock_statuses(
        &server,
        &[json!({"document_id": "DOC1", "status": "error", "error_message": "Source and target language are equal."})],
    )
    .await;

    let dir = temp_dir("error");
    let out = dir.join("out.txt");
    let r = engine(&server)
        .translate_document(
            &input(&dir),
            &out,
            &req(),
            &|_| {},
            &CancellationToken::new(),
        )
        .await;
    assert_eq!(
        r,
        Err(EngineError::Service(
            "Source and target language are equal.".into()
        ))
    );
    assert!(!out.exists());
}

#[tokio::test]
async fn upload_quota_exceeded() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/document"))
        .respond_with(
            ResponseTemplate::new(456).set_body_json(json!({"message": "Quota exceeded"})),
        )
        .mount(&server)
        .await;

    let dir = temp_dir("quota");
    let r = engine(&server)
        .translate_document(
            &input(&dir),
            &dir.join("o.txt"),
            &req(),
            &|_| {},
            &CancellationToken::new(),
        )
        .await;
    assert_eq!(r, Err(EngineError::QuotaExceeded));
}

#[tokio::test]
async fn cancel_stops_polling() {
    let server = MockServer::start().await;
    mock_upload(&server).await;
    mock_statuses(
        &server,
        &[json!({"document_id": "DOC1", "status": "translating", "seconds_remaining": 100})],
    )
    .await;

    let dir = temp_dir("cancel");
    let out = dir.join("o.txt");
    let cancel = CancellationToken::new();
    let c = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        c.cancel();
    });
    let r = engine(&server)
        .translate_document(&input(&dir), &out, &req(), &|_| {}, &cancel)
        .await;
    assert_eq!(r, Err(EngineError::Cancelled));
    assert!(!out.exists());
}

#[tokio::test]
async fn missing_input_is_invalid_request() {
    let server = MockServer::start().await;
    let dir = temp_dir("missing");
    let r = engine(&server)
        .translate_document(
            &dir.join("yok.docx"),
            &dir.join("o.docx"),
            &req(),
            &|_| {},
            &CancellationToken::new(),
        )
        .await;
    assert!(matches!(r, Err(EngineError::InvalidRequest(_))));
}

#[test]
fn supported_extensions() {
    let e = DeeplEngine::new("k:fx");
    for ext in ["docx", "pptx", "xlsx", "pdf", "txt", "srt", "md"] {
        assert!(e.supports(ext), "{ext}");
    }
    assert!(!e.supports("exe"));
}
