//! Live speech recognition: capture system audio or the microphone, cut it into
//! utterances with a voice detector, and transcribe each one with Whisper.
//!
//! Audio never leaves the machine; only the recognized text goes on to translation.

pub mod capture;
pub mod filter;
pub mod listen;
pub mod models;
pub mod recognize;
pub mod vad;

pub use capture::{Capture, Source};
pub use listen::{Event, ListenConfig, Listener, default_model, listen, whisper_language};
pub use models::WhisperModel;
pub use recognize::Recognizer;
pub use vad::{Segmenter, Vad};

/// Whisper and the voice detector work on 16 kHz mono.
pub const SAMPLE_RATE: usize = 16_000;

#[derive(Debug, thiserror::Error)]
pub enum SpeechError {
    #[error("speech model is not installed")]
    NotInstalled,
    #[error("speech model download failed: {0}")]
    Download(String),
    #[error("speech model is invalid: {0}")]
    InvalidModel(String),
    #[error("speech recognition failed: {0}")]
    Inference(String),
    #[error("no audio device: {0}")]
    NoDevice(String),
    #[error("audio capture failed: {0}")]
    Capture(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl From<ort::Error> for SpeechError {
    fn from(e: ort::Error) -> Self {
        SpeechError::Inference(e.to_string())
    }
}
