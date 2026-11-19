//! Tauri shell. Thin layer: every command delegates to interpres-core and the engines.

mod commands;

use std::sync::Arc;

use interpres_core::{DocumentEngine, DocumentQueue, EngineError, EngineId, Store, secrets};
use interpres_engine_deepl::DeeplEngine;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Global shortcut that opens the quick-translate window.
const QUICK_SHORTCUT: &str = "CommandOrControl+Alt+T";
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
            // Global shortcuts are not available on every platform (e.g. Wayland);
            // there the user binds `interpres-desktop --quick` in the compositor.
            if let Err(e) = app.global_shortcut().register(QUICK_SHORTCUT) {
                tracing::warn!(error = %e, "global shortcut unavailable");
            }
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
