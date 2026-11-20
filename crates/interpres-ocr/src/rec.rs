//! Text recognition: PP-OCRv5 SVTR model with CTC decoding.

use yaml_rust2::{Yaml, YamlLoader};

use crate::OcrError;

/// Input height of the recognition model.
pub const HEIGHT: usize = 48;
/// Widest line the model accepts.
const MAX_WIDTH: usize = 3200;

pub struct RecInput {
    /// CHW, BGR channel order, normalized to -1..1.
    pub data: Vec<f32>,
    pub width: usize,
}

/// Scale the line to height 48, keeping its aspect ratio.
pub fn preprocess(line: &image::RgbImage) -> RecInput {
    let ratio = line.width() as f32 / line.height().max(1) as f32;
    let width = ((HEIGHT as f32 * ratio).ceil() as usize).clamp(8, MAX_WIDTH);
    let resized = image::imageops::resize(
        line,
        width as u32,
        HEIGHT as u32,
        image::imageops::FilterType::Triangle,
    );
    let plane = width * HEIGHT;
    let mut data = vec![0f32; 3 * plane];
    for (i, px) in resized.pixels().enumerate() {
        for (c, value) in [px[2], px[1], px[0]].into_iter().enumerate() {
            data[c * plane + i] = (value as f32 / 255.0 - 0.5) / 0.5;
        }
    }
    RecInput { data, width }
}

/// Reads `PostProcess.character_dict` from the model's `inference.yml`.
pub fn parse_dictionary(yaml: &str) -> Result<Vec<String>, OcrError> {
    let docs =
        YamlLoader::load_from_str(yaml).map_err(|e| OcrError::InvalidModel(e.to_string()))?;
    let list = docs
        .first()
        .map(|d| &d["PostProcess"]["character_dict"])
        .and_then(Yaml::as_vec)
        .ok_or_else(|| OcrError::InvalidModel("no character_dict".into()))?;
    list.iter()
        .map(|item| match item {
            Yaml::String(s) => Ok(s.clone()),
            Yaml::Integer(i) => Ok(i.to_string()),
            Yaml::Real(r) => Ok(r.clone()),
            Yaml::Boolean(b) => Ok(b.to_string()),
            other => Err(OcrError::InvalidModel(format!(
                "bad dictionary entry: {other:?}"
            ))),
        })
        .collect()
}

/// Greedy CTC decoding of `probs` (`steps` × `classes`). Class 0 is the CTC blank,
/// 1..=dict.len() the dictionary, the last class a space. Returns the text and the
/// mean probability of its characters.
pub fn ctc_decode(probs: &[f32], steps: usize, classes: usize, dict: &[String]) -> (String, f32) {
    let mut text = String::new();
    let (mut sum, mut n) = (0f32, 0usize);
    let mut prev = 0usize;
    for t in 0..steps {
        let row = &probs[t * classes..(t + 1) * classes];
        let (best, p) = row
            .iter()
            .copied()
            .enumerate()
            .fold(
                (0, f32::MIN),
                |acc, (i, v)| if v > acc.1 { (i, v) } else { acc },
            );
        if best != 0 && best != prev {
            match dict.get(best - 1) {
                Some(ch) => text.push_str(ch),
                None => text.push(' '),
            }
            sum += p;
            n += 1;
        }
        prev = best;
    }
    (text, if n == 0 { 0.0 } else { sum / n as f32 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_and_plain_entries() {
        let yaml = "PostProcess:\n  name: CTCLabelDecode\n  character_dict:\n  - '0'\n  - A\n  - ''''\n  - ğ\n";
        assert_eq!(parse_dictionary(yaml).unwrap(), ["0", "A", "'", "ğ"]);
        assert!(parse_dictionary("Global: {}").is_err());
    }

    fn one_hot(seq: &[usize], classes: usize) -> Vec<f32> {
        let mut v = vec![0.01; seq.len() * classes];
        for (t, &c) in seq.iter().enumerate() {
            v[t * classes + c] = 0.9;
        }
        v
    }

    #[test]
    fn ctc_collapses_repeats_and_blanks() {
        let dict: Vec<String> = ["a", "b"].map(String::from).to_vec();
        let classes = dict.len() + 2; // blank + dict + space
        // a a _ a b space b b
        let probs = one_hot(&[1, 1, 0, 1, 2, 3, 2, 2], classes);
        let (text, score) = ctc_decode(&probs, 8, classes, &dict);
        assert_eq!(text, "aab b");
        assert!((score - 0.9).abs() < 1e-6);
        assert_eq!(
            ctc_decode(&one_hot(&[0, 0], classes), 2, classes, &dict).0,
            ""
        );
    }

    #[test]
    fn line_resized_to_model_height() {
        let input = preprocess(&image::RgbImage::new(200, 40));
        assert_eq!(input.width, 240);
        assert_eq!(input.data.len(), 3 * HEIGHT * 240);
    }
}
