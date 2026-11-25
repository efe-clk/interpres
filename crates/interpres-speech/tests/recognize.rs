//! End-to-end on real speech (FLEURS, see tests/fixtures/README.md): voice
//! detection cuts the clip into utterances, Whisper transcribes them. Skipped
//! when the models are not installed (set `INTERPRES_SPEECH_MODELS` or install
//! them into the default folder).

use std::path::PathBuf;

use interpres_speech::vad::FRAME;
use interpres_speech::{Recognizer, Segmenter, Vad, WhisperModel, filter, models};

fn root() -> Option<PathBuf> {
    std::env::var_os("INTERPRES_SPEECH_MODELS")
        .map(Into::into)
        .or_else(models::default_root)
}

fn fixture(name: &str) -> (Vec<f32>, String) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let pcm = std::fs::read(dir.join(format!("{name}.pcm"))).unwrap();
    let audio = pcm
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_le_bytes(*b) as f32 / 32768.0)
        .collect();
    let text = std::fs::read_to_string(dir.join(format!("{name}.txt"))).unwrap();
    (audio, text)
}

/// One second of silence, the clip, one second of silence; cut into utterances.
fn utterances(vad: &mut Vad, clip: &[f32]) -> Vec<Vec<f32>> {
    let mut audio = vec![0.0; 16_000];
    audio.extend_from_slice(clip);
    audio.extend(vec![0.0; 16_000]);
    let mut seg = Segmenter::default();
    let mut out = Vec::new();
    for frame in audio.as_chunks::<FRAME>().0 {
        let p = vad.probability(frame).unwrap();
        out.extend(seg.push(frame, p));
    }
    out.extend(seg.finish());
    out
}

fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect()
}

/// Word error rate of `hyp` against `reference`.
fn wer(reference: &str, hyp: &str) -> f32 {
    let (r, h) = (words(reference), words(hyp));
    let mut prev: Vec<usize> = (0..=h.len()).collect();
    for (i, x) in r.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, y) in h.iter().enumerate() {
            cur.push(
                (prev[j] + (x != y) as usize)
                    .min(prev[j + 1] + 1)
                    .min(cur[j] + 1),
            );
        }
        prev = cur;
    }
    prev[h.len()] as f32 / r.len() as f32
}

#[test]
fn silence_gives_no_utterance() {
    let Some(root) = root().filter(|r| models::vad_model(r).exists()) else {
        return;
    };
    let mut vad = Vad::load(&models::vad_model(&root)).unwrap();
    assert!(utterances(&mut vad, &vec![0.0; 32_000]).is_empty());
}

#[test]
fn turkish_speech_is_cut_and_transcribed() {
    let Some(root) = root() else { return };
    // Small is the CPU default; Turbo is what a GPU runs. Check whichever exists.
    for (model, max_wer) in [(WhisperModel::Turbo, 0.2), (WhisperModel::Small, 0.5)] {
        if !models::is_installed(&root, model) {
            continue;
        }
        let mut vad = Vad::load(&models::vad_model(&root)).unwrap();
        let mut rec = Recognizer::load(&models::whisper_model(&root, model)).unwrap();
        for name in ["tr_1", "tr_2"] {
            let (clip, reference) = fixture(name);
            let parts = utterances(&mut vad, &clip);
            assert!(!parts.is_empty(), "{name}: no speech found");
            let text: Vec<String> = parts
                .iter()
                .map(|a| rec.transcribe(a, Some("tr"), "").unwrap())
                .filter(|t| !filter::is_hallucination(t))
                .collect();
            let text = text.join(" ");
            let e = wer(&reference, &text);
            eprintln!("{model:?} {name}: {:.0}% | {text}", e * 100.0);
            assert!(e <= max_wer, "{model:?} {name}: WER {e} for {text:?}");
        }
    }
}
