//! Draws the translation as a caption: white text, centered, on a rounded dark
//! panel. Output is premultiplied BGRA (Wayland `Argb8888` little-endian).

use cosmic_text::{Align, Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};

/// Font and glyph caches; creating them scans the system fonts, so keep one.
pub struct Painter {
    fonts: FontSystem,
    glyphs: SwashCache,
}

impl Default for Painter {
    fn default() -> Self {
        Self::new()
    }
}

impl Painter {
    pub fn new() -> Self {
        Self {
            fonts: FontSystem::new(),
            glyphs: SwashCache::new(),
        }
    }

    /// Renders `text` into a `width`×`height` buffer (physical pixels). Empty text
    /// gives a fully transparent buffer.
    pub fn render(&mut self, text: &str, width: u32, height: u32, scale: f32) -> Vec<u8> {
        let Some(mut pixmap) = Pixmap::new(width.max(1), height.max(1)) else {
            return vec![0; (width * height * 4) as usize];
        };
        if !text.trim().is_empty() {
            self.panel(&mut pixmap, scale);
            self.text(&mut pixmap, text, scale);
        }
        // tiny-skia stores premultiplied RGBA; Wayland wants BGRA.
        let mut out = pixmap.take();
        for px in out.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        out
    }

    fn panel(&self, pixmap: &mut Pixmap, scale: f32) {
        let (w, h) = (pixmap.width() as f32, pixmap.height() as f32);
        let r = (10.0 * scale).min(h / 2.0);
        let mut pb = PathBuilder::new();
        pb.move_to(r, 0.0);
        pb.line_to(w - r, 0.0);
        pb.quad_to(w, 0.0, w, r);
        pb.line_to(w, h - r);
        pb.quad_to(w, h, w - r, h);
        pb.line_to(r, h);
        pb.quad_to(0.0, h, 0.0, h - r);
        pb.line_to(0.0, r);
        pb.quad_to(0.0, 0.0, r, 0.0);
        pb.close();
        if let Some(path) = pb.finish() {
            let mut paint = Paint::default();
            paint.set_color_rgba8(16, 16, 20, 215);
            paint.anti_alias = true;
            pixmap.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    fn text(&mut self, pixmap: &mut Pixmap, text: &str, scale: f32) {
        let (w, h) = (pixmap.width() as f32, pixmap.height() as f32);
        let pad = 12.0 * scale;
        // Start from a size that suits the box, shrink until the text fits.
        let mut size = (h / 3.0).clamp(14.0 * scale, 34.0 * scale);
        let mut buffer = Buffer::new(&mut self.fonts, Metrics::new(size, size * 1.25));
        let attrs = Attrs::new().family(Family::SansSerif);
        let mut text_h = 0.0;
        for _ in 0..8 {
            buffer.set_metrics(Metrics::new(size, size * 1.25));
            buffer.set_size(Some(w - 2.0 * pad), None);
            buffer.set_text(text, &attrs, Shaping::Advanced, Some(Align::Center));
            buffer.shape_until_scroll(&mut self.fonts, false);
            text_h = buffer.layout_runs().map(|r| r.line_height).sum::<f32>();
            if text_h <= h - 2.0 * pad * 0.5 || size <= 10.0 * scale {
                break;
            }
            size *= 0.88;
        }
        let top = ((h - text_h) / 2.0).max(0.0) as i32;
        let left = pad as i32;
        let data = pixmap.data_mut();
        let (pw, ph) = (w as i32, h as i32);
        buffer.draw(
            &mut self.fonts,
            &mut self.glyphs,
            Color::rgb(255, 255, 255),
            |x, y, gw, gh, color| {
                let alpha = color.a() as u32;
                if alpha == 0 {
                    return;
                }
                for dy in 0..gh as i32 {
                    for dx in 0..gw as i32 {
                        let (px, py) = (left + x + dx, top + y + dy);
                        if px < 0 || py < 0 || px >= pw || py >= ph {
                            continue;
                        }
                        // Source-over blend of white (premultiplied) onto the panel.
                        let i = ((py * pw + px) * 4) as usize;
                        for c in 0..3 {
                            let dst = data[i + c] as u32;
                            data[i + c] = (alpha * 255 / 255 + dst * (255 - alpha) / 255) as u8;
                        }
                        let dst_a = data[i + 3] as u32;
                        data[i + 3] = (alpha + dst_a * (255 - alpha) / 255) as u8;
                    }
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_is_transparent() {
        let mut p = Painter::new();
        let buf = p.render("  ", 200, 60, 1.0);
        assert_eq!(buf.len(), 200 * 60 * 4);
        assert!(buf.iter().all(|&b| b == 0));
    }

    #[test]
    fn text_draws_a_panel_and_light_glyph_pixels() {
        let mut p = Painter::new();
        let buf = p.render("The door is locked.", 400, 80, 1.0);
        // The centre of the panel is opaque-ish dark…
        let centre = ((40 * 400 + 5) * 4) as usize;
        assert!(buf[centre + 3] > 150, "panel alpha");
        // …and some pixels are bright (the glyphs).
        let bright = buf
            .chunks_exact(4)
            .filter(|px| px[0] > 200 && px[1] > 200 && px[2] > 200)
            .count();
        assert!(bright > 50, "glyph pixels: {bright}");
    }
}
