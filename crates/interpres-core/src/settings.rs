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
    pub ui: UiSettings,
    pub game: GameSettings,
}

/// Screen translation for games (OCR overlay).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameSettings {
    /// Screen region to read, in logical global coordinates: x, y, width, height.
    pub region: Option<(i32, i32, u32, u32)>,
    /// OCR recognition model: `general` (Chinese, Japanese, English), `korean`, `latin`.
    pub ocr_model: String,
    /// `None` means auto-detect.
    pub source_lang: Option<String>,
    /// `None` falls back to the main target language.
    pub target_lang: Option<String>,
    pub caption: CaptionStyle,
}

/// How the translated caption looks and where it goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptionStyle {
    /// `below` or `above` the captured region, or `custom` (uses `area`).
    pub placement: String,
    /// Caption rectangle for `custom`: x, y, width, height (logical screen coordinates).
    pub area: Option<(i32, i32, u32, u32)>,
    /// Text size in logical pixels; `None` fits the text to the box.
    pub font_size: Option<u32>,
    /// Background colour as `#rrggbb`.
    pub background: String,
    /// Background opacity, 0–100.
    pub opacity: u8,
}

impl Default for CaptionStyle {
    fn default() -> Self {
        Self {
            placement: "below".into(),
            area: None,
            font_size: None,
            background: "#101014".into(),
            opacity: 85,
        }
    }
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            region: None,
            ocr_model: "general".into(),
            source_lang: None,
            target_lang: None,
            caption: CaptionStyle::default(),
        }
    }
}

/// Preferences of graphical front-ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    /// `dark` or `light`.
    pub theme: String,
    /// Interface language, e.g. `tr` or `en`.
    pub language: String,
    pub shortcuts: Shortcuts,
}

/// Keyboard shortcuts as `mod+shift+k` (`mod` is Ctrl).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shortcuts {
    pub palette: String,
    pub translate: String,
    pub swap: String,
    pub copy: String,
    pub add_files: String,
    /// System-wide: starts or stops the game overlay.
    pub overlay: String,
    /// System-wide: opens the quick-translate window.
    pub quick: String,
    /// System-wide: shows the main window, or sends it to the tray.
    pub toggle: String,
}

impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            palette: "mod+k".into(),
            translate: "mod+enter".into(),
            swap: "mod+shift+s".into(),
            copy: "mod+shift+c".into(),
            add_files: "mod+o".into(),
            overlay: "mod+alt+o".into(),
            quick: "mod+alt+t".into(),
            toggle: "alt+q".into(),
        }
    }
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            language: "tr".into(),
            shortcuts: Shortcuts::default(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            engine: EngineId::Deepl,
            fallback_engine: None,
            source_lang: None,
            target_lang: None,
            ui: UiSettings::default(),
            game: GameSettings::default(),
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
            ui: UiSettings {
                theme: "light".into(),
                language: "en".into(),
                shortcuts: Shortcuts {
                    quick: "mod+shift+space".into(),
                    ..Shortcuts::default()
                },
            },
            game: GameSettings {
                region: Some((10, 20, 800, 120)),
                ocr_model: "latin".into(),
                source_lang: None,
                target_lang: Some("TR".into()),
                caption: CaptionStyle {
                    placement: "custom".into(),
                    area: Some((0, 900, 1600, 100)),
                    font_size: Some(28),
                    background: "#ffffff".into(),
                    opacity: 40,
                },
            },
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
        assert_eq!(s.ui.theme, "dark");
        assert_eq!(s.ui.shortcuts.palette, "mod+k");
        assert_eq!(s.game.ocr_model, "general");
    }
}
