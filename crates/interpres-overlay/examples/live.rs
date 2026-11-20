//! Live overlay on Wayland:
//! `live X Y W H TARGET SECONDS [below|above] [#rrggbb] [OPACITY] [FONT_PX]`
//! translates a region into a caption next to it;
//! `pick` opens the region picker and prints the region.

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("this example needs Linux with a Wayland compositor");
}

#[cfg(target_os = "linux")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    linux::main().await
}

#[cfg(target_os = "linux")]
mod linux {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use interpres_core::{EngineId, Store, Translator, secrets};
    use interpres_engine_deepl::DeeplEngine;
    use interpres_ocr::{Ocr, RecModel, models};
    use interpres_overlay::caption::Style;
    use interpres_overlay::wayland::{WaylandCaption, pick_region};
    use interpres_overlay::{Pipeline, Placement, Region, default_capture};

    pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
        let placement = match a.get(7).map(String::as_str) {
            Some("above") => Placement::Above,
            _ => Placement::Below,
        };
        let style = Style {
            background: a
                .get(8)
                .map_or(Style::default().background, |c| Style::parse_color(c)),
            opacity: a.get(9).and_then(|o| o.parse().ok()).unwrap_or(85),
            font_size: a.get(10).and_then(|f| f.parse().ok()),
        };
        let caption = WaylandCaption::show(region, placement, style)?;
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
}
