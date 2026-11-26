//! The whole live chain on one thread: capture → voice detection → Whisper →
//! filter. The model is loaded when listening starts and freed when it stops.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::vad::FRAME;
use crate::{
    Recognizer, Segmenter, Source, SpeechError, Vad, WhisperModel, capture, filter, models,
};

pub struct ListenConfig {
    pub source: Source,
    pub model: WhisperModel,
    /// Whisper language code (`tr`, `en`); `None` detects it per sentence.
    pub language: Option<String>,
    /// Words to spell right (glossary terms), given to Whisper as a hint.
    pub prompt: String,
    pub models_root: PathBuf,
}

#[derive(Debug)]
pub enum Event {
    /// Models loaded and audio flowing.
    Ready,
    /// One finished sentence.
    Utterance(String),
    /// Listening ended because of this error.
    Failed(SpeechError),
}

/// Running listener; dropping it stops listening and frees the model.
pub struct Listener {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The model to use when the user has not chosen: Turbo with a GPU, else Small.
pub fn default_model() -> WhisperModel {
    if crate::recognize::gpu_name().is_some() {
        WhisperModel::Turbo
    } else {
        WhisperModel::Small
    }
}

/// Starts listening; `on_event` is called from the listener thread.
pub fn listen(cfg: ListenConfig, mut on_event: impl FnMut(Event) + Send + 'static) -> Listener {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let thread = std::thread::spawn(move || {
        if let Err(e) = run(&cfg, &flag, &mut on_event) {
            on_event(Event::Failed(e));
        }
    });
    Listener {
        stop,
        thread: Some(thread),
    }
}

fn run(
    cfg: &ListenConfig,
    stop: &AtomicBool,
    on_event: &mut impl FnMut(Event),
) -> Result<(), SpeechError> {
    if !models::is_installed(&cfg.models_root, cfg.model) {
        return Err(SpeechError::NotInstalled);
    }
    let mut vad = Vad::load(&models::vad_model(&cfg.models_root))?;
    let mut rec = Recognizer::load(&models::whisper_model(&cfg.models_root, cfg.model))?;
    let (tx, rx) = mpsc::channel::<Vec<f32>>();
    let _capture = capture::start(cfg.source, move |s| {
        let _ = tx.send(s.to_vec());
    })?;
    on_event(Event::Ready);

    let mut seg = Segmenter::default();
    let mut buf = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        let Ok(chunk) = rx.recv_timeout(Duration::from_millis(100)) else {
            continue;
        };
        buf.extend(chunk);
        while buf.len() >= FRAME {
            let frame: Vec<f32> = buf.drain(..FRAME).collect();
            let p = vad.probability(&frame)?;
            let Some(audio) = seg.push(&frame, p) else {
                continue;
            };
            let text = rec.transcribe(&audio, cfg.language.as_deref(), &cfg.prompt)?;
            if filter::keep(&audio, &text) {
                on_event(Event::Utterance(text));
            } else {
                tracing::debug!(%text, "dropped utterance");
            }
        }
    }
    Ok(())
}

/// Whisper's code for a DeepL language code: `EN-US` → `en`, `PT-BR` → `pt`,
/// `NB` → `no`.
pub fn whisper_language(deepl: &str) -> String {
    let base = deepl.split('-').next().unwrap_or(deepl).to_lowercase();
    match base.as_str() {
        "nb" => "no".into(),
        _ => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_codes() {
        assert_eq!(whisper_language("TR"), "tr");
        assert_eq!(whisper_language("EN-US"), "en");
        assert_eq!(whisper_language("pt-BR"), "pt");
        assert_eq!(whisper_language("NB"), "no");
        assert_eq!(whisper_language("ZH-HANS"), "zh");
    }

    #[test]
    fn missing_models_fail_at_once() {
        let (tx, rx) = mpsc::channel();
        let _l = listen(
            ListenConfig {
                source: Source::System,
                model: WhisperModel::Small,
                language: None,
                prompt: String::new(),
                models_root: "/nonexistent".into(),
            },
            move |e| {
                let _ = tx.send(e);
            },
        );
        let e = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            matches!(e, Event::Failed(SpeechError::NotInstalled)),
            "{e:?}"
        );
    }
}
