//! Wire format: one JSON object per line.
//!
//! Client → daemon: [`RequestFrame`]. Daemon → client: [`ServerFrame`], either the
//! response to a request (matched by `id`) or an event pushed after `subscribe`.

use std::path::PathBuf;

use interpres_core::documents::DocumentJob;
use interpres_core::{DocumentRequest, EngineError, EngineId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Request {
    Ping,
    DocumentsList,
    DocumentsAdd {
        paths: Vec<PathBuf>,
        engine: EngineId,
        request: DocumentRequest,
        /// Apply the glossary for the pair (only when `request.source` is set).
        use_glossary: bool,
    },
    DocumentsCancel {
        id: u64,
    },
    DocumentsRemove {
        id: u64,
    },
    /// Start receiving [`Event::Documents`] on this connection.
    Subscribe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestFrame {
    pub id: u64,
    #[serde(flatten)]
    pub request: Request,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum Event {
    /// The full job list after a change.
    Documents(Vec<DocumentJob>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ServerFrame {
    Response {
        id: u64,
        result: Result<serde_json::Value, EngineError>,
    },
    Event {
        event: Event,
    },
}
