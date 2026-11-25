//! Live check: `cargo run --release -p interpres-speech --example listen -- system 20 tr`
//! (source: system | mic, seconds, language or `auto`). Prints each utterance.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use interpres_speech::vad::FRAME;
use interpres_speech::{
    Recognizer, Segmenter, Source, Vad, WhisperModel, capture, filter, models, recognize,
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let source = match args.get(1).map(String::as_str) {
        Some("mic") => Source::Microphone,
        _ => Source::System,
    };
    let seconds: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20);
    let lang = args.get(3).map(String::as_str).filter(|l| *l != "auto");
    let root = models::default_root().expect("data dir");
    let gpu = recognize::gpu_name();
    let model = if gpu.is_some() {
        WhisperModel::Turbo
    } else {
        WhisperModel::Small
    };
    println!("gpu: {gpu:?}, model: {model:?}");
    let mut vad = Vad::load(&models::vad_model(&root)).expect("vad");
    let mut rec = Recognizer::load(&models::whisper_model(&root, model)).expect("whisper");

    let (tx, rx) = mpsc::channel::<Vec<f32>>();
    let _capture = capture::start(source, move |s| {
        let _ = tx.send(s.to_vec());
    })
    .expect("capture");
    let mut seg = Segmenter::default();
    let mut buf = Vec::new();
    let end = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < end {
        let Ok(chunk) = rx.recv_timeout(Duration::from_millis(200)) else {
            continue;
        };
        buf.extend(chunk);
        while buf.len() >= FRAME {
            let frame: Vec<f32> = buf.drain(..FRAME).collect();
            let p = vad.probability(&frame).expect("vad");
            if let Some(utt) = seg.push(&frame, p) {
                let t = Instant::now();
                let text = rec.transcribe(&utt, lang, "").expect("whisper");
                let keep = filter::keep(&utt, &text);
                println!(
                    "[{:.1}s audio, {} ms, keep={keep}] {text}",
                    utt.len() as f32 / 16_000.0,
                    t.elapsed().as_millis()
                );
            }
        }
    }
}
