//! Tauri shell. Thin layer: every command delegates to interpres-core and the engines.

mod commands;

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
            commands::claude_code_found,
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
