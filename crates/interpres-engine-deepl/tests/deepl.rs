use interpres_core::{Engine, EngineError, Formality, LanguageKind, TranslateRequest};
use interpres_engine_deepl::{DeeplEngine, FREE_URL, PRO_URL, base_url_for_key};
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "test-key:fx";

fn req(formality: Formality) -> TranslateRequest {
    TranslateRequest {
        text: "Merhaba dünya".into(),
        source: None,
        target: "EN-US".into(),
        formality,
    }
}

async fn engine_with(status: u16, body: serde_json::Value) -> (MockServer, DeeplEngine) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/translate"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    let engine = DeeplEngine::with_base_url(KEY, server.uri());
    (server, engine)
}

#[test]
fn free_and_pro_endpoints() {
    assert_eq!(base_url_for_key("abc:fx"), FREE_URL);
    assert_eq!(base_url_for_key("abc"), PRO_URL);
    assert_eq!(DeeplEngine::new("abc:fx").base_url(), FREE_URL);
    assert_eq!(DeeplEngine::new("abc").base_url(), PRO_URL);
}

#[tokio::test]
async fn translates_text() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/translate"))
        .and(header("Authorization", "DeepL-Auth-Key test-key:fx"))
        .and(body_json(json!({
            "text": ["Merhaba dünya"],
            "target_lang": "EN-US",
            "formality": "more"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "translations": [{"detected_source_language": "TR", "text": "Hello world"}]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let engine = DeeplEngine::with_base_url(KEY, server.uri());
    let t = engine.translate(&req(Formality::More)).await.unwrap();
    assert_eq!(t.text, "Hello world");
    assert_eq!(t.detected_source.as_deref(), Some("TR"));
}

#[tokio::test]
async fn sends_source_and_omits_default_formality() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/translate"))
        .and(body_json(json!({
            "text": ["Merhaba dünya"],
            "target_lang": "EN-US",
            "source_lang": "TR"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "translations": [{"text": "Hello world"}]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let engine = DeeplEngine::with_base_url(KEY, server.uri());
    let mut r = req(Formality::Default);
    r.source = Some("TR".into());
    engine.translate(&r).await.unwrap();
}

#[tokio::test]
async fn forbidden_is_invalid_key() {
    let (_s, e) = engine_with(403, json!({"message": "Forbidden"})).await;
    assert_eq!(
        e.translate(&req(Formality::Default)).await,
        Err(EngineError::InvalidKey)
    );
}

#[tokio::test]
async fn status_456_is_quota_exceeded() {
    let (_s, e) = engine_with(456, json!({"message": "Quota exceeded"})).await;
    assert_eq!(
        e.translate(&req(Formality::Default)).await,
        Err(EngineError::QuotaExceeded)
    );
}

#[tokio::test]
async fn status_429_is_rate_limited() {
    let (_s, e) = engine_with(429, json!({})).await;
    assert_eq!(
        e.translate(&req(Formality::Default)).await,
        Err(EngineError::RateLimited)
    );
}

#[tokio::test]
async fn bad_request_carries_message() {
    let (_s, e) = engine_with(
        400,
        json!({"message": "Value for 'target_lang' not supported."}),
    )
    .await;
    assert_eq!(
        e.translate(&req(Formality::Default)).await,
        Err(EngineError::InvalidRequest(
            "Value for 'target_lang' not supported.".into()
        ))
    );
}

#[tokio::test]
async fn server_error_is_service_error() {
    let (_s, e) = engine_with(503, json!({})).await;
    assert!(matches!(
        e.translate(&req(Formality::Default)).await,
        Err(EngineError::Service(_))
    ));
}

#[tokio::test]
async fn unreachable_server_is_network_error() {
    // Bind a free port, then release it so nothing listens there.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let e = DeeplEngine::with_base_url(KEY, format!("http://127.0.0.1:{port}"));
    assert!(matches!(
        e.translate(&req(Formality::Default)).await,
        Err(EngineError::Network(_))
    ));
}

#[tokio::test]
async fn lists_languages() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/languages"))
        .and(query_param("type", "target"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"language": "DE", "name": "German", "supports_formality": true},
            {"language": "EN-US", "name": "English (American)", "supports_formality": false}
        ])))
        .mount(&server)
        .await;
    let e = DeeplEngine::with_base_url(KEY, server.uri());
    let langs = e.languages(LanguageKind::Target).await.unwrap();
    assert_eq!(langs.len(), 2);
    assert_eq!(langs[0].code, "DE");
    assert!(langs[0].supports_formality);
}

#[tokio::test]
async fn reads_usage() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "character_count": 1200, "character_limit": 500000
        })))
        .mount(&server)
        .await;
    let e = DeeplEngine::with_base_url(KEY, server.uri());
    let u = e.usage().await.unwrap();
    assert_eq!((u.character_count, u.character_limit), (1200, 500000));
}
