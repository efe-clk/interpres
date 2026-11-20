//! Writes a caption to PNG to check how it looks:
//! `cargo run -p interpres-overlay --example caption_preview -- out.png "text"`

use interpres_overlay::caption::Painter;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (w, h, scale) = (1280u32, 170u32, 2.0f32);
    let mut bgra = Painter::new().render(&a[1], w, h, scale);
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    // Composite over grey so the panel edges are visible.
    let img = image::RgbaImage::from_raw(w, h, bgra).unwrap();
    let mut bg = image::RgbaImage::from_pixel(w, h, image::Rgba([90, 110, 140, 255]));
    image::imageops::overlay(&mut bg, &img, 0, 0);
    bg.save(&a[0]).unwrap();
}
