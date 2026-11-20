//! Downloads the models if needed and reads the test images:
//! `cargo run --release -p interpres-ocr --example ocr_demo`

use std::time::Instant;

use interpres_ocr::{Ocr, RecModel, models};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = models::default_root().ok_or("no data dir")?;
    for model in [RecModel::General, RecModel::Latin] {
        if !models::is_installed(&root, model) {
            println!(
                "downloading {model:?} ({} MB)...",
                models::download_size(model) / 1_000_000
            );
            models::download(&root, model, "https://huggingface.co", |_, _| {}).await?;
        }
    }
    for (model, files) in [
        (RecModel::General, ["en", "ja"].as_slice()),
        (RecModel::Latin, ["tr"].as_slice()),
    ] {
        let mut ocr = Ocr::load(&root, model)?;
        for name in files {
            let path = format!("{}/tests/fixtures/{name}.png", env!("CARGO_MANIFEST_DIR"));
            let img = image::open(&path)?.to_rgb8();
            ocr.recognize(&img)?; // warm-up
            let start = Instant::now();
            let lines = ocr.recognize(&img)?;
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            println!("{name} ({model:?}, {ms:.0} ms):");
            for l in lines {
                println!("  {:.2}  {:?}  {}", l.score, l.bbox, l.text);
            }
        }
    }
    Ok(())
}
