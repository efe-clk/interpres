//! Game screen translation: OCR model download, region picking and the overlay loop.
//!
//! On Wayland compositors with layer-shell (Hyprland, Sway, KDE) the picker and the
//! caption are layer surfaces. Elsewhere (Windows, X11) they are Tauri windows.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use interpres_core::{EngineError, EngineId, Settings, Store, Translator, secrets};
use interpres_engine_deepl::DeeplEngine;
use interpres_ocr::{Ocr, RecModel, models};
use interpres_overlay::pipeline::OverlayError;
use interpres_overlay::{Pipeline, Region, caption_area, default_capture};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::oneshot;

pub const FRAME_EVENT: &str = "game-frame";
pub const STATE_EVENT: &str = "game-state";
const DOWNLOAD_EVENT: &str = "ocr-download";
const STEP_INTERVAL: Duration = Duration::from_millis(300);

type CmdResult<T> = Result<T, EngineError>;

fn invalid(e: impl std::fmt::Display) -> EngineError {
    EngineError::InvalidRequest(e.to_string())
}

pub fn rec_model(name: &str) -> RecModel {
    match name {
        "korean" => RecModel::Korean,
        "latin" => RecModel::Latin,
        _ => RecModel::General,
    }
}

fn models_root() -> CmdResult<std::path::PathBuf> {
    models::default_root().ok_or_else(|| EngineError::Service("no data directory".into()))
}

/// Overlay state shared by the commands.
#[derive(Default)]
pub struct Game {
    runner: Mutex<Option<Runner>>,
    /// Resolves the pending window-based region picker.
    picked: Mutex<Option<oneshot::Sender<Option<Region>>>>,
}

struct Runner {
    stop: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<()>,
}

#[derive(Clone, Serialize)]
pub struct Frame {
    source: String,
    translation: String,
}

#[derive(Clone, Serialize)]
pub struct GameState {
    running: bool,
    error: Option<EngineError>,
}

#[derive(Serialize)]
pub struct OcrStatus {
    installed: bool,
    /// Download size in bytes.
    size: u64,
}

#[derive(Clone, Serialize)]
struct Progress {
    done: u64,
    total: u64,
}

#[tauri::command]
pub fn ocr_status(model: String) -> CmdResult<OcrStatus> {
    let model = rec_model(&model);
    Ok(OcrStatus {
        installed: models::is_installed(&models_root()?, model),
        size: models::download_size(model),
    })
}

#[tauri::command]
pub async fn ocr_download(app: AppHandle, model: String) -> CmdResult<()> {
    models::download(
        &models_root()?,
        rec_model(&model),
        "https://huggingface.co",
        |done, total| {
            let _ = app.emit(DOWNLOAD_EVENT, Progress { done, total });
        },
    )
    .await
    .map_err(|e| EngineError::Network(e.to_string()))
}

/// Lets the user drag a rectangle on screen. `None` when cancelled.
#[tauri::command]
pub async fn game_pick_region(app: AppHandle) -> CmdResult<Option<(i32, i32, u32, u32)>> {
    #[cfg(target_os = "linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        match tauri::async_runtime::spawn_blocking(interpres_overlay::wayland::pick_region).await {
            Ok(Ok(r)) => return Ok(r.map(|r| (r.x, r.y, r.width, r.height))),
            Ok(Err(e)) => {
                tracing::info!(error = %e, "layer-shell picker unavailable, using a window")
            }
            Err(e) => return Err(invalid(e)),
        }
    }
    pick_with_window(&app).await
}

/// Window-based picker: a transparent full-screen window on the monitor under the
/// cursor; its page calls `game_region_picked`.
async fn pick_with_window(app: &AppHandle) -> CmdResult<Option<(i32, i32, u32, u32)>> {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())
        .ok_or_else(|| invalid("no monitor"))?;
    let scale = monitor.scale_factor();
    let pos = monitor.position().to_logical::<f64>(scale);
    let size = monitor.size().to_logical::<f64>(scale);

    let (tx, rx) = oneshot::channel();
    *app.state::<Game>().picked.lock().unwrap() = Some(tx);
    let window =
        WebviewWindowBuilder::new(app, "picker", WebviewUrl::App("index.html#picker".into()))
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .position(pos.x, pos.y)
            .inner_size(size.width, size.height)
            .build()
            .map_err(invalid)?;
    let _ = window.set_focus();
    let picked = rx.await.unwrap_or(None);
    let _ = window.close();
    Ok(picked.map(|r| (pos.x as i32 + r.x, pos.y as i32 + r.y, r.width, r.height)))
}

/// Called by the picker page with a window-relative region, or `None` to cancel.
#[tauri::command]
pub fn game_region_picked(game: tauri::State<'_, Game>, region: Option<(i32, i32, u32, u32)>) {
    if let Some(tx) = game.picked.lock().unwrap().take() {
        let _ = tx.send(region.map(|(x, y, width, height)| Region {
            x,
            y,
            width,
            height,
        }));
    }
}

#[tauri::command]
pub fn game_status(game: tauri::State<'_, Game>) -> bool {
    game.runner.lock().unwrap().is_some()
}

#[tauri::command]
pub fn game_start(app: AppHandle) -> CmdResult<()> {
    start(&app)
}

#[tauri::command]
pub fn game_stop(app: AppHandle) {
    stop(&app);
}

