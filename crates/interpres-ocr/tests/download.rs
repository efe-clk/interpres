use interpres_ocr::{OcrError, RecModel, models};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn rejects_files_that_fail_verification() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not a model"))
        .mount(&server)
        .await;
    let root = std::env::temp_dir().join(format!("interpres-ocr-dl-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);

    let r = models::download(&root, RecModel::Latin, &server.uri(), |_, _| {}).await;
    assert!(matches!(r, Err(OcrError::Download(_))), "{r:?}");
    assert!(!models::is_installed(&root, RecModel::Latin));
    // Nothing half-written is left behind.
    let det_dir = models::det_model(&root).parent().unwrap().to_path_buf();
    let leftovers: Vec<_> = std::fs::read_dir(det_dir).unwrap().collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[tokio::test]
async fn http_errors_are_download_errors() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let root = std::env::temp_dir().join(format!("interpres-ocr-404-{}", std::process::id()));
    let r = models::download(&root, RecModel::General, &server.uri(), |_, _| {}).await;
    assert!(matches!(r, Err(OcrError::Download(_))));
}
