//! Commands invoked from `src/lib/api.ts`. Errors are returned as `EngineError` so the
//! UI can show one set of states (no key / quota / network / ...).

use interpres_core::{
    Engine, EngineError, EngineId, Language, LanguageKind, Settings, TranslateRequest, Translation,
    secrets,
};
use interpres_engine_deepl::{DeeplEngine, Usage};
use serde::Serialize;

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
pub async fn translate(req: TranslateRequest) -> CmdResult<Translation> {
    deepl()?.translate(&req).await
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
pub fn save_settings(settings: Settings) -> CmdResult<()> {
    settings
        .save(&Settings::default_path().map_err(internal)?)
        .map_err(internal)
}

/// Whether the `claude` command (Claude Code) is on PATH.
#[tauri::command]
pub fn claude_code_found() -> bool {
    let names: &[&str] = if cfg!(windows) {
        &["claude.exe", "claude.cmd"]
    } else {
        &["claude"]
    };
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|dir| names.iter().any(|n| dir.join(n).is_file()))
    })
}
