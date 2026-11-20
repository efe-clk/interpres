//! Text recognition for the game overlay: PaddleOCR PP-OCRv5 mobile models run with
//! ONNX Runtime, which is linked into the binary (users install nothing).
//!
//! Pipeline: detect text regions ([`det`]), cut each one out, read it ([`rec`]).
//! Boxes are axis-aligned: game subtitles and UI text are horizontal, so rotated
//! text is not handled.

pub mod det;
pub mod models;
pub mod rec;

use std::path::Path;
use std::sync::OnceLock;

use image::RgbImage;
use ort::environment::Environment;
use ort::session::Session;
use ort::value::Tensor;
use serde::{Deserialize, Serialize};

pub use models::RecModel;

#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    #[error("OCR model is not installed")]
    NotInstalled,
    #[error("OCR model download failed: {0}")]
    Download(String),
    #[error("OCR model is invalid: {0}")]
    InvalidModel(String),
    #[error("OCR inference failed: {0}")]
    Inference(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl From<ort::Error> for OcrError {
    fn from(e: ort::Error) -> Self {
        OcrError::Inference(e.to_string())
    }
}

/// One line of recognized text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextLine {
    pub text: String,
    /// Mean confidence of the characters, 0–1.
    pub score: f32,
    /// Position in the input image: x, y, width, height.
    pub bbox: [u32; 4],
}

/// Lines below this confidence are dropped.
const MIN_LINE_SCORE: f32 = 0.5;

/// ONNX Runtime allows one environment per process; every engine shares it.
fn environment() -> Result<&'static Environment, OcrError> {
    static ENV: OnceLock<Environment> = OnceLock::new();
    if let Some(env) = ENV.get() {
        return Ok(env);
    }
    let env = ort::init().with_name("interpres-ocr").build()?;
    Ok(ENV.get_or_init(|| env))
}

pub struct Ocr {
    det: Session,
    rec: Session,
    dict: Vec<String>,
}

impl Ocr {
    /// Loads the detection model and the `rec` recognition model from `models_root`
    /// (see [`models::default_root`]).
    pub fn load(models_root: &Path, rec: RecModel) -> Result<Self, OcrError> {
        if !models::is_installed(models_root, rec) {
            return Err(OcrError::NotInstalled);
        }
        let env = environment()?;
        let det = Session::builder(env)?.commit_from_file(models::det_model(models_root))?;
        let rec_session =
            Session::builder(env)?.commit_from_file(models::rec_model(models_root, rec))?;
        let yaml = std::fs::read_to_string(models::rec_config(models_root, rec))?;
        let dict = rec::parse_dictionary(&yaml)?;
        Ok(Self {
            det,
            rec: rec_session,
            dict,
        })
    }

    /// Finds and reads the text in `img`, top to bottom.
    pub fn recognize(&mut self, img: &RgbImage) -> Result<Vec<TextLine>, OcrError> {
        if img.width() < 8 || img.height() < 8 {
            return Ok(vec![]);
        }
        let input = det::preprocess(img);
        let name = self.det.inputs()[0].name().to_string();
        let tensor = Tensor::from_array(([1usize, 3, input.height, input.width], input.data))?;
        let out = self.det.run(ort::inputs![name.as_str() => tensor])?;
        let (_, prob) = out[0].try_extract_tensor::<f32>()?;
        let boxes = det::boxes(
            prob,
            input.width,
            input.height,
            &input.scale,
            img.width(),
            img.height(),
        );
        drop(out);

        let mut lines = Vec::new();
        for bbox in boxes {
            let crop =
                image::imageops::crop_imm(img, bbox[0], bbox[1], bbox[2], bbox[3]).to_image();
            let input = rec::preprocess(&crop);
            let name = self.rec.inputs()[0].name().to_string();
            let tensor = Tensor::from_array(([1usize, 3, rec::HEIGHT, input.width], input.data))?;
            let out = self.rec.run(ort::inputs![name.as_str() => tensor])?;
            let (shape, probs) = out[0].try_extract_tensor::<f32>()?;
            let (steps, classes) = (shape[1] as usize, shape[2] as usize);
            if classes != self.dict.len() + 2 {
                return Err(OcrError::InvalidModel(format!(
                    "model has {classes} classes, dictionary {} entries",
                    self.dict.len()
                )));
            }
            let (text, score) = rec::ctc_decode(probs, steps, classes, &self.dict);
            let text = text.trim().to_string();
            if !text.is_empty() && score >= MIN_LINE_SCORE {
                lines.push(TextLine { text, score, bbox });
            }
        }
        Ok(lines)
    }
}
