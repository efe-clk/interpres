//! Live translation for games and videos: either the text in a screen region (OCR)
//! or speech (system audio / microphone), shown as a caption. Model downloads,
//! region picking and the two loops live here.
//!
//! On Wayland compositors with layer-shell (Hyprland, Sway, KDE) the picker and the
//! caption are layer surfaces. Elsewhere (Windows, X11) they are Tauri windows.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use interpres_core::{
    EngineError, EngineId, Settings, Store, TranslateRequest, Translator, secrets,
};
use interpres_engine_deepl::DeeplEngine;
use interpres_ocr::{Ocr, RecModel, models};
use interpres_overlay::caption::{Style, VAlign};
use interpres_overlay::pipeline::OverlayError;
use interpres_overlay::{Pipeline, Placement, Region, bottom_area, caption_area, default_capture};
use interpres_speech::{self as speech, WhisperModel};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::oneshot;

pub const FRAME_EVENT: &str = "game-frame";
pub const STATE_EVENT: &str = "game-state";
const DOWNLOAD_EVENT: &str = "ocr-download";
const SPEECH_DOWNLOAD_EVENT: &str = "speech-download";
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
    /// Something the user should know that is not an error, e.g. `no_layer_shell`.
    notice: Option<&'static str>,
}

#[derive(Serialize)]
pub struct SpeechStatus {
    installed: bool,
    /// Download size in bytes.
    size: u64,
    /// The model `auto` resolves to.
    model: WhisperModel,
    /// GPU used for recognition; `None` runs on the CPU.
    gpu: Option<String>,
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

/// `small`, `turbo`, or `auto`: turbo with a GPU, small without.
fn speech_model(name: &str, gpu: bool) -> WhisperModel {
    match name {
        "small" => WhisperModel::Small,
        "turbo" => WhisperModel::Turbo,
        _ if gpu => WhisperModel::Turbo,
        _ => WhisperModel::Small,
    }
}

#[tauri::command]
pub async fn speech_status(model: String) -> CmdResult<SpeechStatus> {
    let root = models_root()?;
    // Looking for a GPU starts Vulkan: keep it off the UI thread.
    tauri::async_runtime::spawn_blocking(move || {
        let gpu = speech::recognize::gpu_name();
        let model = speech_model(&model, gpu.is_some());
        SpeechStatus {
            installed: speech::models::is_installed(&root, model),
            size: speech::models::download_size(model),
            model,
            gpu,
        }
    })
    .await
    .map_err(invalid)
}

#[tauri::command]
pub async fn speech_download(app: AppHandle, model: WhisperModel) -> CmdResult<()> {
    speech::models::download(
        &models_root()?,
        model,
        "https://huggingface.co",
        |done, total| {
            let _ = app.emit(SPEECH_DOWNLOAD_EVENT, Progress { done, total });
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
            // Full screen also where windows cannot be positioned (GNOME on Wayland),
            // so the page's coordinates are the monitor's.
            .fullscreen(true)
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
    game.runner
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|r| !r.thread.is_finished())
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
    if game_status(app.state::<Game>()) {
        stop(app);
    } else if let Err(e) = start(app) {
        let _ = app.emit(
            STATE_EVENT,
            GameState {
                running: false,
                error: Some(e),
                notice: None,
            },
        );
    }
}

fn start(app: &AppHandle) -> CmdResult<()> {
    let game = app.state::<Game>();
    let mut runner = game.runner.lock().unwrap();
    match runner.take() {
        // A loop that ended on its own (e.g. no audio device) does not block a restart.
        Some(r) if r.thread.is_finished() => {
            let _ = r.thread.join();
        }
        Some(r) => {
            *runner = Some(r);
            return Ok(());
        }
        None => {}
    }
    let settings = Settings::load(&Settings::default_path().map_err(invalid)?).map_err(invalid)?;
    if settings.game.input == "speech" {
        *runner = Some(start_speech(app, &settings)?);
        return Ok(());
    }
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
    let translator = translator(app)?;
    let target = target_lang(&settings);
    let ocr = Ocr::load(&root, model).map_err(invalid)?;
    let (placement, style) = caption_look(&settings.game.caption);
    let show_original = settings.game.caption.show_original;
    let mut pipeline = Pipeline::new(region, target, default_capture(), Box::new(ocr), translator);
    pipeline.source_lang = settings.game.source_lang;

    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = stop.clone();
    let handle = app.clone();
    let thread = std::thread::Builder::new()
        .name("interpres-overlay".into())
        .spawn(move || {
            run(
                handle,
                pipeline,
                region,
                placement,
                style,
                show_original,
                stop_flag,
            )
        })
        .map_err(invalid)?;
    *runner = Some(Runner { stop, thread });
    let _ = app.emit(
        STATE_EVENT,
        GameState {
            running: true,
            error: None,
            notice: None,
        },
    );
    Ok(())
}

fn translator(app: &AppHandle) -> CmdResult<Arc<Translator>> {
    let key = secrets::get_key(EngineId::Deepl)
        .map_err(|e| EngineError::Service(e.to_string()))?
        .ok_or(EngineError::MissingKey)?;
    let store = app.state::<Arc<Store>>().inner().clone();
    Ok(Arc::new(
        Translator::new(Arc::new(DeeplEngine::new(key)), None).with_store(store),
    ))
}

fn target_lang(settings: &Settings) -> String {
    settings
        .game
        .target_lang
        .clone()
        .or(settings.target_lang.clone())
        .unwrap_or_else(|| "EN-US".into())
}

/// Starts speech translation: captions go across the bottom of the screen the
/// pointer is on.
fn start_speech(app: &AppHandle, settings: &Settings) -> CmdResult<Runner> {
    let sp = &settings.game.speech;
    let root = models_root()?;
    let model = speech_model(&sp.model, speech::recognize::gpu_name().is_some());
    if !speech::models::is_installed(&root, model) {
        return Err(invalid("the speech model is not downloaded"));
    }
    let translator = translator(app)?;
    let target = target_lang(settings);
    // Glossary terms of the pair help Whisper spell names right.
    let prompt = match &sp.language {
        Some(lang) => app
            .state::<Arc<Store>>()
            .glossary_terms(lang, &target)
            .map_err(invalid)?
            .into_iter()
            .take(40)
            .map(|t| t.source)
            .collect::<Vec<_>>()
            .join(", "),
        None => String::new(),
    };
    let config = speech::ListenConfig {
        source: if sp.source == "microphone" {
            speech::Source::Microphone
        } else {
            speech::Source::System
        },
        model,
        language: sp.language.as_deref().map(speech::whisper_language),
        prompt,
        models_root: root,
    };
    let area = bottom_area(screen_under_cursor(app));
    let (_, mut style) = caption_look(&settings.game.caption);
    style.valign = VAlign::Bottom;
    let job = SpeechJob {
        translator,
        source_lang: sp.language.clone(),
        target,
        area,
        style,
        show_original: settings.game.caption.show_original,
    };
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = stop.clone();
    let handle = app.clone();
    let thread = std::thread::Builder::new()
        .name("interpres-speech".into())
        .spawn(move || run_speech(handle, config, job, stop_flag))
        .map_err(invalid)?;
    let _ = app.emit(
        STATE_EVENT,
        GameState {
            running: true,
            error: None,
            notice: Some("speech_loading"),
        },
    );
    Ok(Runner { stop, thread })
}

struct SpeechJob {
    translator: Arc<Translator>,
    source_lang: Option<String>,
    target: String,
    area: Region,
    style: Style,
    show_original: bool,
}

fn run_speech(app: AppHandle, config: speech::ListenConfig, job: SpeechJob, stop: Arc<AtomicBool>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let listener = speech::listen(config, move |e| {
        let _ = tx.send(e);
    });
    let sink = Sink::open(&app, job.area, Placement::Custom(job.area), job.style);
    let state = |running, error, notice| {
        let _ = app.emit(
            STATE_EVENT,
            GameState {
                running,
                error,
                notice,
            },
        );
    };
    while !stop.load(Ordering::SeqCst) {
        let event = match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(e) => e,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        match event {
            speech::Event::Ready => state(true, None, Some("speech_listening")),
            speech::Event::Failed(e) => {
                tracing::warn!(error = %e, "speech translation stopped");
                state(false, Some(invalid(e)), None);
                break;
            }
            speech::Event::Utterance(text) => {
                let req = TranslateRequest {
                    text: text.clone(),
                    source: job.source_lang.clone(),
                    target: job.target.clone(),
                    formality: Default::default(),
                    glossary: None,
                };
                match tauri::async_runtime::block_on(job.translator.translate(&req)) {
                    Ok(t) => {
                        sink.show(&text, &t.text, job.show_original);
                        let _ = app.emit(
                            FRAME_EVENT,
                            Frame {
                                source: text,
                                translation: t.text,
                            },
                        );
                    }
                    // Keep listening: the next sentence may translate fine.
                    Err(e) => state(true, Some(e), None),
                }
            }
        }
    }
    drop(listener);
    sink.close();
}

/// The monitor under the pointer (or the primary one), in logical coordinates.
fn screen_under_cursor(app: &AppHandle) -> Region {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    monitor_region(monitor.as_ref())
}

fn monitor_region(monitor: Option<&tauri::Monitor>) -> Region {
    monitor.map_or(
        Region {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
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
    )
}

/// Caption placement and style from the settings.
fn caption_look(c: &interpres_core::CaptionStyle) -> (Placement, Style) {
    let placement = match (c.placement.as_str(), c.area) {
        ("custom", Some((x, y, width, height))) => Placement::Custom(Region {
            x,
            y,
            width,
            height,
        }),
        ("above", _) => Placement::Above,
        _ => Placement::Below,
    };
    let style = Style {
        font_size: c.font_size.map(|s| s as f32),
        background: Style::parse_color(&c.background),
        opacity: c.opacity.min(100),
        // Keep the panel next to the subtitle it translates.
        valign: match placement {
            Placement::Below => VAlign::Top,
            Placement::Above => VAlign::Bottom,
            Placement::Custom(_) => VAlign::Center,
        },
    };
    (placement, style)
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
            notice: None,
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
    fn open(app: &AppHandle, region: Region, placement: Placement, style: Style) -> Sink {
        #[cfg(target_os = "linux")]
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            match interpres_overlay::wayland::WaylandCaption::show(region, placement, style) {
                Ok(c) => return Sink::Layer(c),
                Err(e) => {
                    tracing::info!(error = %e, "layer-shell caption unavailable, using a window");
                    // e.g. GNOME: the caption window cannot be placed next to the region.
                    let _ = app.emit(
                        STATE_EVENT,
                        GameState {
                            running: true,
                            error: None,
                            notice: Some("no_layer_shell"),
                        },
                    );
                }
            }
        }
        // The window page reads the style from the settings itself.
        let _ = style;
        if let Err(e) = open_caption_window(app, region, placement) {
            tracing::warn!(error = %e, "could not open the caption window");
        }
        Sink::Window(app.clone())
    }

    /// Shows a translation. The caption window reads the frame event itself.
    fn show(&self, original: &str, translation: &str, show_original: bool) {
        #[cfg(target_os = "linux")]
        if let Sink::Layer(c) = self {
            let original = if show_original {
                original.replace('\n', " ")
            } else {
                String::new()
            };
            c.set_text(original, translation);
        }
        let _ = (original, translation, show_original);
    }

    fn close(self) {
        if let Sink::Window(app) = self
            && let Some(w) = app.get_webview_window("caption")
        {
            let _ = w.close();
        }
    }
}

fn open_caption_window(app: &AppHandle, region: Region, placement: Placement) -> tauri::Result<()> {
    let monitor = app
        .monitor_from_point(region.x as f64, region.y as f64)?
        .or(app.primary_monitor()?);
    let screen = monitor_region(monitor.as_ref());
    let area = caption_area(region, screen, placement);
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

fn run(
    app: AppHandle,
    mut pipeline: Pipeline,
    region: Region,
    placement: Placement,
    style: Style,
    show_original: bool,
    stop: Arc<AtomicBool>,
) {
    let sink = Sink::open(&app, region, placement, style);
    while !stop.load(Ordering::SeqCst) {
        match tauri::async_runtime::block_on(pipeline.step()) {
            Ok(Some(frame)) => {
                sink.show(&frame.source, &frame.translation, show_original);
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
                        notice: None,
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
                        notice: None,
                    },
                );
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        std::thread::sleep(STEP_INTERVAL);
    }
    sink.close();
}
