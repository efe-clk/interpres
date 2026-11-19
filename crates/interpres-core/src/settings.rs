//! User settings stored as TOML in the OS config directory.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::EngineId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub engine: EngineId,
    pub fallback_engine: Option<EngineId>,
    /// `None` means auto-detect.
    pub source_lang: Option<String>,
    pub target_lang: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            engine: EngineId::Deepl,
            fallback_engine: None,
            source_lang: None,
            target_lang: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("could not determine the config directory")]
    NoConfigDir,
    #[error("could not read or write {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid settings file {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("could not serialize settings: {0}")]
    Serialize(#[from] toml::ser::Error),
}

impl Settings {
    /// `<config dir>/interpres/settings.toml`.
    pub fn default_path() -> Result<PathBuf, SettingsError> {
        directories::ProjectDirs::from("", "", "interpres")
            .map(|d| d.config_dir().join("settings.toml"))
            .ok_or(SettingsError::NoConfigDir)
    }

    /// Loads settings; a missing file yields the defaults.
    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        match std::fs::read_to_string(path) {
            Ok(s) => toml::from_str(&s).map_err(|source| SettingsError::Parse {
                path: path.into(),
                source,
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(SettingsError::Io {
                path: path.into(),
                source,
            }),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        let io = |source| SettingsError::Io {
            path: path.into(),
            source,
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        std::fs::write(path, toml::to_string_pretty(self)?).map_err(io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("interpres-test-{}-{name}", std::process::id()))
            .join("settings.toml")
    }

    #[test]
    fn missing_file_gives_defaults() {
        let p = temp_path("missing");
        assert_eq!(Settings::load(&p).unwrap(), Settings::default());
    }

    #[test]
    fn roundtrip() {
        let p = temp_path("roundtrip");
        let s = Settings {
            engine: EngineId::ClaudeApi,
            fallback_engine: Some(EngineId::Deepl),
            source_lang: None,
            target_lang: Some("EN-US".into()),
        };
        s.save(&p).unwrap();
        assert_eq!(Settings::load(&p).unwrap(), s);
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }

    #[test]
    fn partial_file_fills_defaults() {
        let s: Settings = toml::from_str("target_lang = \"DE\"").unwrap();
        assert_eq!(s.engine, EngineId::Deepl);
        assert_eq!(s.target_lang.as_deref(), Some("DE"));
    }
}
