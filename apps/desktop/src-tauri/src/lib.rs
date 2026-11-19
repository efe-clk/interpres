//! Tauri shell. Thin layer: every command delegates to interpres-core and the engines.

mod commands;
mod documents;

use std::str::FromStr;
use std::sync::{Arc, Mutex};

use interpres_core::{DocumentEngine, DocumentQueue, EngineError, EngineId, Store, secrets};
use interpres_engine_deepl::DeeplEngine;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Command-line flag that opens the quick-translate window (for Wayland compositor binds).
const QUICK_FLAG: &str = "--quick";
/// Command-line flag that shows the main window, or sends it to the tray when it is
/// already in front.
const TOGGLE_FLAG: &str = "--toggle";

/// The registered show/hide shortcut, to tell it apart from the quick-translate one.
#[derive(Default)]
struct ToggleShortcut(Mutex<Option<Shortcut>>);

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|a| a == QUICK_FLAG) {
                show_quick(app);
            } else if args.iter().any(|a| a == TOGGLE_FLAG) {
                toggle_main(app);
            } else {
                show_main(app);
            }
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state != ShortcutState::Pressed {
                        return;
                    }
                    let toggle = app.state::<ToggleShortcut>();
                    let is_toggle = toggle
                        .0
                        .lock()
                        .unwrap()
                        .as_ref()
                        .is_some_and(|t| t.id() == shortcut.id());
                    if is_toggle {
                        toggle_main(app);
                    } else {
                        show_quick(app);
                    }
                })
                .build(),
        )
        .manage(ToggleShortcut::default())
        .setup(|app| {
            let settings = interpres_core::Settings::default_path()
                .and_then(|p| interpres_core::Settings::load(&p))
                .unwrap_or_default();
            register_global_shortcuts(app.handle(), &settings.ui.shortcuts);
            build_tray(app.handle(), &settings.ui.language)?;
            // Started for quick translate only: keep the main window hidden and stay
            // in the background so the next shortcut press opens instantly.
            if std::env::args().any(|a| a == QUICK_FLAG) {
                show_quick(app.handle());
            } else if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
            }
            let store = Arc::new(open_store());
            app.manage(store.clone());
            start_document_queue(app.handle(), store);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the main window sends it to the tray; "Quit" in the tray exits.
            if window.label() == "main"
                && let WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                let _ = window.hide();
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

/// Replaces the system-wide shortcuts (quick translate, show/hide). Global shortcuts
/// are not available everywhere (e.g. Wayland); there the user binds
/// `interpres-desktop --quick` / `--toggle` in the compositor instead.
pub(crate) fn register_global_shortcuts(app: &AppHandle, shortcuts: &interpres_core::Shortcuts) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    if let Err(e) = gs.register(to_accelerator(&shortcuts.quick).as_str()) {
        tracing::warn!(error = %e, combo = %shortcuts.quick, "global shortcut unavailable");
    }
    let toggle = Shortcut::from_str(&to_accelerator(&shortcuts.toggle)).ok();
    if let Some(t) = &toggle
        && let Err(e) = gs.register(*t)
    {
        tracing::warn!(error = %e, combo = %shortcuts.toggle, "global shortcut unavailable");
    }
    *app.state::<ToggleShortcut>().0.lock().unwrap() = toggle;
}

/// Tray icon with show/hide, quick translate and quit. Labels follow the UI language.
fn build_tray(app: &AppHandle, language: &str) -> tauri::Result<()> {
    let (show, quick, quit) = if language == "en" {
        ("Show / hide", "Quick translate", "Quit")
    } else {
        ("Göster / gizle", "Hızlı çeviri", "Çık")
    };
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "toggle", show, true, None::<&str>)?,
            &MenuItem::with_id(app, "quick", quick, true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", quit, true, None::<&str>)?,
        ],
    )?;
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Interpres")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "toggle" => toggle_main(app),
            "quick" => show_quick(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // Left click, where the platform reports it (not on most Linux trays).
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Hidden or behind other windows → bring to front; in front → send to the tray.
fn toggle_main(app: &AppHandle) {
    let Some(w) = app.get_webview_window("main") else {
        return;
    };
    let in_front = w.is_visible().unwrap_or(false)
        && !w.is_minimized().unwrap_or(false)
        && w.is_focused().unwrap_or(false);
    if in_front {
        let _ = w.hide();
    } else {
        show_main(app);
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

/// Starts the local queue, then switches to a running daemon's queue if there is one.
/// Every change is forwarded to the UI as `documents-changed`.
fn start_document_queue(app: &AppHandle, store: Arc<Store>) {
    let (queue, worker) = DocumentQueue::new(Box::new(document_engine));
    let mut rx = queue.subscribe();
    let docs = Arc::new(documents::Documents::new(queue, store, app.clone()));
    app.manage(docs.clone());
    tauri::async_runtime::spawn(worker);
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        while rx.changed().await.is_ok() {
            let jobs = rx.borrow_and_update().clone();
            let _ = handle.emit(documents::EVENT, jobs);
        }
    });
    tauri::async_runtime::spawn(async move {
        if let Some(client) = interpres_daemon::try_connect().await {
            docs.attach(client).await;
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
