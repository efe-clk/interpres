//! The speech model files: Whisper (whisper.cpp `ggml` format, MIT) and Silero VAD
//! (ONNX, MIT), downloaded from Hugging Face at pinned revisions and verified.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::SpeechError;

/// Whisper model size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WhisperModel {
    /// Fast enough on the CPU; weaker on Turkish (26% word errors measured).
    #[default]
    Small,
    /// `large-v3-turbo`, 5-bit: far more accurate (8.7%), needs a GPU to be live.
    Turbo,
}

struct ModelFile {
    /// `owner/name` on Hugging Face.
    repo: &'static str,
    rev: &'static str,
    /// Path inside the repository.
    path: &'static str,
    size: u64,
    sha256: &'static str,
}

const VAD: ModelFile = ModelFile {
    repo: "onnx-community/silero-vad",
    rev: "e71cae966052b992a7eca6b17738916ce0eca4ec",
    path: "onnx/model.onnx",
    size: 2_243_022,
    sha256: "a4a068cd6cf1ea8355b84327595838ca748ec29a25bc91fc82e6c299ccdc5808",
};

const WHISPER_REPO: &str = "ggerganov/whisper.cpp";
const WHISPER_REV: &str = "5359861c739e955e79d9a303bcbc70fb988958b1";

fn whisper_file(model: WhisperModel) -> ModelFile {
    let (path, size, sha256) = match model {
        WhisperModel::Small => (
            "ggml-small.bin",
            487_601_967,
            "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        ),
        WhisperModel::Turbo => (
            "ggml-large-v3-turbo-q5_0.bin",
            574_041_195,
            "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
        ),
    };
    ModelFile {
        repo: WHISPER_REPO,
        rev: WHISPER_REV,
        path,
        size,
        sha256,
    }
}

/// `<data dir>/interpres/models` (shared with the OCR models).
pub fn default_root() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "interpres").map(|d| d.data_dir().join("models"))
}

/// `<root>/<repo name>/<file name>`, e.g. `whisper.cpp/ggml-small.bin`.
fn path_of(root: &Path, f: &ModelFile) -> PathBuf {
    let repo = f.repo.rsplit('/').next().unwrap_or(f.repo);
    let name = f.path.rsplit('/').next().unwrap_or(f.path);
    root.join(repo).join(name)
}

pub fn vad_model(root: &Path) -> PathBuf {
    path_of(root, &VAD)
}

pub fn whisper_model(root: &Path, model: WhisperModel) -> PathBuf {
    path_of(root, &whisper_file(model))
}

fn files(model: WhisperModel) -> [ModelFile; 2] {
    [VAD, whisper_file(model)]
}

/// Total download size in bytes.
pub fn download_size(model: WhisperModel) -> u64 {
    files(model).iter().map(|f| f.size).sum()
}

/// All files present with the expected size (the hash is checked when downloading).
pub fn is_installed(root: &Path, model: WhisperModel) -> bool {
    files(model)
        .iter()
        .all(|f| std::fs::metadata(path_of(root, f)).is_ok_and(|m| m.len() == f.size))
}

/// Downloads the missing files for `model` from `base_url` (normally
/// `https://huggingface.co`), verifying each against its SHA-256. Calls
/// `progress(done_bytes, total_bytes)` as data arrives.
pub async fn download(
    root: &Path,
    model: WhisperModel,
    base_url: &str,
    progress: impl Fn(u64, u64),
) -> Result<(), SpeechError> {
    let total = download_size(model);
    let mut done = 0u64;
    let http = reqwest::Client::new();
    for f in files(model) {
        let dest = path_of(root, &f);
        if std::fs::metadata(&dest).is_ok_and(|m| m.len() == f.size) {
            done += f.size;
            progress(done, total);
            continue;
        }
        tokio::fs::create_dir_all(dest.parent().expect("has parent")).await?;
        let url = format!(
            "{}/{}/resolve/{}/{}",
            base_url.trim_end_matches('/'),
            f.repo,
            f.rev,
            f.path
        );
        let mut resp = http
            .get(&url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| SpeechError::Download(e.to_string()))?;
        // Write to a temporary file; move into place only after the hash matches.
        let tmp = dest.with_extension("part");
        let mut out = tokio::fs::File::create(&tmp).await?;
        let mut hasher = Sha256::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| SpeechError::Download(e.to_string()))?
        {
            hasher.update(&chunk);
            out.write_all(&chunk).await?;
            done += chunk.len() as u64;
            progress(done.min(total), total);
        }
        out.flush().await?;
        drop(out);
        let hash: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if hash != f.sha256 {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(SpeechError::Download(format!(
                "{} failed verification",
                f.path
            )));
        }
        tokio::fs::rename(&tmp, &dest).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_paths() {
        assert_eq!(download_size(WhisperModel::Small), 2_243_022 + 487_601_967);
        let root = Path::new("/m");
        assert_eq!(vad_model(root), Path::new("/m/silero-vad/model.onnx"));
        assert_eq!(
            whisper_model(root, WhisperModel::Turbo),
            Path::new("/m/whisper.cpp/ggml-large-v3-turbo-q5_0.bin")
        );
        assert!(!is_installed(
            Path::new("/nonexistent"),
            WhisperModel::Small
        ));
    }

    #[tokio::test]
    async fn rejects_a_file_with_the_wrong_hash() {
        use wiremock::matchers::path;
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(path(format!(
            "/onnx-community/silero-vad/resolve/{}/onnx/model.onnx",
            VAD.rev
        )))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0u8; 64]))
        .mount(&server)
        .await;
        let root = std::env::temp_dir().join(format!("interpres-speech-{}", std::process::id()));
        let err = download(&root, WhisperModel::Small, &server.uri(), |_, _| {})
            .await
            .unwrap_err();
        assert!(matches!(err, SpeechError::Download(m) if m.contains("verification")));
        // Nothing is left behind: neither the model nor the partial file.
        assert!(!vad_model(&root).exists());
        assert!(!vad_model(&root).with_extension("part").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