/// Starts the overlay if it is stopped, stops it otherwise.
pub fn toggle(app: &AppHandle) {
    if app.state::<Game>().runner.lock().unwrap().is_some() {
        stop(app);
    } else if let Err(e) = start(app) {
        let _ = app.emit(
            STATE_EVENT,
            GameState {
                running: false,
                error: Some(e),
            },
        );
    }
}

fn start(app: &AppHandle) -> CmdResult<()> {
    let game = app.state::<Game>();
    let mut runner = game.runner.lock().unwrap();
    if runner.is_some() {
        return Ok(());
    }
    let settings = Settings::load(&Settings::default_path().map_err(invalid)?).map_err(invalid)?;
    let (x, y, width, height) = settings
        .game
        .region
        .ok_or_else(|| invalid("no screen region selected"))?;
    let region = Region {
        x,
        y,
        width,
        height,
    };
    let model = rec_model(&settings.game.ocr_model);
    let root = models_root()?;
    if !models::is_installed(&root, model) {
        return Err(invalid("the OCR model is not downloaded"));
    }
    let key = secrets::get_key(EngineId::Deepl)
        .map_err(|e| EngineError::Service(e.to_string()))?
        .ok_or(EngineError::MissingKey)?;
    let store = app.state::<Arc<Store>>().inner().clone();
    let translator =
        Arc::new(Translator::new(Arc::new(DeeplEngine::new(key)), None).with_store(store));
    let target = settings
        .game
        .target_lang
        .or(settings.target_lang)
        .unwrap_or_else(|| "EN-US".into());
    let ocr = Ocr::load(&root, model).map_err(invalid)?;
    let mut pipeline = Pipeline::new(region, target, default_capture(), Box::new(ocr), translator);
    pipeline.source_lang = settings.game.source_lang;

    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = stop.clone();
    let handle = app.clone();
    let thread = std::thread::Builder::new()
        .name("interpres-overlay".into())
        .spawn(move || run(handle, pipeline, region, stop_flag))
        .map_err(invalid)?;
    *runner = Some(Runner { stop, thread });
    let _ = app.emit(
        STATE_EVENT,
        GameState {
            running: true,
            error: None,
        },
    );
    Ok(())
}

fn stop(app: &AppHandle) {
    let runner = app.state::<Game>().runner.lock().unwrap().take();
    if let Some(r) = runner {
        r.stop.store(true, Ordering::SeqCst);
        let _ = r.thread.join();
    }
    let _ = app.emit(
        STATE_EVENT,
        GameState {
            running: false,
            error: None,
        },
    );
}

/// Where translations are shown.
enum Sink {
    #[cfg(target_os = "linux")]
    Layer(interpres_overlay::wayland::WaylandCaption),
    Window(AppHandle),
}

impl Sink {
    fn open(app: &AppHandle, region: Region) -> Sink {
        #[cfg(target_os = "linux")]
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            match interpres_overlay::wayland::WaylandCaption::show(region) {
                Ok(c) => return Sink::Layer(c),
                Err(e) => {
                    tracing::info!(error = %e, "layer-shell caption unavailable, using a window")
                }
            }
        }
        if let Err(e) = open_caption_window(app, region) {
            tracing::warn!(error = %e, "could not open the caption window");
        }
        Sink::Window(app.clone())
    }

    fn close(self) {
        if let Sink::Window(app) = self
            && let Some(w) = app.get_webview_window("caption")
        {
            let _ = w.close();
        }
    }
}

fn open_caption_window(app: &AppHandle, region: Region) -> tauri::Result<()> {
    let monitor = app
        .monitor_from_point(region.x as f64, region.y as f64)?
        .or(app.primary_monitor()?);
    let screen = monitor.map_or(
        Region {
            x: 0,
            y: 0,
            width: u32::MAX / 2,
            height: u32::MAX / 2,
        },
        |m| {
            let s = m.scale_factor();
            let p = m.position().to_logical::<f64>(s);
            let z = m.size().to_logical::<f64>(s);
            Region {
                x: p.x as i32,
                y: p.y as i32,
                width: z.width as u32,
                height: z.height as u32,
            }
        },
    );
    let area = caption_area(region, screen);
    let window =
        WebviewWindowBuilder::new(app, "caption", WebviewUrl::App("index.html#caption".into()))
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .focused(false)
            .position(area.x as f64, area.y as f64)
            .inner_size(area.width as f64, area.height as f64)
            .build()?;
    // Clicks go through to the game.
    window.set_ignore_cursor_events(true)?;
    Ok(())
}

fn run(app: AppHandle, mut pipeline: Pipeline, region: Region, stop: Arc<AtomicBool>) {
    let sink = Sink::open(&app, region);
    while !stop.load(Ordering::SeqCst) {
        match tauri::async_runtime::block_on(pipeline.step()) {
            Ok(Some(frame)) => {
                #[cfg(target_os = "linux")]
                if let Sink::Layer(c) = &sink {
                    c.set_text(frame.translation.clone());
                }
                let _ = app.emit(
                    FRAME_EVENT,
                    Frame {
                        source: frame.source,
                        translation: frame.translation,
                    },
                );
            }
            Ok(None) => {}
            Err(OverlayError::Translate(e)) => {
                // Keep running: the next subtitle may translate fine (e.g. after a network blip).
                let _ = app.emit(
                    STATE_EVENT,
                    GameState {
                        running: true,
                        error: Some(e),
                    },
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, "overlay step failed");
                let _ = app.emit(
                    STATE_EVENT,
                    GameState {
                        running: true,
                        error: Some(EngineError::Service(e.to_string())),
                    },
                );
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        std::thread::sleep(STEP_INTERVAL);
    }
    sink.close();
}
