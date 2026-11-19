//! Tauri shell. Thin layer: every command delegates to interpres-core and the engines.

mod commands;

use std::sync::Arc;

use interpres_core::{DocumentEngine, DocumentQueue, EngineError, EngineId, Store, secrets};
use interpres_engine_deepl::DeeplEngine;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Command-line flag that opens the quick-translate window (for Wayland compositor binds).
const QUICK_FLAG: &str = "--quick";

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|a| a == QUICK_FLAG) {
                show_quick(app);
            } else if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        show_quick(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let shortcuts = interpres_core::Settings::default_path()
                .and_then(|p| interpres_core::Settings::load(&p))
                .map(|s| s.ui.shortcuts)
                .unwrap_or_default();
            register_quick_shortcut(app.handle(), &shortcuts.quick);
            if std::env::args().any(|a| a == QUICK_FLAG) {
                show_quick(app.handle());
            }
            app.manage(Arc::new(open_store()));
            start_document_queue(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            // The hidden quick window would otherwise keep the app alive.
            if window.label() == "main" && matches!(event, WindowEvent::CloseRequested { .. }) {
                window.app_handle().exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::translate,
            commands::languages,
            commands::key_status,
            commands::set_deepl_key,
            commands::delete_deepl_key,
            commands::deepl_usage,
            commands::get_settings,
            commands::save_settings,
            commands::documents_list,
            commands::documents_add,
            commands::documents_cancel,
            commands::documents_remove,
            commands::documents_reveal,
            commands::documents_open,
            commands::glossary_pairs,
            commands::glossary_terms,
            commands::glossary_upsert,
            commands::glossary_delete,
            commands::glossary_import,
            commands::glossary_export,
            commands::history_list,
            commands::cache_clear,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// `mod+alt+t` → `CommandOrControl+Alt+T` (Tauri accelerator syntax).
fn to_accelerator(combo: &str) -> String {
    combo
        .split('+')
        .map(|part| match part.to_lowercase().as_str() {
            "mod" => "CommandOrControl".to_string(),
            "ctrl" => "Control".to_string(),
            "alt" => "Alt".to_string(),
            "shift" => "Shift".to_string(),
            "meta" => "Super".to_string(),
            "arrowup" => "Up".to_string(),
            "arrowdown" => "Down".to_string(),
            "arrowleft" => "Left".to_string(),
            "arrowright" => "Right".to_string(),
            key if key.chars().count() == 1 => key.to_uppercase(),
            key => {
                let mut c = key.chars();
                c.next()
                    .map(|f| f.to_uppercase().chain(c).collect())
                    .unwrap_or_default()
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// Replaces the system-wide quick-translate shortcut. Global shortcuts are not available
/// everywhere (e.g. Wayland); there the user binds `interpres-desktop --quick` in the
/// compositor instead.
pub(crate) fn register_quick_shortcut(app: &AppHandle, combo: &str) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    if let Err(e) = gs.register(to_accelerator(combo).as_str()) {
        tracing::warn!(error = %e, combo, "global shortcut unavailable");
    }
}

fn show_quick(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("quick") {
        let _ = w.center();
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.emit("quick-open", ());
    }
}

/// Opens the database; falls back to an in-memory one so the app still starts.
fn open_store() -> Store {
    match Store::default_path().and_then(|p| Store::open(&p)) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "could not open the database; using a temporary one");
            Store::open_in_memory().expect("in-memory SQLite")
        }
    }
}

/// Engines for queued documents. Keys are read when a job starts.
fn document_engine(engine: EngineId) -> Result<Arc<dyn DocumentEngine>, EngineError> {
    match engine {
        EngineId::Deepl => {
            let key = secrets::get_key(EngineId::Deepl)
                .map_err(|e| EngineError::Service(e.to_string()))?
                .ok_or(EngineError::MissingKey)?;
            Ok(Arc::new(DeeplEngine::new(key)))
        }
        other => Err(EngineError::InvalidRequest(format!(
            "{other} cannot translate documents yet"
        ))),
    }
}

/// Starts the queue worker and forwards every change to the UI as `documents-changed`.
fn start_document_queue(app: &AppHandle) {
    let (queue, worker) = DocumentQueue::new(Box::new(document_engine));
    let mut rx = queue.subscribe();
    app.manage(queue);
    tauri::async_runtime::spawn(worker);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        while rx.changed().await.is_ok() {
            let jobs = rx.borrow_and_update().clone();
            let _ = app.emit("documents-changed", jobs);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::to_accelerator;

    #[test]
    fn converts_combos() {
        assert_eq!(to_accelerator("mod+alt+t"), "CommandOrControl+Alt+T");
        assert_eq!(
            to_accelerator("mod+shift+space"),
            "CommandOrControl+Shift+Space"
        );
        assert_eq!(to_accelerator("ctrl+f5"), "Control+F5");
        assert_eq!(to_accelerator("alt+arrowup"), "Alt+Up");
    }
}
