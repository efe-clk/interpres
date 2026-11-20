//! One overlay step: capture → (if the picture changed) OCR → (if the text changed)
//! translate.

use std::sync::Arc;

use image::RgbImage;
use interpres_core::{EngineError, TranslateRequest, Translator};
use interpres_ocr::{Ocr, OcrError, TextLine};
use serde::{Deserialize, Serialize};

use crate::Region;
use crate::capture::{Capture, CaptureError};
use crate::change::Fingerprint;

/// Reads text from an image (the OCR engine, or a fake in tests).
pub trait Recognizer: Send {
    fn recognize(&mut self, img: &RgbImage) -> Result<Vec<TextLine>, OcrError>;
}

impl Recognizer for Ocr {
    fn recognize(&mut self, img: &RgbImage) -> Result<Vec<TextLine>, OcrError> {
        Ocr::recognize(self, img)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OverlayError {
    #[error(transparent)]
    Capture(#[from] CaptureError),
    #[error(transparent)]
    Ocr(#[from] OcrError),
    #[error(transparent)]
    Translate(#[from] EngineError),
}

/// What the overlay shows. An empty `source` means the text disappeared.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub source: String,
    pub translation: String,
    pub lines: Vec<TextLine>,
}

pub struct Pipeline {
    pub region: Region,
    pub source_lang: Option<String>,
    pub target_lang: String,
    capture: Box<dyn Capture>,
    ocr: Box<dyn Recognizer>,
    translator: Arc<Translator>,
    last_fingerprint: Option<Fingerprint>,
    last_text: String,
}

impl Pipeline {
    pub fn new(
        region: Region,
        target_lang: String,
        capture: Box<dyn Capture>,
        ocr: Box<dyn Recognizer>,
        translator: Arc<Translator>,
    ) -> Self {
        Self {
            region,
            source_lang: None,
            target_lang,
            capture,
            ocr,
            translator,
            last_fingerprint: None,
            last_text: String::new(),
        }
    }

    /// Captures and reads the region. `None` when neither the picture nor the text
    /// changed since the last call.
    pub fn read(&mut self) -> Result<Option<(String, Vec<TextLine>)>, OverlayError> {
        let img = self.capture.capture(&self.region)?;
        let fp = Fingerprint::of(&img);
        if self
            .last_fingerprint
            .as_ref()
            .is_some_and(|prev| !fp.differs_from(prev))
        {
            return Ok(None);
        }
        self.last_fingerprint = Some(fp);
        let lines = self.ocr.recognize(&img)?;
        let text = lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if text == self.last_text {
            return Ok(None);
        }
        self.last_text = text.clone();
        Ok(Some((text, lines)))
    }

    /// Reads the region and translates new text. `None` when nothing changed.
    pub async fn step(&mut self) -> Result<Option<Frame>, OverlayError> {
        let Some((source, lines)) = self.read()? else {
            return Ok(None);
        };
        if source.is_empty() {
            return Ok(Some(Frame {
                source,
                translation: String::new(),
                lines,
            }));
        }
        let req = TranslateRequest {
            // Lines of one subtitle belong together; translate them as one sentence.
            text: source.replace('\n', " "),
            source: self.source_lang.clone(),
            target: self.target_lang.clone(),
            formality: Default::default(),
            glossary: None,
        };
        let translation = self.translator.translate(&req).await?.text;
        Ok(Some(Frame {
            source,
            translation,
            lines,
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use interpres_core::{
        Capabilities, Engine, EngineId, Language, LanguageKind, Store, Translation,
    };

    use super::*;

    /// Returns the queued frames in order, repeating the last one.
    struct Frames(Mutex<Vec<RgbImage>>);

    impl Capture for Frames {
        fn capture(&mut self, _: &Region) -> Result<RgbImage, CaptureError> {
            let mut v = self.0.lock().unwrap();
            Ok(if v.len() > 1 {
                v.remove(0)
            } else {
                v[0].clone()
            })
        }
    }

    /// "Reads" the first pixel: 0 = no text, otherwise "line N".
    struct FakeOcr(Arc<AtomicUsize>);

    impl Recognizer for FakeOcr {
        fn recognize(&mut self, img: &RgbImage) -> Result<Vec<TextLine>, OcrError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            let v = img.get_pixel(0, 0)[0];
            Ok(if v == 0 {
                vec![]
            } else {
                vec![TextLine {
                    text: format!("line {}", v / 50),
                    score: 1.0,
                    bbox: [0, 0, 10, 10],
                }]
            })
        }
    }

    struct Upper(Arc<AtomicUsize>);

    #[async_trait]
    impl Engine for Upper {
        fn id(&self) -> EngineId {
            EngineId::Deepl
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                streaming: false,
                documents: false,
                glossary: false,
                formality: false,
            }
        }
        async fn translate(&self, req: &TranslateRequest) -> Result<Translation, EngineError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(Translation {
                text: req.text.to_uppercase(),
                detected_source: None,
                engine: EngineId::Deepl,
            })
        }
        async fn languages(&self, _: LanguageKind) -> Result<Vec<Language>, EngineError> {
            Ok(vec![])
        }
    }

    fn frame(v: u8) -> RgbImage {
        RgbImage::from_pixel(100, 30, image::Rgb([v, v, v]))
    }

    #[tokio::test]
    async fn translates_only_when_text_changes() {
        let (ocr_calls, tr_calls) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let store = Arc::new(Store::open_in_memory().unwrap());
        let translator =
            Arc::new(Translator::new(Arc::new(Upper(tr_calls.clone())), None).with_store(store));
        let frames = vec![frame(50), frame(50), frame(100), frame(0), frame(50)];
        let mut p = Pipeline::new(
            Region {
                x: 0,
                y: 0,
                width: 100,
                height: 30,
            },
            "EN-US".into(),
            Box::new(Frames(Mutex::new(frames))),
            Box::new(FakeOcr(ocr_calls.clone())),
            translator,
        );

        let f = p.step().await.unwrap().unwrap();
        assert_eq!(
            (f.source.as_str(), f.translation.as_str()),
            ("line 1", "LINE 1")
        );
        // Same picture: no OCR, no translation.
        assert!(p.step().await.unwrap().is_none());
        assert_eq!(ocr_calls.load(Ordering::SeqCst), 1);

        assert_eq!(p.step().await.unwrap().unwrap().translation, "LINE 2");
        // Text gone: an empty frame clears the overlay.
        let cleared = p.step().await.unwrap().unwrap();
        assert!(cleared.source.is_empty() && cleared.translation.is_empty());
        // The first line again comes from the cache.
        assert_eq!(p.step().await.unwrap().unwrap().translation, "LINE 1");
        assert_eq!(tr_calls.load(Ordering::SeqCst), 2);
    }
}
