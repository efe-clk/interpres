//! Engine trait, shared types, translator service, settings and secrets.
//!
//! Everything here is independent of the UI and the process model so that the CLI,
//! the desktop app and future daemon / MCP / overlay front-ends share one code path.

mod engine;
mod error;
pub mod secrets;
pub mod settings;
mod translator;
mod types;

pub use engine::{Engine, TextStream};
pub use error::EngineError;
pub use settings::{Settings, UiSettings};
pub use translator::Translator;
pub use types::{
    Capabilities, EngineId, Formality, Language, LanguageKind, TranslateRequest, Translation,
};
