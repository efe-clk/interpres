use serde::{Deserialize, Serialize};

use crate::glossary::Glossary;

/// Identifies a translation engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EngineId {
    Deepl,
}

impl EngineId {
    pub fn as_str(self) -> &'static str {
        match self {
            EngineId::Deepl => "deepl",
        }
    }
}

impl std::fmt::Display for EngineId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Requested formality of the translation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Formality {
    #[default]
    Default,
    More,
    Less,
    PreferMore,
    PreferLess,
}

/// A single text translation request.
///
/// Language codes follow DeepL conventions (e.g. `EN-US`, `DE`, `PT-BR`).
/// `source: None` means auto-detect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslateRequest {
    pub text: String,
    pub source: Option<String>,
    pub target: String,
    #[serde(default)]
    pub formality: Formality,
    /// Terms to enforce. Engines use it only when `source` is set.
    #[serde(default)]
    pub glossary: Option<Glossary>,
}

/// The result of a translation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Translation {
    pub text: String,
    pub detected_source: Option<String>,
    pub engine: EngineId,
}

/// What an engine supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub streaming: bool,
    pub documents: bool,
    pub glossary: bool,
    pub formality: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguageKind {
    Source,
    Target,
}

/// A language an engine can translate from or to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Language {
    pub code: String,
    pub name: String,
    pub supports_formality: bool,
}
