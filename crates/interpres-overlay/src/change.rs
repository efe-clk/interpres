//! Cheap "did the picture change?" check, so OCR only runs when something new is
//! on screen.

use image::RgbImage;

/// Side length of the thumbnail compared between frames.
const THUMB: u32 = 24;
/// Mean absolute difference (0–255) above which a frame counts as changed.
const THRESHOLD: f32 = 4.0;

/// A small grayscale fingerprint of a frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Fingerprint(Vec<u8>);

impl Fingerprint {
    pub fn of(img: &RgbImage) -> Self {
        let gray = image::DynamicImage::ImageRgb8(img.clone()).to_luma8();
        let small =
            image::imageops::resize(&gray, THUMB, THUMB, image::imageops::FilterType::Triangle);
        Self(small.into_raw())
    }

    pub fn differs_from(&self, other: &Fingerprint) -> bool {
        let diff: u32 = self
            .0
            .iter()
            .zip(&other.0)
            .map(|(a, b)| a.abs_diff(*b) as u32)
            .sum();
        diff as f32 / self.0.len() as f32 > THRESHOLD
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(v: u8) -> RgbImage {
        RgbImage::from_pixel(200, 50, image::Rgb([v, v, v]))
    }

    #[test]
    fn same_and_noisy_frames_are_unchanged() {
        let a = solid(40);
        let mut b = a.clone();
        // A few pixels of noise (e.g. a flickering particle) do not count.
        for x in 0..5 {
            b.put_pixel(x, 0, image::Rgb([255, 255, 255]));
        }
        assert!(!Fingerprint::of(&a).differs_from(&Fingerprint::of(&b)));
    }

    #[test]
    fn new_text_counts_as_change() {
        let a = solid(40);
        let mut b = a.clone();
        for y in 15..35 {
            for x in 20..180 {
                b.put_pixel(x, y, image::Rgb([255, 255, 255]));
            }
        }
        assert!(Fingerprint::of(&a).differs_from(&Fingerprint::of(&b)));
    }
}
