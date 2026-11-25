//! Drops what Whisper invents on silence or music ("Thanks for watching",
//! "Altyazı M.K.") and audio too quiet to be speech. The phrase list follows the
//! one in yusufipk/dikte (ideas only; no code taken).

/// Below this level an utterance is treated as silence.
const MIN_DBFS: f32 = -55.0;

/// Phrases Whisper is known to produce from silence (normalized: lower case,
/// letters and digits only, single spaces). The whole text must match.
const WHOLE: &[&str] = &[
    "thanks for watching",
    "thank you for watching",
    "thank you",
    "thank you so much for watching",
    "please subscribe",
    "subscribe to the channel",
    "izlediğiniz için teşekkürler",
    "izlediğiniz için teşekkür ederim",
    "abone olmayı unutmayın",
    "altyazı",
    "music",
    "müzik",
    "you",
    "ご視聴ありがとうございました",
    "продолжение следует",
];

/// Credits that leak from subtitle files in Whisper's training data; any text
/// containing one is dropped.
const CONTAINS: &[&str] = &[
    "altyazı m k",
    "amara org",
    "subtitles by",
    "untertitel im auftrag",
    "sous titres réalisés",
];

/// Loudness in dBFS (RMS); silence is -inf.
pub fn dbfs(audio: &[f32]) -> f32 {
    if audio.is_empty() {
        return f32::NEG_INFINITY;
    }
    let power = audio.iter().map(|s| s * s).sum::<f32>() / audio.len() as f32;
    10.0 * power.log10()
}

fn normalize(text: &str) -> String {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// True when `text` is empty, non-speech or a known invented phrase.
pub fn is_hallucination(text: &str) -> bool {
    let norm = normalize(text);
    norm.is_empty() || WHOLE.contains(&norm.as_str()) || CONTAINS.iter().any(|p| norm.contains(p))
}

/// Whether a recognized utterance should be shown.
pub fn keep(audio: &[f32], text: &str) -> bool {
    dbfs(audio) >= MIN_DBFS && !is_hallucination(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loudness() {
        assert_eq!(dbfs(&[]), f32::NEG_INFINITY);
        assert!((dbfs(&[1.0, -1.0]) - 0.0).abs() < 1e-4);
        assert!((dbfs(&[0.1; 100]) + 20.0).abs() < 1e-3);
    }

    #[test]
    fn known_inventions_are_dropped() {
        for t in [
            "Thanks for watching!",
            "  ",
            "[Müzik]",
            "♪♪",
            "Altyazı M.K.",
            "Bu bölümün altyazıları Altyazı M.K. tarafından",
            "Subtitles by the Amara.org community",
        ] {
            assert!(is_hallucination(t), "{t}");
        }
    }

    #[test]
    fn real_speech_is_kept() {
        let audio = vec![0.1; 16000];
        for t in [
            "Kapı kilitli.",
            "Thank you for the key, I will open the door.",
            "Müzik çok yüksek.",
        ] {
            assert!(keep(&audio, t), "{t}");
        }
        // Real words but far too quiet to be speech.
        assert!(!keep(&vec![0.0001; 16000], "Kapı kilitli."));
    }
}
