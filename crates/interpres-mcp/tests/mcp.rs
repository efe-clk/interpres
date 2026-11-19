use std::sync::Arc;
use std::time::Duration;

use interpres_core::{EngineError, Store};
use interpres_engine_deepl::DeeplEngine;
use interpres_mcp::InterpresMcp;
use rmcp::model::{CallToolRequestParams, CallToolResult, ClientConfig};
use rmcp::service::RunningService;
use rmcp::{ClientHandler, RoleClient, ServiceExt};
use serde_json::{Value, json};
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Clone, Default)]
struct Client;

impl ClientHandler for Client {
    fn get_info(&self) -> ClientConfig {
        ClientConfig::default()
    }
}

/// Starts the server against `deepl_url` (or with no key) and connects a client.
async fn connect(
    deepl_url: Option<String>,
    store: Arc<Store>,
) -> RunningService<RoleClient, Client> {
    let engine = Arc::new(move || match &deepl_url {
        Some(url) => Ok(DeeplEngine::with_base_url("k:fx", url.clone())
            .with_poll_interval(Duration::from_millis(1))),
        None => Err(EngineError::MissingKey),
    });
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);
    tokio::spawn(async move {
        let server = InterpresMcp::new(store, engine).serve(server_io).await?;
        server.waiting().await?;
        anyhow::Ok(())
    });
    Client.serve(client_io).await.expect("client connects")
}

async fn call(
    client: &RunningService<RoleClient, Client>,
    name: &'static str,
    args: Value,
) -> CallToolResult {
    client
        .call_tool(
            CallToolRequestParams::new(name).with_arguments(args.as_object().unwrap().clone()),
        )
        .await
        .expect("tool call")
}

fn text(r: &CallToolResult) -> String {
    r.content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_error(r: &CallToolResult) -> bool {
    r.is_error == Some(true)
}

#[tokio::test]
async fn lists_tools() {
    let client = connect(None, Arc::new(Store::open_in_memory().unwrap())).await;
    let tools = client.list_all_tools().await.unwrap();
    let mut names: Vec<_> = tools.iter().map(|t| t.name.to_string()).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "glossary_add",
            "glossary_list",
            "glossary_remove",
            "glossary_terms",
            "history_search",
            "translate_document",
            "translate_text",
            "usage"
        ]
    );
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn translates_text_and_records_history() {
    let deepl = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/translate"))
        .and(body_json(
            json!({"text": ["Merhaba"], "target_lang": "EN-US", "formality": "less"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "translations": [{"text": "Hi", "detected_source_language": "TR"}]
        })))
        .expect(1)
        .mount(&deepl)
        .await;

    let store = Arc::new(Store::open_in_memory().unwrap());
    let client = connect(Some(deepl.uri()), store.clone()).await;
    let args = json!({"text": "Merhaba", "target_lang": "en-us", "formality": "less"});
    let r = call(&client, "translate_text", args.clone()).await;
    assert!(!is_error(&r));
    assert_eq!(text(&r), "Hi");
    // Second call comes from the cache (the mock expects exactly one request).
    assert_eq!(text(&call(&client, "translate_text", args).await), "Hi");

    let h = call(&client, "history_search", json!({"query": "merhaba"})).await;
    assert!(text(&h).contains("Merhaba => Hi"), "{}", text(&h));
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn missing_key_is_a_tool_error() {
    let client = connect(None, Arc::new(Store::open_in_memory().unwrap())).await;
    let r = call(
        &client,
        "translate_text",
        json!({"text": "a", "target_lang": "DE"}),
    )
    .await;
    assert!(is_error(&r));
    assert!(text(&r).contains("no API key"), "{}", text(&r));

    let r = call(
        &client,
        "translate_text",
        json!({"text": "a", "target_lang": "DE", "formality": "weird"}),
    )
    .await;
    assert!(is_error(&r));
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn glossary_add_list_remove() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    let client = connect(None, store.clone()).await;
    let add = call(
        &client,
        "glossary_add",
        json!({"source_lang": "tr", "target_lang": "EN-US", "source": "sunucu", "target": "server"}),
    )
    .await;
    assert!(!is_error(&add));
    assert_eq!(
        text(&call(&client, "glossary_list", json!({})).await),
        "TR -> EN: 1 terms"
    );
    assert_eq!(
        text(
            &call(
                &client,
                "glossary_terms",
                json!({"source_lang": "TR", "target_lang": "EN"})
            )
            .await
        ),
        "sunucu -> server"
    );
    let rm = call(
        &client,
        "glossary_remove",
        json!({"source_lang": "TR", "target_lang": "EN", "source": "sunucu"}),
    )
    .await;
    assert!(!is_error(&rm));
    assert!(store.glossary_pairs().unwrap().is_empty());
    assert!(is_error(
        &call(
            &client,
            "glossary_remove",
            json!({"source_lang": "TR", "target_lang": "EN", "source": "yok"})
        )
        .await
    ));
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn translates_document() {
    let deepl = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/document"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"document_id": "D", "document_key": "K"})),
        )
        .mount(&deepl)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/document/D"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"document_id": "D", "status": "done"})),
        )
        .mount(&deepl)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/document/D/result"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Hello"))
        .mount(&deepl)
        .await;

    let dir = std::env::temp_dir().join(format!("interpres-mcp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("not.txt");
    std::fs::write(&input, "Merhaba").unwrap();

    let client = connect(
        Some(deepl.uri()),
        Arc::new(Store::open_in_memory().unwrap()),
    )
    .await;
    let r = call(
        &client,
        "translate_document",
        json!({"path": input, "target_lang": "en-us"}),
    )
    .await;
    assert!(!is_error(&r), "{}", text(&r));
    let out = dir.join("not.EN-US.txt");
    assert_eq!(text(&r), out.display().to_string());
    assert_eq!(std::fs::read_to_string(out).unwrap(), "Hello");

    let missing = call(
        &client,
        "translate_document",
        json!({"path": dir.join("yok.txt"), "target_lang": "DE"}),
    )
    .await;
    assert!(is_error(&missing));
    let exe = dir.join("a.exe");
    std::fs::write(&exe, "x").unwrap();
    assert!(is_error(
        &call(
            &client,
            "translate_document",
            json!({"path": exe, "target_lang": "DE"})
        )
        .await
    ));
    client.cancel().await.unwrap();
}
