//! Engine trait, shared types, translator service, settings and secrets.
//!
//! Everything here is independent of the UI and the process model so that the CLI,
//! the desktop app and future daemon / MCP / overlay front-ends share one code path.

pub mod documents;
mod engine;
mod error;
pub mod glossary;
pub mod secrets;
pub mod settings;
pub mod store;
mod translator;
mod types;

pub use documents::{DocumentEngine, DocumentQueue, DocumentRequest};
pub use engine::{Engine, TextStream};
pub use error::EngineError;
pub use glossary::{Glossary, GlossaryTerm};
pub use settings::{Settings, UiSettings};
pub use store::Store;
pub use translator::Translator;
pub use types::{
    Capabilities, EngineId, Formality, Language, LanguageKind, TranslateRequest, Translation,
};
