use interpres_core::{Engine, Formality, Glossary, GlossaryTerm, TranslateRequest};
use interpres_engine_deepl::DeeplEngine;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn glossary() -> Glossary {
    Glossary::new(
        "TR",
        "EN-US",
        vec![
            GlossaryTerm {
                source: "sunucu".into(),
                target: "server".into(),
            },
            GlossaryTerm {
                source: "a\tb".into(),
                target: "c\nd".into(),
            },
        ],
    )
}

fn req(source: Option<&str>) -> TranslateRequest {
    TranslateRequest {
        text: "Sunucu çalışıyor".into(),
        source: source.map(Into::into),
        target: "EN-US".into(),
        formality: Formality::Default,
        glossary: Some(glossary()),
    }
}

fn name() -> String {
    format!("interpres TR-EN {}", &glossary().hash[..16])
}

async fn mock_translate(server: &MockServer, body: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path("/v2/translate"))
        .and(body_json(body))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "translations": [{"text": "The server is running"}]
        })))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn creates_glossary_and_deletes_outdated_one() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/glossaries"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"glossaries": [
                {"glossary_id": "OLD", "name": "interpres TR-EN 0000000000000000"},
                {"glossary_id": "OTHER", "name": "interpres TR-DE 0000000000000000"},
                {"glossary_id": "USER", "name": "my own glossary"}
            ]})),
        )
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v2/glossaries/OLD"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/glossaries"))
        .and(body_json(json!({
            "name": name(),
            "source_lang": "tr",
            "target_lang": "en",
            "entries": "a b\tc d\nsunucu\tserver",
            "entries_format": "tsv"
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "glossary_id": "NEW", "name": name()
        })))
        .expect(1)
        .mount(&server)
        .await;
    mock_translate(
        &server,
        json!({"text": ["Sunucu çalışıyor"], "target_lang": "EN-US", "source_lang": "TR", "glossary_id": "NEW"}),
    )
    .await;

    let e = DeeplEngine::with_base_url("k:fx", server.uri());
    assert_eq!(
        e.translate(&req(Some("TR"))).await.unwrap().text,
        "The server is running"
    );
}

#[tokio::test]
async fn reuses_matching_glossary() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/glossaries"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"glossaries": [
                {"glossary_id": "SAME", "name": name()}
            ]})),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/glossaries"))
        .respond_with(ResponseTemplate::new(201))
        .expect(0)
        .mount(&server)
        .await;
    mock_translate(
        &server,
        json!({"text": ["Sunucu çalışıyor"], "target_lang": "EN-US", "source_lang": "TR", "glossary_id": "SAME"}),
    )
    .await;

    let e = DeeplEngine::with_base_url("k:fx", server.uri());
    e.translate(&req(Some("TR"))).await.unwrap();
}

#[tokio::test]
async fn no_glossary_without_source_language() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/glossaries"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    mock_translate(
        &server,
        json!({"text": ["Sunucu çalışıyor"], "target_lang": "EN-US"}),
    )
    .await;

    let e = DeeplEngine::with_base_url("k:fx", server.uri());
    e.translate(&req(None)).await.unwrap();
}
