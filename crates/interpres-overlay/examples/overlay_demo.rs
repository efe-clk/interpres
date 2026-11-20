//! Runs the real pipeline on a screen region:
//! `cargo run --release -p interpres-overlay --example overlay_demo -- X Y W H TARGET`

use std::sync::Arc;
use std::time::Instant;

use interpres_core::{EngineId, Store, Translator, secrets};
use interpres_engine_deepl::DeeplEngine;
use interpres_ocr::{Ocr, RecModel, models};
use interpres_overlay::{Pipeline, Region, default_capture};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let region = Region {
        x: a[0].parse()?,
        y: a[1].parse()?,
        width: a[2].parse()?,
        height: a[3].parse()?,
    };
    let key = secrets::get_key(EngineId::Deepl)?.ok_or("no DeepL key")?;
    let translator = Translator::new(Arc::new(DeeplEngine::new(key)), None)
        .with_store(Arc::new(Store::open(&Store::default_path()?)?));
    let root = models::default_root().ok_or("no data dir")?;
    let mut p = Pipeline::new(
        region,
        a[4].clone(),
        default_capture(),
        Box::new(Ocr::load(&root, RecModel::General)?),
        Arc::new(translator),
    );
    for i in 0..3 {
        let t = Instant::now();
        let frame = p.step().await?;
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        match frame {
            Some(f) => println!("step {i} ({ms:.0} ms):\n  {}\n  => {}", f.source.replace('\n', " / "), f.translation),
            None => println!("step {i} ({ms:.0} ms): unchanged"),
        }
    }
    Ok(())
}
