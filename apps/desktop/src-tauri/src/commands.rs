//! Commands invoked from `src/lib/api.ts`. Errors are returned as `EngineError` so the
//! UI can show one set of states (no key / quota / network / ...).

use std::path::PathBuf;
use std::sync::Arc;

use interpres_core::documents::DocumentJob;
use interpres_core::store::{GlossaryPair, HistoryEntry, TermRow};
use interpres_core::{
    DocumentRequest, Engine, EngineError, EngineId, Language, LanguageKind, Settings, Store,
    TranslateRequest, Translation, Translator, secrets,
};
use interpres_engine_deepl::{DeeplEngine, Usage};
use serde::Serialize;

use crate::documents::Documents;
use tauri::State;

type CmdResult<T> = Result<T, EngineError>;

fn internal(e: impl std::fmt::Display) -> EngineError {
    EngineError::Service(e.to_string())
}

fn deepl() -> CmdResult<DeeplEngine> {
    secrets::get_key(EngineId::Deepl)
        .map_err(internal)?
        .map(DeeplEngine::new)
        .ok_or(EngineError::MissingKey)
}

/// `free` or `pro`, derived from the key suffix.
fn plan_of(key: &str) -> &'static str {
    if key.ends_with(":fx") { "free" } else { "pro" }
}

#[tauri::command]
pub async fn translate(
    store: State<'_, Arc<Store>>,
    req: TranslateRequest,
) -> CmdResult<Translation> {
    Translator::new(Arc::new(deepl()?), None)
        .with_store(store.inner().clone())
        .translate(&req)
        .await
}

#[tauri::command]
pub async fn languages(kind: LanguageKind) -> CmdResult<Vec<Language>> {
    deepl()?.languages(kind).await
}

#[derive(Serialize)]
pub struct KeyStatus {
    /// `None` when no key is stored, else `free` or `pro`.
    deepl: Option<&'static str>,
}

#[tauri::command]
pub fn key_status() -> CmdResult<KeyStatus> {
    let key = secrets::get_key(EngineId::Deepl).map_err(internal)?;
    Ok(KeyStatus {
        deepl: key.as_deref().map(plan_of),
    })
}

/// Verifies the key against DeepL, then stores it. Returns the plan.
#[tauri::command]
pub async fn set_deepl_key(key: String) -> CmdResult<&'static str> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err(EngineError::MissingKey);
    }
    DeeplEngine::new(key.clone()).usage().await?;
    secrets::set_key(EngineId::Deepl, &key).map_err(internal)?;
    Ok(plan_of(&key))
}

#[tauri::command]
pub fn delete_deepl_key() -> CmdResult<()> {
    secrets::delete_key(EngineId::Deepl).map_err(internal)
}

#[tauri::command]
pub async fn deepl_usage() -> CmdResult<Usage> {
    deepl()?.usage().await
}

#[tauri::command]
pub fn get_settings() -> CmdResult<Settings> {
    Settings::load(&Settings::default_path().map_err(internal)?).map_err(internal)
}

#[tauri::command]
pub fn save_settings(app: tauri::AppHandle, settings: Settings) -> CmdResult<()> {
    settings
        .save(&Settings::default_path().map_err(internal)?)
        .map_err(internal)?;
    crate::register_quick_shortcut(&app, &settings.ui.shortcuts.quick);
    Ok(())
}

#[tauri::command]
pub async fn documents_list(docs: State<'_, Arc<Documents>>) -> CmdResult<Vec<DocumentJob>> {
    docs.list().await
}

#[tauri::command]
pub async fn documents_add(
    docs: State<'_, Arc<Documents>>,
    paths: Vec<PathBuf>,
    engine: EngineId,
    request: DocumentRequest,
    use_glossary: bool,
) -> CmdResult<Vec<u64>> {
    docs.add(paths, engine, request, use_glossary).await
}

#[tauri::command]
pub async fn documents_cancel(docs: State<'_, Arc<Documents>>, id: u64) -> CmdResult<()> {
    docs.cancel(id).await
}

#[tauri::command]
pub async fn documents_remove(docs: State<'_, Arc<Documents>>, id: u64) -> CmdResult<()> {
    docs.remove(id).await
}

#[tauri::command]
pub async fn documents_reveal(docs: State<'_, Arc<Documents>>, id: u64) -> CmdResult<()> {
    tauri_plugin_opener::reveal_item_in_dir(docs.output(id).await?).map_err(internal)
}

#[tauri::command]
pub async fn documents_open(docs: State<'_, Arc<Documents>>, id: u64) -> CmdResult<()> {
    tauri_plugin_opener::open_path(docs.output(id).await?, None::<&str>).map_err(internal)
}

// ---------- Glossary ----------

#[tauri::command]
pub fn glossary_pairs(store: State<'_, Arc<Store>>) -> CmdResult<Vec<GlossaryPair>> {
    store.glossary_pairs().map_err(internal)
}

#[tauri::command]
pub fn glossary_terms(
    store: State<'_, Arc<Store>>,
    source_lang: String,
    target_lang: String,
) -> CmdResult<Vec<TermRow>> {
    store
        .glossary_terms(&source_lang, &target_lang)
        .map_err(internal)
}

#[tauri::command]
pub fn glossary_upsert(
    store: State<'_, Arc<Store>>,
    id: Option<i64>,
    source_lang: String,
    target_lang: String,
    source: String,
    target: String,
) -> CmdResult<()> {
    store
        .upsert_term(id, &source_lang, &target_lang, &source, &target)
        .map_err(|e| EngineError::InvalidRequest(e.to_string()))
}

#[tauri::command]
pub async fn glossary_delete(store: State<'_, Arc<Store>>, id: i64) -> CmdResult<()> {
    let pair = store.term_pair(id).map_err(internal)?;
    store.delete_term(id).map_err(internal)?;
    if let Some((src, tgt)) = pair
        && store
            .glossary_terms(&src, &tgt)
            .map_err(internal)?
            .is_empty()
        && let Ok(engine) = deepl()
        && let Err(e) = engine.delete_glossaries(&src, &tgt).await
    {
        // The local delete worked; a leftover DeepL glossary is harmless.
        tracing::warn!(error = %e, "could not delete the DeepL glossary");
    }
    Ok(())
}

#[tauri::command]
pub fn glossary_import(
    store: State<'_, Arc<Store>>,
    path: PathBuf,
    source_lang: String,
    target_lang: String,
) -> CmdResult<usize> {
    let f = std::fs::File::open(&path).map_err(|e| EngineError::InvalidRequest(e.to_string()))?;
    store
        .import_csv(&source_lang, &target_lang, f)
        .map_err(|e| EngineError::InvalidRequest(e.to_string()))
}

#[tauri::command]
pub fn glossary_export(
    store: State<'_, Arc<Store>>,
    path: PathBuf,
    source_lang: String,
    target_lang: String,
) -> CmdResult<usize> {
    let f = std::fs::File::create(&path).map_err(|e| EngineError::InvalidRequest(e.to_string()))?;
    store
        .export_csv(&source_lang, &target_lang, f)
        .map_err(internal)
}

// ---------- History and cache ----------

#[tauri::command]
pub fn history_list(store: State<'_, Arc<Store>>, query: String) -> CmdResult<Vec<HistoryEntry>> {
    store.history(&query, 200).map_err(internal)
}

#[tauri::command]
pub fn cache_clear(store: State<'_, Arc<Store>>) -> CmdResult<()> {
    store.cache_clear().map_err(internal)
}
