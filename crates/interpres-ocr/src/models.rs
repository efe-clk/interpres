//! The OCR model files: where they live, whether they are installed, and a verified
//! download from Hugging Face (PaddlePaddle, Apache-2.0), pinned to fixed revisions.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::OcrError;

/// Recognition model, chosen by the script of the game's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecModel {
    /// Chinese (simplified and traditional), Japanese and English.
    #[default]
    General,
    Korean,
    /// English and other Latin-script languages (including Turkish).
    Latin,
}

struct ModelFile {
    repo: &'static str,
    rev: &'static str,
    name: &'static str,
    size: u64,
    sha256: &'static str,
}

const DET: ModelFile = ModelFile {
    repo: "PP-OCRv5_mobile_det_onnx",
    rev: "e6f4fa85f00e168c862bc462aebca69eef9b3d3d",
    name: "inference.onnx",
    size: 4_826_518,
    sha256: "a431985659dc921974177a95adcfbb90fd9e51989a5e04d70d0b75f597b6e61d",
};

fn rec_files(model: RecModel) -> [ModelFile; 2] {
    let (repo, rev, onnx_size, onnx_sha, yml_size, yml_sha) = match model {
        RecModel::General => (
            "PP-OCRv5_mobile_rec_onnx",
            "ed152b8b495f84de93cda5709d768548a9127622",
            16_534_782,
            "da72dc72ca4dc220df0dfde68c1dedc31c58d3e76a25871122e5056227d50092",
            148_345,
            "5dfeb2777f6d0db8177d8128a8acfcf6e6276dc4ac73ea3bf0dc06d6a5e85d8e",
        ),
        RecModel::Korean => (
            "korean_PP-OCRv5_mobile_rec_onnx",
            "5c6f574b8e2230adf4287b33e736d71b9fabd28e",
            13_418_787,
            "92f0b7785e64fc9090106a241cf4c1eb97472824558272751b88a2a4476d3a08",
            96_039,
            "f757fa1c40e99edcf27e9cce879b93eb2a51fa46f5ef39095689b8c37dd75998",
        ),
        RecModel::Latin => (
            "latin_PP-OCRv5_mobile_rec_onnx",
            "89d3a50e2c27e2e7cceeab0e944c25c807d5db4f",
            8_042_023,
            "7888113072263cb471b93f66dd5e2ad70548dc526fa1ace760d0d973dd121498",
            6_817,
            "0bbe984570f597af3638e50bdf2e8276f3ab26a61966096538b3b0d1849f5c84",
        ),
    };
    [
        ModelFile {
            repo,
            rev,
            name: "inference.onnx",
            size: onnx_size,
            sha256: onnx_sha,
        },
        ModelFile {
            repo,
            rev,
            name: "inference.yml",
            size: yml_size,
            sha256: yml_sha,
        },
    ]
}

/// `<data dir>/interpres/models`.
pub fn default_root() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "interpres").map(|d| d.data_dir().join("models"))
}

fn path_of(root: &Path, f: &ModelFile) -> PathBuf {
    root.join(f.repo).join(f.name)
}

pub fn det_model(root: &Path) -> PathBuf {
    path_of(root, &DET)
}

pub fn rec_model(root: &Path, model: RecModel) -> PathBuf {
    path_of(root, &rec_files(model)[0])
}

pub fn rec_config(root: &Path, model: RecModel) -> PathBuf {
    path_of(root, &rec_files(model)[1])
}

fn files(model: RecModel) -> Vec<ModelFile> {
    let mut v = vec![DET];
    v.extend(rec_files(model));
    v
}

/// Total download size in bytes.
pub fn download_size(model: RecModel) -> u64 {
    files(model).iter().map(|f| f.size).sum()
}

/// All files present with the expected size (the hash is checked when downloading).
pub fn is_installed(root: &Path, model: RecModel) -> bool {
    files(model)
        .iter()
        .all(|f| std::fs::metadata(path_of(root, f)).is_ok_and(|m| m.len() == f.size))
}

/// Downloads the missing files for `model` from `base_url` (normally
/// `https://huggingface.co`), verifying each against its SHA-256. Calls
/// `progress(done_bytes, total_bytes)` as data arrives.
pub async fn download(
    root: &Path,
    model: RecModel,
    base_url: &str,
    progress: impl Fn(u64, u64),
) -> Result<(), OcrError> {
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
            "{}/PaddlePaddle/{}/resolve/{}/{}",
            base_url.trim_end_matches('/'),
            f.repo,
            f.rev,
            f.name
        );
        let mut resp = http
            .get(&url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| OcrError::Download(e.to_string()))?;
        // Write to a temporary file; move into place only after the hash matches.
        let tmp = dest.with_extension("part");
        let mut out = tokio::fs::File::create(&tmp).await?;
        let mut hasher = Sha256::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| OcrError::Download(e.to_string()))?
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
            return Err(OcrError::Download(format!(
                "{} failed verification",
                f.name
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
        assert_eq!(
            download_size(RecModel::Latin),
            4_826_518 + 8_042_023 + 6_817
        );
        let root = Path::new("/m");
        assert_eq!(
            det_model(root),
            Path::new("/m/PP-OCRv5_mobile_det_onnx/inference.onnx")
        );
        assert_eq!(
            rec_config(root, RecModel::Korean),
            Path::new("/m/korean_PP-OCRv5_mobile_rec_onnx/inference.yml")
        );
        assert!(!is_installed(Path::new("/nonexistent"), RecModel::General));
    }
}
