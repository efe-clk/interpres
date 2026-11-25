//! Audio capture: what the speakers play (system audio) or the microphone,
//! converted to 16 kHz mono.
//!
//! System audio is the default output device opened as an input: PipeWire turns
//! that into a capture of the sink, WASAPI into loopback recording.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use serde::{Deserialize, Serialize};

use crate::{SAMPLE_RATE, SpeechError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// What the speakers play: a game, a video, a call.
    #[default]
    System,
    Microphone,
}

/// A running capture; dropping it stops the stream.
pub struct Capture {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Starts capturing `source`; `on_audio` receives 16 kHz mono samples on the audio
/// thread, so it should only hand them on (e.g. into a channel).
pub fn start(
    source: Source,
    on_audio: impl FnMut(&[f32]) + Send + 'static,
) -> Result<Capture, SpeechError> {
    let stop = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let flag = stop.clone();
    // cpal streams cannot move between threads: open, run and close on one.
    let thread = std::thread::spawn(move || match open(source, on_audio) {
        Ok(stream) => {
            let _ = ready_tx.send(Ok(()));
            while !flag.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(50));
            }
            drop(stream);
        }
        Err(e) => {
            let _ = ready_tx.send(Err(e));
        }
    });
    match ready_rx.recv() {
        Ok(Ok(())) => Ok(Capture {
            stop,
            thread: Some(thread),
        }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(SpeechError::Capture("capture thread ended".into())),
    }
}

fn open(
    source: Source,
    on_audio: impl FnMut(&[f32]) + Send + 'static,
) -> Result<cpal::Stream, SpeechError> {
    let host = cpal::default_host();
    let device = match source {
        Source::System => host.default_output_device(),
        Source::Microphone => host.default_input_device(),
    }
    .ok_or_else(|| SpeechError::NoDevice(format!("{source:?}")))?;
    let config = if device.supports_input() {
        device.default_input_config()
    } else {
        device.default_output_config()
    }
    .map_err(|e| SpeechError::NoDevice(e.to_string()))?;
    let converter = Converter::new(config.sample_rate(), config.channels() as usize);
    tracing::info!(
        ?source,
        rate = config.sample_rate(),
        channels = config.channels(),
        "audio capture"
    );
    let stream = match config.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, config.into(), converter, on_audio),
        SampleFormat::I16 => build::<i16>(&device, config.into(), converter, on_audio),
        SampleFormat::I32 => build::<i32>(&device, config.into(), converter, on_audio),
        SampleFormat::U8 => build::<u8>(&device, config.into(), converter, on_audio),
        other => Err(SpeechError::Capture(format!(
            "unsupported sample format {other}"
        ))),
    }?;
    stream
        .play()
        .map_err(|e| SpeechError::Capture(e.to_string()))?;
    Ok(stream)
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut converter: Converter,
    mut on_audio: impl FnMut(&[f32]) + Send + 'static,
) -> Result<cpal::Stream, SpeechError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let mut samples = Vec::new();
    device
        .build_input_stream(
            config,
            move |data: &[T], _: &_| {
                samples.clear();
                samples.extend(data.iter().map(|s| s.to_sample::<f32>()));
                let out = converter.push(&samples);
                if !out.is_empty() {
                    on_audio(&out);
                }
            },
            |e| tracing::warn!(error = %e, "audio stream error"),
            None,
        )
        .map_err(|e| SpeechError::Capture(e.to_string()))
}

/// Downmixes interleaved audio to mono and resamples it to 16 kHz. Each output
/// sample averages the input samples it covers, which also filters out most of
/// what would alias.
struct Converter {
    channels: usize,
    /// Input samples per output sample.
    step: f64,
    pending: Vec<f32>,
    pos: f64,
}

impl Converter {
    fn new(rate: u32, channels: usize) -> Self {
        Self {
            channels: channels.max(1),
            step: rate as f64 / SAMPLE_RATE as f64,
            pending: Vec::new(),
            pos: 0.0,
        }
    }

    fn push(&mut self, interleaved: &[f32]) -> Vec<f32> {
        self.pending.extend(
            interleaved
                .chunks_exact(self.channels)
                .map(|f| f.iter().sum::<f32>() / self.channels as f32),
        );
        let mut out = Vec::new();
        while (self.pos + self.step).ceil() as usize <= self.pending.len() {
            let start = self.pos as usize;
            let end = ((self.pos + self.step) as usize).max(start + 1);
            let window = &self.pending[start..end];
            out.push(window.iter().sum::<f32>() / window.len() as f32);
            self.pos += self.step;
        }
        let used = self.pos as usize;
        self.pending.drain(..used);
        self.pos -= used as f64;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixteen_khz_mono_passes_through() {
        let mut c = Converter::new(16_000, 1);
        let input: Vec<f32> = (0..1000).map(|i| i as f32).collect();
        assert_eq!(c.push(&input), input);
    }

    #[test]
    fn stereo_48k_becomes_16k_mono_and_keeps_a_tone() {
        let mut c = Converter::new(48_000, 2);
        // 1 s of a 300 Hz tone, same on both channels, fed in uneven chunks.
        let tone: Vec<f32> = (0..48_000)
            .flat_map(|i| {
                let s = (i as f32 * 2.0 * std::f32::consts::PI * 300.0 / 48_000.0).sin() * 0.5;
                [s, s]
            })
            .collect();
        let out: Vec<f32> = tone.chunks(1234).flat_map(|ch| c.push(ch)).collect();
        assert!((out.len() as i64 - 16_000).abs() <= 1, "{}", out.len());
        let peak = out.iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!((0.45..=0.5).contains(&peak), "peak {peak}");
    }

    #[test]
    fn odd_rates_keep_the_length() {
        let mut c = Converter::new(44_100, 1);
        let out: Vec<f32> = vec![0.1f32; 44_100]
            .chunks(512)
            .flat_map(|ch| c.push(ch))
            .collect();
        assert!((out.len() as i64 - 16_000).abs() <= 1, "{}", out.len());
        assert!(out.iter().all(|s| (s - 0.1).abs() < 1e-6));
    }
}
