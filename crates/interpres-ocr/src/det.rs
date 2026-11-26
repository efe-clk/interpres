//! Text detection: PP-OCRv5 DB model and a simplified DB post-process. Lines are
//! axis-aligned boxes, or rotated rectangles when the text is tilted.

/// Longest side of the detection input (`DetResizeForTest.resize_long`).
const RESIZE_LONG: f32 = 960.0;
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];
/// `DBPostProcess` defaults from the model config.
const THRESH: f32 = 0.3;
const BOX_THRESH: f32 = 0.6;
const UNCLIP_RATIO: f32 = 1.5;
/// Lines tilted by less than this (degrees) are read as horizontal.
const MIN_TILT: f32 = 8.0;
/// Steeper lines keep the axis-aligned path (and read as vertical lines when tall):
/// an upright column of CJK text that leans a little must not turn into a
/// sideways horizontal line.
const MAX_TILT: f32 = 60.0;
/// A region must be this much longer than wide for its direction to be trusted.
const MIN_ELONGATION: f32 = 1.5;
/// Extra room at both ends of a tilted line, in line heights: small marks at the
/// ends (a full stop) fall outside the detected kernel.
const END_PAD: f32 = 0.3;
/// Only regions at least this many times longer than tall get the end room: blobs
/// in busy backgrounds are found as short, thick "lines", and widening them pulls
/// in letters of the real line next to them.
const PAD_ELONGATION: f32 = 3.0;

/// One detected line, in original image coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextBox {
    /// Axis-aligned bounds: x, y, width, height.
    pub bbox: [u32; 4],
    /// Set for tilted lines: the rotated rectangle to cut out.
    pub tilt: Option<Tilt>,
}

/// A rotated rectangle. `angle` is the direction of the text in radians, clockwise
/// on screen (y points down); `size` is the length along and across the text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tilt {
    pub center: (f32, f32),
    pub size: (f32, f32),
    pub angle: f32,
}

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

