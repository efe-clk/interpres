//! MCP server exposing Interpres: text and document translation, glossary, history.
//!
//! Engine failures (no key, quota, network) are returned as tool results marked as
//! errors, so the calling model can read and react to them.

use std::path::PathBuf;
use std::sync::Arc;

use interpres_core::documents::unique_output_path;
use interpres_core::{
    DocumentEngine, DocumentRequest, EngineError, Formality, Store, TranslateRequest, Translator,
};
use interpres_engine_deepl::DeeplEngine;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ErrorData, ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

/// Creates the DeepL engine for a call. Reading the key per call picks up key changes.
pub type EngineFactory = Arc<dyn Fn() -> Result<DeeplEngine, EngineError> + Send + Sync>;

#[derive(Clone)]
pub struct InterpresMcp {
    store: Arc<Store>,
    engine: EngineFactory,
    #[allow(dead_code, reason = "used by the tool_handler macro")]
    tool_router: ToolRouter<Self>,
}

type ToolResult = Result<CallToolResult, ErrorData>;

fn ok(text: impl Into<String>) -> ToolResult {
    Ok(CallToolResult::success(vec![ContentBlock::text(
        text.into(),
    )]))
}

fn fail(text: impl Into<String>) -> ToolResult {
    Ok(CallToolResult::error(vec![ContentBlock::text(text.into())]))
}

fn engine_error(e: EngineError) -> ToolResult {
    let hint = match e {
        EngineError::MissingKey | EngineError::InvalidKey => {
            " Set the key with `interpres key set deepl` or in the Interpres app."
        }
        EngineError::QuotaExceeded => " The monthly DeepL character limit is reached.",
        _ => "",
    };
    fail(format!("{e}.{hint}"))
}

fn parse_formality(f: Option<&str>) -> Result<Formality, String> {
    Ok(match f.unwrap_or("default") {
        "default" => Formality::Default,
        "more" => Formality::More,
        "less" => Formality::Less,
        "prefer_more" => Formality::PreferMore,
        "prefer_less" => Formality::PreferLess,
        other => return Err(format!("unknown formality: {other}")),
    })
}

