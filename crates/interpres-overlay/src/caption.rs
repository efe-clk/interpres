//! Draws the translation as a caption: white text, centered, on a rounded dark
//! panel. Output is premultiplied BGRA (Wayland `Argb8888` little-endian).

use cosmic_text::{Align, Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};

/// How the caption looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    /// Text size in logical pixels; `None` fits the text to the box.
    pub font_size: Option<f32>,
    pub background: [u8; 3],
    /// Background opacity, 0–100.
    pub opacity: u8,
    /// Where the panel sits in the caption area: next to the captured region.
    pub valign: VAlign,
}

/// Vertical position of the panel inside the caption area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VAlign {
    Top,
    #[default]
    Center,
    Bottom,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            font_size: None,
            background: [16, 16, 20],
            opacity: 85,
            valign: VAlign::Center,
        }
    }
}

impl Style {
    /// Parses `#rrggbb`; falls back to the default dark background.
    pub fn parse_color(hex: &str) -> [u8; 3] {
        let h = hex.trim_start_matches('#');
        let byte = |i: usize| h.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok());
        match (h.len(), byte(0), byte(2), byte(4)) {
            (6, Some(r), Some(g), Some(b)) => [r, g, b],
            _ => Style::default().background,
        }
    }

    /// White text on dark or faint backgrounds, near-black on light ones.
    fn text_color(&self) -> [u8; 3] {
        let [r, g, b] = self.background.map(|c| c as f32 / 255.0);
        let luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        if self.opacity < 30 || luminance < 0.55 {
            [255, 255, 255]
        } else {
            [20, 20, 24]
        }
    }
}

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

    /// Renders `text` into a `width`×`height` buffer (physical pixels): a panel that
    /// hugs the text, aligned by `style.valign`. Empty text gives a fully transparent
    /// buffer.
    pub fn render(
        &mut self,
        text: &str,
        width: u32,
        height: u32,
        scale: f32,
        style: &Style,
    ) -> Vec<u8> {
        let Some(mut pixmap) = Pixmap::new(width.max(1), height.max(1)) else {
            return vec![0; (width * height * 4) as usize];
        };
        if !text.trim().is_empty() {
            let (w, h) = (width as f32, height as f32);
            let (pad_x, pad_y) = (14.0 * scale, 8.0 * scale);
            let (buffer, text_w, text_h) =
                self.layout(text, w - 2.0 * pad_x, h - 2.0 * pad_y, scale, style);
            // The panel wraps the text; the text box is centred horizontally.
            let panel_w = (text_w + 2.0 * pad_x).min(w);
            let panel_h = (text_h + 2.0 * pad_y).min(h);
            let x0 = (w - panel_w) / 2.0;
            let y0 = match style.valign {
                VAlign::Top => 0.0,
                VAlign::Bottom => h - panel_h,
                VAlign::Center => (h - panel_h) / 2.0,
            };
            if style.opacity > 0 {
                self.panel(&mut pixmap, x0, y0, panel_w, panel_h, scale, style);
            }
            // The buffer is laid out `w - 2*pad_x` wide with centred lines, so its
            // origin is at pad_x.
            self.glyphs_into(
                &mut pixmap,
                buffer,
                pad_x as i32,
                (y0 + pad_y) as i32,
                style,
            );
        }
        // tiny-skia stores premultiplied RGBA; Wayland wants BGRA.
        let mut out = pixmap.take();
        for px in out.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        out
    }

    /// Shapes `text` into lines `max_w` wide. A fixed size is used as is; otherwise
    /// the size starts from one that suits the box and shrinks until it fits
    /// `max_h`. Returns the buffer and the width and height the text really takes.
    fn layout(
        &mut self,
        text: &str,
        max_w: f32,
        max_h: f32,
        scale: f32,
        style: &Style,
    ) -> (Buffer, f32, f32) {
        let fixed = style.font_size.map(|s| s * scale);
        let mut size = fixed.unwrap_or_else(|| (max_h / 1.6).clamp(14.0 * scale, 34.0 * scale));
        let mut buffer = Buffer::new(&mut self.fonts, Metrics::new(size, size * 1.25));
        let attrs = Attrs::new().family(Family::SansSerif);
        let (mut text_w, mut text_h) = (0.0, 0.0);
        for _ in 0..10 {
            buffer.set_metrics(Metrics::new(size, size * 1.25));
            buffer.set_size(Some(max_w.max(1.0)), None);
            buffer.set_text(text, &attrs, Shaping::Advanced, Some(Align::Center));
            buffer.shape_until_scroll(&mut self.fonts, false);
            text_w = buffer.layout_runs().map(|r| r.line_w).fold(0.0, f32::max);
            text_h = buffer.layout_runs().map(|r| r.line_height).sum::<f32>();
            if fixed.is_some() || text_h <= max_h || size <= 10.0 * scale {
                break;
            }
            size *= 0.88;
        }
        (buffer, text_w, text_h)
    }

    #[allow(clippy::too_many_arguments)]
    fn panel(
        &self,
        pixmap: &mut Pixmap,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        scale: f32,
        style: &Style,
    ) {
        let r = (8.0 * scale).min(h / 2.0).min(w / 2.0);
        let mut pb = PathBuilder::new();
        pb.move_to(x + r, y);
        pb.line_to(x + w - r, y);
        pb.quad_to(x + w, y, x + w, y + r);
        pb.line_to(x + w, y + h - r);
        pb.quad_to(x + w, y + h, x + w - r, y + h);
        pb.line_to(x + r, y + h);
        pb.quad_to(x, y + h, x, y + h - r);
        pb.line_to(x, y + r);
        pb.quad_to(x, y, x + r, y);
        pb.close();
        if let Some(path) = pb.finish() {
            let mut paint = Paint::default();
            let [r, g, b] = style.background;
            let alpha = (style.opacity.min(100) as u32 * 255 / 100) as u8;
            paint.set_color_rgba8(r, g, b, alpha);
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

    fn glyphs_into(
        &mut self,
        pixmap: &mut Pixmap,
        mut buffer: Buffer,
        left: i32,
        top: i32,
        style: &Style,
    ) {
        let (pw, ph) = (pixmap.width() as i32, pixmap.height() as i32);
        let data = pixmap.data_mut();
        let fg = style.text_color();
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
                        // Source-over blend of the text colour (premultiplied).
                        let i = ((py * pw + px) * 4) as usize;
                        for c in 0..3 {
                            let dst = data[i + c] as u32;
                            data[i + c] =
                                (fg[c] as u32 * alpha / 255 + dst * (255 - alpha) / 255) as u8;
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
        let buf = p.render("  ", 200, 60, 1.0, &Style::default());
        assert_eq!(buf.len(), 200 * 60 * 4);
        assert!(buf.iter().all(|&b| b == 0));
    }

    #[test]
    fn text_draws_a_panel_and_light_glyph_pixels() {
        let mut p = Painter::new();
        let buf = p.render("The door is locked.", 400, 80, 1.0, &Style::default());
        // The middle of the panel is opaque-ish…
        let centre = ((40 * 400 + 200) * 4) as usize;
        assert!(buf[centre + 3] > 150, "panel alpha");
        // …and some pixels are bright (the glyphs).
        let bright = buf
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|px| px[0] > 200 && px[1] > 200 && px[2] > 200)
            .count();
        assert!(bright > 50, "glyph pixels: {bright}");
    }

    #[test]
    fn light_background_gets_dark_text_and_zero_opacity_no_panel() {
        let mut p = Painter::new();
        let light = Style {
            font_size: Some(24.0),
            background: [250, 250, 250],
            opacity: 100,
            valign: VAlign::Center,
        };
        let buf = p.render("Hello", 300, 60, 1.0, &light);
        let dark_glyphs = buf
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|px| px[0] < 60 && px[3] > 200)
            .count();
        assert!(dark_glyphs > 20, "dark glyph pixels: {dark_glyphs}");

        let bare = Style {
            opacity: 0,
            ..Style::default()
        };
        let buf = p.render("Hello", 300, 60, 1.0, &bare);
        // Corner (no glyph there) stays fully transparent.
        assert_eq!(&buf[0..4], &[0, 0, 0, 0]);
    }

    #[test]
    fn panel_hugs_short_text_and_follows_valign() {
        let mut p = Painter::new();
        let top = Style {
            valign: VAlign::Top,
            ..Style::default()
        };
        let buf = p.render("Hi", 600, 160, 1.0, &top);
        let alpha = |x: u32, y: u32| buf[((y * 600 + x) * 4 + 3) as usize];
        // Far left/right and the bottom stay empty: the panel only wraps "Hi"…
        assert_eq!(alpha(20, 20), 0);
        assert_eq!(alpha(580, 20), 0);
        assert_eq!(alpha(300, 150), 0);
        // …and sits at the top, centred.
        assert!(alpha(300, 6) > 150);
    }

    #[test]
    fn parses_hex_colours() {
        assert_eq!(Style::parse_color("#ff8000"), [255, 128, 0]);
        assert_eq!(Style::parse_color("nope"), Style::default().background);
    }
}