/// Lines in original image coordinates, top to bottom, left to right.
pub fn boxes(
    prob: &[f32],
    width: usize,
    height: usize,
    scale: &(f32, f32),
    orig_w: u32,
    orig_h: u32,
) -> Vec<TextBox> {
    let mut seen = vec![false; width * height];
    let mut out = Vec::new();
    let mut tilted = Vec::new();
    let mut stack = Vec::new();
    let mut region = Vec::new();
    for start in 0..width * height {
        if seen[start] || prob[start] <= THRESH {
            continue;
        }
        // Flood-fill one connected region of the thresholded map.
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        let (mut sum, mut count) = (0f32, 0usize);
        seen[start] = true;
        stack.push(start);
        region.clear();
        while let Some(i) = stack.pop() {
            region.push(i);
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
        if let Some(t) = tilt(&region, width, scale) {
            tilted.push(t);
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
    let mut out: Vec<TextBox> = merge_vertical(out)
        .into_iter()
        .map(|bbox| TextBox { bbox, tilt: None })
        .chain(
            merge_tilted(tilted)
                .into_iter()
                .filter_map(|t| tilted_box(t, orig_w, orig_h)),
        )
        .collect();
    // Reading order: rows (with some tolerance), then left to right.
    out.sort_by_key(|b| {
        let (top, left) = reading_position(b);
        ((top / 8.0).floor() as i64, left as i64)
    });
    out
}

/// Top edge and left end of a line, measured in the line's own direction: for
/// a tilted line, along the normal and along the text. Lines of different length
/// would be misordered by their box tops or start points.
fn reading_position(b: &TextBox) -> (f32, f32) {
    match b.tilt {
        Some(t) => {
            let (c, s) = (t.angle.cos(), t.angle.sin());
            let normal = -t.center.0 * s + t.center.1 * c;
            let along = t.center.0 * c + t.center.1 * s;
            (normal - t.size.1 / 2.0, along - t.size.0 / 2.0)
        }
        None => (b.bbox[1] as f32, b.bbox[0] as f32),
    }
}

/// The rotated rectangle (in original image coordinates) of a region (pixel indices
/// into the detection map) whose main direction is tilted between `MIN_TILT` and
/// `MAX_TILT` degrees; `None` for level, steep or roundish regions, which take the
/// axis-aligned path.
fn tilt(region: &[usize], width: usize, scale: &(f32, f32)) -> Option<Tilt> {
    let n = region.len() as f32;
    let xy = |i: usize| ((i % width) as f32, (i / width) as f32);
    let (mx, my) = region.iter().fold((0.0, 0.0), |(sx, sy), &i| {
        let (x, y) = xy(i);
        (sx + x / n, sy + y / n)
    });
    let (cxx, cyy, cxy) = region.iter().fold((0.0, 0.0, 0.0), |(a, b, c), &i| {
        let (x, y) = xy(i);
        let (dx, dy) = (x - mx, y - my);
        (a + dx * dx, b + dy * dy, c + dx * dy)
    });
    // Direction of the main axis of the pixel cloud.
    let angle = 0.5 * (2.0 * cxy).atan2(cxx - cyy);
    if !(MIN_TILT..=MAX_TILT).contains(&angle.to_degrees().abs()) {
        return None;
    }
    let (c, s) = (angle.cos(), angle.sin());
    let (mut u0, mut u1, mut v0, mut v1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for &i in region {
        let (x, y) = xy(i);
        let (u, v) = ((x - mx) * c + (y - my) * s, -(x - mx) * s + (y - my) * c);
        (u0, u1, v0, v1) = (u0.min(u), u1.max(u), v0.min(v), v1.max(v));
    }
    let (len, thick) = (u1 - u0 + 1.0, v1 - v0 + 1.0);
    if len < thick * MIN_ELONGATION {
        return None;
    }
    // Unclip as for axis-aligned boxes, then map the corners to the original image
    // (the two axes are scaled slightly differently).
    let d = len * thick * UNCLIP_RATIO / (2.0 * (len + thick));
    let pad = if len >= thick * PAD_ELONGATION {
        d + thick * END_PAD
    } else {
        d
    };
    let (u0, u1, v0, v1) = (u0 - 0.5 - pad, u1 + 0.5 + pad, v0 - 0.5 - d, v1 + 0.5 + d);
    let corner = |u: f32, v: f32| {
        (
            (mx + 0.5 + u * c - v * s) / scale.0,
            (my + 0.5 + u * s + v * c) / scale.1,
        )
    };
    let p = [
        corner(u0, v0),
        corner(u1, v0),
        corner(u1, v1),
        corner(u0, v1),
    ];
    let dist = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).hypot(a.1 - b.1);
    Some(Tilt {
        center: (
            p.iter().map(|q| q.0).sum::<f32>() / 4.0,
            p.iter().map(|q| q.1).sum::<f32>() / 4.0,
        ),
        size: (dist(p[0], p[1]), dist(p[0], p[3])),
        angle: (p[1].1 - p[0].1).atan2(p[1].0 - p[0].0),
    })
}

/// Pieces of a tilted line split at word gaps become one line: same direction, on
/// the same center line, and end to end.
fn merge_tilted(mut lines: Vec<Tilt>) -> Vec<Tilt> {
    let join = |a: &Tilt, b: &Tilt| -> Option<Tilt> {
        if (a.angle - b.angle).abs() > 5f32.to_radians() {
            return None;
        }
        let (c, s) = (a.angle.cos(), a.angle.sin());
        let (dx, dy) = (b.center.0 - a.center.0, b.center.1 - a.center.1);
        let (along, across) = (dx * c + dy * s, -dx * s + dy * c);
        let thick = a.size.1.min(b.size.1);
        // Similar height: a background blob next to a line is not part of it.
        if a.size.1.max(b.size.1) > thick * 1.5 {
            return None;
        }
        let gap = along.abs() - (a.size.0 + b.size.0) / 2.0;
        if across.abs() > thick * 0.5 || gap > thick {
            return None;
        }
        let lo = (-a.size.0 / 2.0).min(along - b.size.0 / 2.0);
        let hi = (a.size.0 / 2.0).max(along + b.size.0 / 2.0);
        let (mid, off) = ((lo + hi) / 2.0, across / 2.0);
        Some(Tilt {
            center: (
                a.center.0 + mid * c - off * s,
                a.center.1 + mid * s + off * c,
            ),
            size: (hi - lo, a.size.1.max(b.size.1)),
            angle: a.angle,
        })
    };
    loop {
        let mut merged = false;
        'outer: for i in 0..lines.len() {
            for j in i + 1..lines.len() {
                if let Some(t) = join(&lines[i], &lines[j]) {
                    lines[i] = t;
                    lines.remove(j);
                    merged = true;
                    break 'outer;
                }
            }
        }
        if !merged {
            return lines;
        }
    }
}

/// A tilted line with its axis-aligned bounds (clipped to the image); `None` when
/// too small or outside the image.
fn tilted_box(t: Tilt, orig_w: u32, orig_h: u32) -> Option<TextBox> {
    let (c, s) = (t.angle.cos(), t.angle.sin());
    let (hw, hh) = (t.size.0 / 2.0, t.size.1 / 2.0);
    let p = [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)]
        .map(|(u, v)| (t.center.0 + u * c - v * s, t.center.1 + u * s + v * c));
    let x0 = p.iter().map(|q| q.0).fold(f32::MAX, f32::min).max(0.0);
    let y0 = p.iter().map(|q| q.1).fold(f32::MAX, f32::min).max(0.0);
    let x1 = p
        .iter()
        .map(|q| q.0)
        .fold(f32::MIN, f32::max)
        .min(orig_w as f32);
    let y1 = p
        .iter()
        .map(|q| q.1)
        .fold(f32::MIN, f32::max)
        .min(orig_h as f32);
    if t.size.0 < 4.0 || t.size.1 < 4.0 || x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(TextBox {
        bbox: [x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32],
        tilt: Some(t),
    })
}

/// A box that may be (part of) a vertical line: taller than wide, or a single
/// roughly square character.
fn verticalish(b: &[u32; 4]) -> bool {
    let ratio = b[3] as f32 / b[2].max(1) as f32;
    ratio >= 0.6
}

/// Joins boxes stacked on top of each other into one vertical line: detection
/// often splits vertical text at the gaps between characters. Wide boxes (lines
/// of horizontal text) are never joined, so two subtitle lines stay two lines.
fn merge_vertical(mut boxes: Vec<[u32; 4]>) -> Vec<[u32; 4]> {
    let stacked = |a: &[u32; 4], b: &[u32; 4]| {
        if !verticalish(a) || !verticalish(b) {
            return false;
        }
        let (ax1, bx1) = (a[0] + a[2], b[0] + b[2]);
        let overlap_x = ax1.min(bx1).saturating_sub(a[0].max(b[0]));
        let narrow = a[2].min(b[2]);
        let (top, bottom) = if a[1] <= b[1] { (a, b) } else { (b, a) };
        let gap = bottom[1] as i64 - (top[1] + top[3]) as i64;
        overlap_x * 2 >= narrow && gap < (narrow as f32 * 0.6) as i64
    };
    loop {
        let mut merged = false;
        'outer: for i in 0..boxes.len() {
            for j in i + 1..boxes.len() {
                if stacked(&boxes[i], &boxes[j]) {
                    let (a, b) = (boxes[i], boxes[j]);
                    let x0 = a[0].min(b[0]);
                    let y0 = a[1].min(b[1]);
                    let x1 = (a[0] + a[2]).max(b[0] + b[2]);
                    let y1 = (a[1] + a[3]).max(b[1] + b[3]);
                    boxes[i] = [x0, y0, x1 - x0, y1 - y0];
                    boxes.remove(j);
                    merged = true;
                    break 'outer;
                }
            }
        }
        if !merged {
            return boxes;
        }
    }
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
        assert!(
            boxes[0].bbox[1] < boxes[1].bbox[1],
            "top line first: {boxes:?}"
        );
        // The unclipped box contains the kernel.
        let b = boxes[1].bbox;
        assert!(
            b[0] <= 5 && b[0] + b[2] >= 60 && b[1] <= 25 && b[1] + b[3] >= 32,
            "{b:?}"
        );
    }

    #[test]
    fn stacked_characters_join_into_one_vertical_line() {
        // Three square characters one under the other, small gaps.
        let chars = vec![[10, 10, 40, 40], [12, 56, 38, 40], [10, 102, 40, 30]];
        assert_eq!(merge_vertical(chars), vec![[10, 10, 40, 122]]);
    }

    #[test]
    fn two_horizontal_lines_stay_apart() {
        let lines = vec![[100, 20, 470, 45], [80, 70, 600, 50]];
        assert_eq!(merge_vertical(lines.clone()), lines);
        // Side-by-side vertical lines (two columns) are not joined either.
        let columns = vec![[10, 10, 40, 300], [70, 10, 40, 300]];
        assert_eq!(merge_vertical(columns.clone()), columns);
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
        let b = boxes(&prob, w, h, &(0.5, 0.5), 128, 64)[0].bbox;
        assert!(b[0] < 20 && b[0] + b[2] > 100, "{b:?}");
    }

    /// A band along `angle_deg` (clockwise), 60 long and 6 thick, centered at (50, 50).
    fn band(angle_deg: f32) -> Vec<f32> {
        let (w, h) = (100, 100);
        let (c, s) = (angle_deg.to_radians().cos(), angle_deg.to_radians().sin());
        let mut prob = vec![0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = (x as f32 - 50.0, y as f32 - 50.0);
                let (u, v) = (dx * c + dy * s, -dx * s + dy * c);
                if u.abs() <= 30.0 && v.abs() <= 3.0 {
                    prob[y * w + x] = 0.9;
                }
            }
        }
        prob
    }

    #[test]
    fn tilted_band_gets_a_rotated_rectangle() {
        for deg in [-25.0f32, 20.0, 45.0] {
            let b = boxes(&band(deg), 100, 100, &(1.0, 1.0), 100, 100);
            assert_eq!(b.len(), 1, "{deg}: {b:?}");
            let t = b[0].tilt.unwrap_or_else(|| panic!("{deg}: not tilted"));
            assert!((t.angle.to_degrees() - deg).abs() < 2.0, "{deg}: {t:?}");
            assert!((t.center.0 - 50.5).abs() < 1.5 && (t.center.1 - 50.5).abs() < 1.5);
            // Longer than the kernel along the text (unclipped), and thin.
            assert!(t.size.0 > 61.0 && t.size.1 < 20.0, "{deg}: {t:?}");
        }
    }

    #[test]
    fn level_and_steep_bands_stay_axis_aligned() {
        for deg in [3.0f32, -5.0, 75.0, 90.0] {
            let b = boxes(&band(deg), 100, 100, &(1.0, 1.0), 100, 100);
            assert!(b.iter().all(|b| b.tilt.is_none()), "{deg}: {b:?}");
        }
    }

    #[test]
    fn pieces_of_a_tilted_line_join_but_parallel_lines_do_not() {
        let a = 20f32.to_radians();
        let at = |u: f32, v: f32, len: f32| Tilt {
            center: (
                100.0 + u * a.cos() - v * a.sin(),
                100.0 + u * a.sin() + v * a.cos(),
            ),
            size: (len, 30.0),
            angle: a,
        };
        // Two words with a small gap, and a second line 50 px below.
        let merged = merge_tilted(vec![
            at(0.0, 0.0, 100.0),
            at(110.0, 0.0, 100.0),
            at(0.0, 50.0, 100.0),
        ]);
        assert_eq!(merged.len(), 2, "{merged:?}");
        assert!((merged[0].size.0 - 210.0).abs() < 0.5, "{merged:?}");
        let mid = at(55.0, 0.0, 0.0).center;
        assert!(
            (merged[0].center.0 - mid.0).abs() < 0.5 && (merged[0].center.1 - mid.1).abs() < 0.5
        );
    }
}
