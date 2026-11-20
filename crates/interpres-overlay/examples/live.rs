//! Live overlay on Wayland:
//! `live X Y W H TARGET SECONDS` translates a region into a caption below it;
//! `pick` opens the region picker and prints the region.

use std::sync::Arc;
use std::time::{Duration, Instant};

use interpres_core::{EngineId, Store, Translator, secrets};
use interpres_engine_deepl::DeeplEngine;
use interpres_ocr::{Ocr, RecModel, models};
use interpres_overlay::wayland::{WaylandCaption, pick_region};
use interpres_overlay::{Pipeline, Region, default_capture};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a[0] == "pick" {
        println!("{:?}", pick_region()?);
        return Ok(());
    }
    let n = |i: usize| a[i].parse::<i64>().unwrap();
    let region = Region {
        x: n(1) as i32,
        y: n(2) as i32,
        width: n(3) as u32,
        height: n(4) as u32,
    };
    let key = secrets::get_key(EngineId::Deepl)?.ok_or("no DeepL key")?;
    let translator = Translator::new(Arc::new(DeeplEngine::new(key)), None)
        .with_store(Arc::new(Store::open(&Store::default_path()?)?));
    let root = models::default_root().ok_or("no data dir")?;
    let mut pipeline = Pipeline::new(
        region,
        a[5].clone(),
        default_capture(),
        Box::new(Ocr::load(&root, RecModel::General)?),
        Arc::new(translator),
    );
    let caption = WaylandCaption::show(region)?;
    let end = Instant::now() + Duration::from_secs(n(6) as u64);
    while Instant::now() < end {
        if let Some(frame) = pipeline.step().await? {
            println!(
                "{} => {}",
                frame.source.replace('\n', " / "),
                frame.translation
            );
            caption.set_text(frame.translation);
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    Ok(())
}
