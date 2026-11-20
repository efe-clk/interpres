//! Text detection: PP-OCRv5 DB model and a simplified DB post-process that yields
//! axis-aligned boxes.

/// Longest side of the detection input (`DetResizeForTest.resize_long`).
const RESIZE_LONG: f32 = 960.0;
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];
/// `DBPostProcess` defaults from the model config.
const THRESH: f32 = 0.3;
const BOX_THRESH: f32 = 0.6;
const UNCLIP_RATIO: f32 = 1.5;

pub struct DetInput {
    /// CHW, BGR channel order (as PaddleOCR decodes images).
    pub data: Vec<f32>,
    pub width: usize,
    pub height: usize,
    /// Input size / original size, per axis.
    pub scale: (f32, f32),
}

/// Resize so the long side is 960 (sides rounded to multiples of 32), normalize.
pub fn preprocess(img: &image::RgbImage) -> DetInput {
    let (w, h) = (img.width() as f32, img.height() as f32);
    let ratio = RESIZE_LONG / w.max(h);
    let round32 = |v: f32| (((v / 32.0).round() as usize) * 32).max(32);
    let (rw, rh) = (round32(w * ratio), round32(h * ratio));
    let resized = image::imageops::resize(
        img,
        rw as u32,
        rh as u32,
        image::imageops::FilterType::Triangle,
    );
    let plane = rw * rh;
    let mut data = vec![0f32; 3 * plane];
    for (i, px) in resized.pixels().enumerate() {
        // BGR order: channel 0 is blue.
        for (c, value) in [px[2], px[1], px[0]].into_iter().enumerate() {
            data[c * plane + i] = (value as f32 / 255.0 - MEAN[c]) / STD[c];
        }
    }
    DetInput {
        data,
        width: rw,
        height: rh,
        scale: (rw as f32 / w, rh as f32 / h),
    }
}

/// Boxes (x, y, w, h) in original image coordinates, top to bottom, left to right.
pub fn boxes(
    prob: &[f32],
    width: usize,
    height: usize,
    scale: &(f32, f32),
    orig_w: u32,
    orig_h: u32,
) -> Vec<[u32; 4]> {
    let mut seen = vec![false; width * height];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..width * height {
        if seen[start] || prob[start] <= THRESH {
            continue;
        }
        // Flood-fill one connected region of the thresholded map.
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        let (mut sum, mut count) = (0f32, 0usize);
        seen[start] = true;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % width, i / width);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
            sum += prob[i];
            count += 1;
            let mut visit = |j: usize| {
                if !seen[j] && prob[j] > THRESH {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < width {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - width);
            }
            if y + 1 < height {
                visit(i + width);
            }
        }
        let (bw, bh) = ((x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32);
        if bw.min(bh) < 3.0 || sum / (count as f32) < BOX_THRESH {
            continue;
        }
        // DB "unclip": grow the shrunk text kernel back to the full text size.
        let d = bw * bh * UNCLIP_RATIO / (2.0 * (bw + bh));
        let fx0 = ((x0 as f32 - d) / scale.0).max(0.0);
        let fy0 = ((y0 as f32 - d) / scale.1).max(0.0);
        let fx1 = ((x1 as f32 + 1.0 + d) / scale.0).min(orig_w as f32);
        let fy1 = ((y1 as f32 + 1.0 + d) / scale.1).min(orig_h as f32);
        let (bx, by) = (fx0 as u32, fy0 as u32);
        let (bw, bh) = ((fx1 - fx0) as u32, (fy1 - fy0) as u32);
        if bw >= 4 && bh >= 4 {
            out.push([bx, by, bw, bh]);
        }
    }
    // Reading order: rows (with some tolerance), then left to right.
    out.sort_by_key(|b| (b[1] / 8, b[0]));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_keeps_ratio_in_multiples_of_32() {
        let img = image::RgbImage::new(1280, 150);
        let input = preprocess(&img);
        assert_eq!((input.width, input.height), (960, 128));
        assert_eq!(input.data.len(), 3 * 960 * 128);
        let small = preprocess(&image::RgbImage::new(10, 10));
        assert_eq!((small.width, small.height), (960, 960));
    }

    #[test]
    fn finds_separate_regions_in_reading_order() {
        let (w, h) = (100, 40);
        let mut prob = vec![0f32; w * h];
        // Two "lines": a wide one at the bottom left, one at the top right.
        for y in 25..32 {
            for x in 5..60 {
                prob[y * w + x] = 0.9;
            }
        }
        for y in 4..10 {
            for x in 70..95 {
                prob[y * w + x] = 0.8;
            }
        }
        // Weak noise is ignored.
        prob[2 * w + 2] = 0.4;
        let boxes = boxes(&prob, w, h, &(1.0, 1.0), 100, 40);
        assert_eq!(boxes.len(), 2);
        assert!(boxes[0][1] < boxes[1][1], "top line first: {boxes:?}");
        // The unclipped box contains the kernel.
        let b = boxes[1];
        assert!(
            b[0] <= 5 && b[0] + b[2] >= 60 && b[1] <= 25 && b[1] + b[3] >= 32,
            "{b:?}"
        );
    }

    #[test]
    fn maps_back_to_original_scale() {
        let (w, h) = (64, 32);
        let mut prob = vec![0f32; w * h];
        for y in 10..20 {
            for x in 10..50 {
                prob[y * w + x] = 1.0;
            }
        }
        let b = boxes(&prob, w, h, &(0.5, 0.5), 128, 64)[0];
        assert!(b[0] < 20 && b[0] + b[2] > 100, "{b:?}");
    }
}
