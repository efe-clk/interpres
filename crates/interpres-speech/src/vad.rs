//! Voice activity detection: Silero VAD (v5, ONNX) gives a speech probability per
//! 32 ms frame; [`Segmenter`] turns those into finished utterances.

use std::path::Path;

use ort::session::Session;
use ort::value::Tensor;

use crate::{SAMPLE_RATE, SpeechError};

/// Samples per frame at 16 kHz (what Silero v5 expects).
pub const FRAME: usize = 512;
/// Silero v5 is fed the last 64 samples of the previous frame before each frame.
const CONTEXT: usize = 64;
const STATE: usize = 2 * 128;

pub struct Vad {
    session: Session,
    state: Vec<f32>,
    context: Vec<f32>,
}

impl Vad {
    pub fn load(model: &Path) -> Result<Self, SpeechError> {
        if !model.exists() {
            return Err(SpeechError::NotInstalled);
        }
        let env = interpres_ocr::environment()?;
        let session = Session::builder(env)?.commit_from_file(model)?;
        Ok(Self {
            session,
            state: vec![0.0; STATE],
            context: vec![0.0; CONTEXT],
        })
    }

    /// Speech probability (0–1) of one 512-sample frame.
    pub fn probability(&mut self, frame: &[f32]) -> Result<f32, SpeechError> {
        let mut input = Vec::with_capacity(CONTEXT + FRAME);
        input.extend_from_slice(&self.context);
        input.extend_from_slice(frame);
        input.resize(CONTEXT + FRAME, 0.0);
        self.context.copy_from_slice(&input[FRAME..]);
        let out = self.session.run(ort::inputs![
            "input" => Tensor::from_array(([1usize, CONTEXT + FRAME], input))?,
            "state" => Tensor::from_array(([2usize, 1, 128], self.state.clone()))?,
            "sr" => Tensor::from_array(([0usize; 0], vec![SAMPLE_RATE as i64]))?,
        ])?;
        let (_, prob) = out["output"].try_extract_tensor::<f32>()?;
        let (_, state) = out["stateN"].try_extract_tensor::<f32>()?;
        self.state.copy_from_slice(state);
        Ok(prob.first().copied().unwrap_or(0.0))
    }
}

/// Speech starts above this probability…
const START: f32 = 0.5;
/// …and continues until it drops below this one for [`END_SILENCE`] frames.
const STOP: f32 = 0.35;
/// ~0.5 s of silence ends an utterance.
const END_SILENCE: usize = 16;
/// Utterances shorter than ~0.25 s are dropped (clicks, breaths).
const MIN_SPEECH: usize = 8;
/// Long speech is cut every ~15 s so the caption does not wait forever.
const MAX_FRAMES: usize = 15 * SAMPLE_RATE / FRAME;
/// ~0.1 s of audio before the start is kept, so the first syllable is not lost.
const PRE_ROLL: usize = 3;
/// An utterance must be this much louder than the background (dikte uses 10 dB).
const ABOVE_FLOOR_DB: f32 = 10.0;

/// Groups frames into utterances from their speech probabilities.
#[derive(Default)]
pub struct Segmenter {
    recent: Vec<Vec<f32>>,
    speech: Vec<f32>,
    speech_frames: usize,
    silence: usize,
    active: bool,
    /// Loudness of non-speech frames (dBFS), slowly averaged.
    floor: Option<f32>,
}

impl Segmenter {
    /// Feeds one frame and its probability; returns an utterance when one ends.
    pub fn push(&mut self, frame: &[f32], prob: f32) -> Option<Vec<f32>> {
        if !self.active {
            if prob >= START {
                self.active = true;
                self.speech = self.recent.concat();
                self.speech_frames = 0;
                self.silence = 0;
            } else {
                let level = crate::filter::dbfs(frame);
                self.floor = Some(match self.floor {
                    Some(f) => f * 0.95 + level * 0.05,
                    None => level,
                });
                self.recent.push(frame.to_vec());
                if self.recent.len() > PRE_ROLL {
                    self.recent.remove(0);
                }
                return None;
            }
        }
        self.speech.extend_from_slice(frame);
        self.speech_frames += 1;
        self.silence = if prob < STOP { self.silence + 1 } else { 0 };
        if self.silence >= END_SILENCE || self.speech_frames >= MAX_FRAMES {
            return self.finish();
        }
        None
    }

    /// Ends the current utterance (e.g. when listening stops).
    pub fn finish(&mut self) -> Option<Vec<f32>> {
        if !self.active {
            return None;
        }
        self.active = false;
        self.recent.clear();
        let voiced = self.speech_frames.saturating_sub(self.silence);
        let audio = std::mem::take(&mut self.speech);
        let loud_enough = self
            .floor
            .is_none_or(|f| crate::filter::dbfs(&audio) >= f + ABOVE_FLOOR_DB);
        (voiced >= MIN_SPEECH && loud_enough).then_some(audio)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(amp: f32) -> Vec<f32> {
        (0..FRAME).map(|i| amp * (i as f32 * 0.3).sin()).collect()
    }

    #[test]
    fn speech_between_silences_is_one_utterance_with_pre_roll() {
        let mut s = Segmenter::default();
        let quiet = tone(0.001);
        for _ in 0..10 {
            assert!(s.push(&quiet, 0.05).is_none());
        }
        let loud = tone(0.3);
        for _ in 0..20 {
            assert!(s.push(&loud, 0.9).is_none());
        }
        let mut out = None;
        for _ in 0..END_SILENCE {
            out = out.or(s.push(&quiet, 0.05));
        }
        let audio = out.expect("utterance");
        assert_eq!(audio.len(), (PRE_ROLL + 20 + END_SILENCE) * FRAME);
        // Silence afterwards produces nothing more.
        assert!(s.push(&quiet, 0.05).is_none());
    }

    #[test]
    fn short_blips_and_quiet_murmur_are_dropped() {
        let mut s = Segmenter::default();
        let quiet = tone(0.01);
        for _ in 0..10 {
            s.push(&quiet, 0.05);
        }
        // Too short.
        for _ in 0..3 {
            s.push(&tone(0.3), 0.9);
        }
        for _ in 0..END_SILENCE {
            assert!(s.push(&quiet, 0.05).is_none());
        }
        // Long enough but no louder than the background.
        for _ in 0..20 {
            s.push(&quiet, 0.9);
        }
        for _ in 0..END_SILENCE {
            assert!(s.push(&quiet, 0.05).is_none());
        }
    }

    #[test]
    fn long_speech_is_cut() {
        let mut s = Segmenter::default();
        let loud = tone(0.3);
        let cuts = (0..MAX_FRAMES * 2)
            .filter_map(|_| s.push(&loud, 0.9))
            .count();
        assert_eq!(cuts, 2);
    }
}
