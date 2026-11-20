//! Reads the fixture images with the real models. Skipped when the models are not
//! installed (they are ~33 MB; run `cargo run -p interpres-ocr --example ocr_demo`).

use interpres_ocr::{Ocr, RecModel, models};

fn read(model: RecModel, name: &str) -> Option<Vec<String>> {
    let root = std::env::var_os("INTERPRES_OCR_MODELS")
        .map(Into::into)
        .or_else(models::default_root)?;
    if !models::is_installed(&root, model) {
        eprintln!("skipping: {model:?} OCR model not installed");
        return None;
    }
    let mut ocr = Ocr::load(&root, model).unwrap();
    let path = format!("{}/tests/fixtures/{name}.png", env!("CARGO_MANIFEST_DIR"));
    let img = image::open(path).unwrap().to_rgb8();
    Some(
        ocr.recognize(&img)
            .unwrap()
            .into_iter()
            .map(|l| l.text)
            .collect(),
    )
}

#[test]
fn reads_english_japanese_and_turkish_subtitles() {
    if let Some(lines) = read(RecModel::General, "en") {
        assert_eq!(lines, ["The door is locked.", "You need the silver key."]);
    }
    if let Some(lines) = read(RecModel::General, "ja") {
        assert_eq!(lines, ["扉は鍵がかかっている。", "銀の鍵が必要だ。"]);
    }
    if let Some(lines) = read(RecModel::Latin, "tr") {
        assert_eq!(
            lines,
            ["Kapı kilitli. Gümüş anahtarı", "bulmanız gerekiyor."]
        );
    }
}

#[test]
fn reads_vertical_japanese() {
    if let Some(lines) = read(RecModel::General, "ja_vertical") {
        assert_eq!(lines, ["扉は鍵がかかっている"]);
    }
}

#[test]
fn missing_models_are_reported() {
    let err = Ocr::load(std::path::Path::new("/nonexistent"), RecModel::General).err();
    assert!(matches!(err, Some(interpres_ocr::OcrError::NotInstalled)));
}