fn upper(s: &str) -> String {
    s.trim().to_uppercase()
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TranslateTextArgs {
    /// Text to translate.
    pub text: String,
    /// Target language code, e.g. EN-US, EN-GB, DE, TR, PT-BR.
    pub target_lang: String,
    /// Source language code, e.g. TR. Auto-detected when omitted. The glossary for
    /// the pair applies only when this is given.
    pub source_lang: Option<String>,
    /// One of: default, more, less, prefer_more, prefer_less.
    pub formality: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TranslateDocumentArgs {
    /// Absolute path of a docx, pptx, xlsx, pdf, txt, srt or md file.
    pub path: String,
    /// Target language code, e.g. EN-US, DE, TR.
    pub target_lang: String,
    /// Source language code. Auto-detected when omitted. The glossary applies only
    /// when this is given.
    pub source_lang: Option<String>,
    /// Where to write the result. Defaults to `name.<LANG>.ext` next to the input.
    pub output_path: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PairArgs {
    /// Source language code, e.g. TR.
    pub source_lang: String,
    /// Target language code, e.g. EN. Regional variants map to the base language.
    pub target_lang: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TermArgs {
    pub source_lang: String,
    pub target_lang: String,
    /// The term in the source language.
    pub source: String,
    /// The required translation (omit when removing).
    pub target: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct HistoryArgs {
    /// Only entries whose source or translation contains this text.
    pub query: Option<String>,
    /// Maximum number of entries (default 20).
    pub limit: Option<usize>,
}

#[tool_router]
impl InterpresMcp {
    pub fn new(store: Arc<Store>, engine: EngineFactory) -> Self {
        Self {
            store,
            engine,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Translate text with DeepL. Uses the Interpres cache, glossary and history."
    )]
    async fn translate_text(&self, Parameters(a): Parameters<TranslateTextArgs>) -> ToolResult {
        let formality = match parse_formality(a.formality.as_deref()) {
            Ok(f) => f,
            Err(e) => return fail(e),
        };
        let engine = match (self.engine)() {
            Ok(e) => e,
            Err(e) => return engine_error(e),
        };
        let req = TranslateRequest {
            text: a.text,
            source: a.source_lang.as_deref().map(upper),
            target: upper(&a.target_lang),
            formality,
            glossary: None,
        };
        match Translator::new(Arc::new(engine), None)
            .with_store(self.store.clone())
            .translate(&req)
            .await
        {
            Ok(t) => ok(t.text),
            Err(e) => engine_error(e),
        }
    }

    #[tool(
        description = "Translate a document file with DeepL, keeping its format. Returns the path of the translated file."
    )]
    async fn translate_document(
        &self,
        Parameters(a): Parameters<TranslateDocumentArgs>,
    ) -> ToolResult {
        let input = PathBuf::from(&a.path);
        if !input.is_file() {
            return fail(format!("file not found: {}", a.path));
        }
        let engine = match (self.engine)() {
            Ok(e) => e,
            Err(e) => return engine_error(e),
        };
        let ext = input
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if !engine.supports(&ext) {
            return fail(format!("unsupported file type: .{ext}"));
        }
        let target = upper(&a.target_lang);
        let source = a.source_lang.as_deref().map(upper);
        let glossary = match &source {
            Some(src) => match self.store.glossary(src, &target) {
                Ok(g) => g,
                Err(e) => return fail(e.to_string()),
            },
            None => None,
        };
        let output = a
            .output_path
            .map(PathBuf::from)
            .unwrap_or_else(|| unique_output_path(&input, &target));
        let req = DocumentRequest {
            source,
            target,
            formality: Formality::Default,
            glossary,
        };
        match engine
            .translate_document(&input, &output, &req, &|_| {}, &CancellationToken::new())
            .await
        {
            Ok(()) => ok(output.display().to_string()),
            Err(e) => engine_error(e),
        }
    }

    #[tool(description = "List the language pairs that have glossary terms.")]
    async fn glossary_list(&self) -> ToolResult {
        match self.store.glossary_pairs() {
            Ok(pairs) if pairs.is_empty() => ok("No glossary terms."),
            Ok(pairs) => ok(pairs
                .iter()
                .map(|p| format!("{} -> {}: {} terms", p.source_lang, p.target_lang, p.count))
                .collect::<Vec<_>>()
                .join("\n")),
            Err(e) => fail(e.to_string()),
        }
    }

    #[tool(
        description = "Show the glossary terms of a language pair, one `source -> target` per line."
    )]
    async fn glossary_terms(&self, Parameters(a): Parameters<PairArgs>) -> ToolResult {
        match self.store.glossary_terms(&a.source_lang, &a.target_lang) {
            Ok(terms) if terms.is_empty() => ok("No terms for this pair."),
            Ok(terms) => ok(terms
                .iter()
                .map(|t| format!("{} -> {}", t.source, t.target))
                .collect::<Vec<_>>()
                .join("\n")),
            Err(e) => fail(e.to_string()),
        }
    }

    #[tool(description = "Add a glossary term, or replace the translation of an existing one.")]
    async fn glossary_add(&self, Parameters(a): Parameters<TermArgs>) -> ToolResult {
        let Some(target) = a.target else {
            return fail("target is required");
        };
        match self
            .store
            .upsert_term(None, &a.source_lang, &a.target_lang, &a.source, &target)
        {
            Ok(()) => ok(format!("Added: {} -> {}", a.source.trim(), target.trim())),
            Err(e) => fail(e.to_string()),
        }
    }

    #[tool(description = "Remove a glossary term by its source text.")]
    async fn glossary_remove(&self, Parameters(a): Parameters<TermArgs>) -> ToolResult {
        let terms = match self.store.glossary_terms(&a.source_lang, &a.target_lang) {
            Ok(t) => t,
            Err(e) => return fail(e.to_string()),
        };
        let Some(term) = terms.iter().find(|t| t.source == a.source.trim()) else {
            return fail(format!("no such term: {}", a.source));
        };
        if let Err(e) = self.store.delete_term(term.id) {
            return fail(e.to_string());
        }
        // The pair is empty: drop its DeepL glossary too (best effort).
        if terms.len() == 1
            && let Ok(engine) = (self.engine)()
            && let Err(e) = engine
                .delete_glossaries(&a.source_lang, &a.target_lang)
                .await
        {
            tracing::warn!(error = %e, "could not delete the DeepL glossary");
        }
        ok(format!("Removed: {}", term.source))
    }

    #[tool(description = "Search past translations, newest first.")]
    async fn history_search(&self, Parameters(a): Parameters<HistoryArgs>) -> ToolResult {
        match self
            .store
            .history(a.query.as_deref().unwrap_or(""), a.limit.unwrap_or(20))
        {
            Ok(h) if h.is_empty() => ok("No matching history."),
            Ok(h) => ok(h
                .iter()
                .map(|e| {
                    let src = e
                        .source_lang
                        .as_deref()
                        .or(e.detected_source.as_deref())
                        .unwrap_or("?");
                    format!(
                        "[{src} -> {}] {} => {}",
                        e.target_lang, e.source_text, e.target_text
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")),
            Err(e) => fail(e.to_string()),
        }
    }

    #[tool(description = "Show DeepL character usage for the current billing period.")]
    async fn usage(&self) -> ToolResult {
        let engine = match (self.engine)() {
            Ok(e) => e,
            Err(e) => return engine_error(e),
        };
        match engine.usage().await {
            Ok(u) => ok(format!(
                "{} / {} characters",
                u.character_count, u.character_limit
            )),
            Err(e) => engine_error(e),
        }
    }
}

#[tool_handler]
impl ServerHandler for InterpresMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("interpres", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Interpres translates text and documents with DeepL and keeps a glossary \
                 and history. Language codes follow DeepL (EN-US, DE, TR, PT-BR...). \
                 Glossaries apply only when source_lang is given.",
            )
    }
}
