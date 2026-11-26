//! Speech to text with whisper.cpp. Built with Vulkan: a GPU is used when there is
//! one (any vendor); otherwise whisper.cpp runs on the CPU in the same process.

use std::path::Path;

use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

use crate::SpeechError;

/// Name of the first GPU whisper.cpp can use, if any.
pub fn gpu_name() -> Option<String> {
    // whisper.cpp and ggml log to stderr; route that into `tracing` first.
    whisper_rs::install_logging_hooks();
    whisper_rs::vulkan::list_devices()
        .into_iter()
        .next()
        .map(|d| d.name)
}

pub struct Recognizer {
    state: WhisperState,
    threads: i32,
}

impl Recognizer {
    /// Loads a `ggml` Whisper model (see [`crate::models::whisper_model`]) and
    /// warms it up. Slow (seconds); call it off the UI thread.
    pub fn load(model: &Path) -> Result<Self, SpeechError> {
        if !model.exists() {
            return Err(SpeechError::NotInstalled);
        }
        whisper_rs::install_logging_hooks();
        let path = model
            .to_str()
            .ok_or_else(|| SpeechError::InvalidModel("path is not UTF-8".into()))?;
        let ctx = WhisperContext::new_with_params(path, WhisperContextParameters::default())
            .map_err(|e| SpeechError::InvalidModel(e.to_string()))?;
        let state = ctx
            .create_state()
            .map_err(|e| SpeechError::InvalidModel(e.to_string()))?;
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(8)) as i32;
        let mut rec = Self { state, threads };
        // The first run compiles the GPU shaders (up to ~20 s on a cold driver
        // cache); do it now so the first sentence is not late.
        rec.transcribe(&vec![0.0; crate::SAMPLE_RATE], Some("en"), "")?;
        Ok(rec)
    }

    /// Transcribes 16 kHz mono audio. `language` is an ISO code such as `tr`;
    /// `None` detects it. `prompt` biases spelling (e.g. glossary terms).
    pub fn transcribe(
        &mut self,
        audio: &[f32],
        language: Option<&str>,
        prompt: &str,
    ) -> Result<String, SpeechError> {
        let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        p.set_language(Some(language.unwrap_or("auto")));
        p.set_n_threads(self.threads);
        // Each utterance stands alone: carrying text over makes Whisper repeat itself.
        p.set_no_context(true);
        p.set_no_timestamps(true);
        p.set_suppress_nst(true);
        p.set_print_progress(false);
        p.set_print_realtime(false);
        p.set_print_timestamps(false);
        p.set_print_special(false);
        if !prompt.trim().is_empty() {
            p.set_initial_prompt(prompt);
        }
        self.state
            .full(p, audio)
            .map_err(|e| SpeechError::Inference(e.to_string()))?;
        let text = self
            .state
            .as_iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        Ok(text.split_whitespace().collect::<Vec<_>>().join(" "))
    }
}
