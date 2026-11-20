//! Wayland overlay surfaces (wlr-layer-shell: Hyprland, Sway, KDE): the region
//! picker and the caption overlay. Layer surfaces sit above fullscreen games and
//! are positioned by the compositor, which ordinary windows cannot be on Wayland.

use std::num::NonZeroU32;
use std::thread::JoinHandle;
use std::time::Duration;

use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region as WlRegion};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::{EventLoop, channel};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::registry_handlers;
use smithay_client_toolkit::seat::keyboard::{
    KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers,
};
use smithay_client_toolkit::seat::pointer::{
    BTN_LEFT, PointerEvent, PointerEventKind, PointerHandler,
};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, QueueHandle};

use crate::caption::Painter;
use crate::{Region, caption_area};

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ---------------------------------------------------------------- region picker

/// Dims the screen and lets the user drag a rectangle. Returns `None` when
/// cancelled (Esc or right click). Blocks until done.
pub fn pick_region() -> Result<Option<Region>, String> {
    let conn = Connection::connect_to_env().map_err(err)?;
    let (globals, mut queue) = registry_queue_init(&conn).map_err(err)?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).map_err(err)?;
    let layer_shell = LayerShell::bind(&globals, &qh).map_err(err)?;
    let shm = Shm::bind(&globals, &qh).map_err(err)?;

    let surface = compositor.create_surface(&qh);
    let layer = layer_shell.create_layer_surface(
        &qh,
        surface,
        Layer::Overlay,
        Some("interpres-picker"),
        None,
    );
    layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    layer.set_exclusive_zone(-1);
    layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
    layer.set_size(0, 0);
    layer.commit();

    let mut picker = Picker {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        shm,
        pool: SlotPool::new(1920 * 1080 * 4, &Shm::bind(&globals, &qh).map_err(err)?)
            .map_err(err)?,
        layer,
        width: 0,
        height: 0,
        origin: (0, 0),
        keyboard: None,
        pointer: None,
        start: None,
        current: None,
        result: None,
    };
    loop {
        queue.blocking_dispatch(&mut picker).map_err(err)?;
        if let Some(result) = picker.result.take() {
            return Ok(result);
        }
    }
}

struct Picker {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,
    pool: SlotPool,
    layer: LayerSurface,
    width: u32,
    height: u32,
    /// Logical position of the output the picker is on.
    origin: (i32, i32),
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    start: Option<(f64, f64)>,
    current: Option<(f64, f64)>,
    result: Option<Option<Region>>,
}

impl Picker {
    fn selection(&self) -> Option<(u32, u32, u32, u32)> {
        let (s, c) = (self.start?, self.current?);
        let (x0, y0) = (s.0.min(c.0).max(0.0) as u32, s.1.min(c.1).max(0.0) as u32);
        let (x1, y1) = (s.0.max(c.0) as u32, s.1.max(c.1) as u32);
        Some((x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0)))
    }

    fn draw(&mut self) {
        let (w, h) = (self.width, self.height);
        if w == 0 || h == 0 {
            return;
        }
        let sel = self.selection();
        let Ok((buffer, canvas)) =
            self.pool
                .create_buffer(w as i32, h as i32, w as i32 * 4, wl_shm::Format::Argb8888)
        else {
            return;
        };
        // Premultiplied BGRA: dim everything, keep the selection clear, outline it.
        let dim = [0u8, 0, 0, 110];
        let border = [0xff, 0x8c, 0x7c, 0xff];
        for (i, px) in canvas.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let (x, y) = ((i as u32) % w, (i as u32) / w);
            let value = match sel {
                Some((sx, sy, sw, sh)) if x >= sx && x < sx + sw && y >= sy && y < sy + sh => {
                    let edge = x < sx + 2 || x + 2 >= sx + sw || y < sy + 2 || y + 2 >= sy + sh;
                    if edge { border } else { [0, 0, 0, 0] }
                }
                _ => dim,
            };
            px.copy_from_slice(&value);
        }
        let surface = self.layer.wl_surface();
        surface.damage_buffer(0, 0, w as i32, h as i32);
        if buffer.attach_to(surface).is_ok() {
            self.layer.commit();
        }
    }
}

impl CompositorHandler for Picker {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        if let Some(pos) = self
            .output_state
            .info(output)
            .and_then(|i| i.logical_position)
        {
            self.origin = pos;
        }
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for Picker {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for Picker {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.result = Some(None);
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        self.width = NonZeroU32::new(configure.new_size.0).map_or(1, NonZeroU32::get);
        self.height = NonZeroU32::new(configure.new_size.1).map_or(1, NonZeroU32::get);
        self.draw();
    }
}

impl SeatHandler for Picker {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seat_state.get_keyboard(qh, &seat, None).ok();
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for Picker {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }
    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        if event.keysym == Keysym::Escape {
            self.result = Some(None);
        }
    }
    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
    }
}

impl PointerHandler for Picker {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if &event.surface != self.layer.wl_surface() {
                continue;
            }
            match event.kind {
                PointerEventKind::Press { button, .. } if button == BTN_LEFT => {
                    self.start = Some(event.position);
                    self.current = Some(event.position);
                }
                PointerEventKind::Press { .. } => self.result = Some(None),
                PointerEventKind::Motion { .. } if self.start.is_some() => {
                    self.current = Some(event.position);
                    self.draw();
                }
                PointerEventKind::Release { button, .. } if button == BTN_LEFT => {
                    self.current = Some(event.position);
                    match self.selection() {
                        // Ignore clicks without a real drag.
                        Some((x, y, w, h)) if w >= 16 && h >= 8 => {
                            self.result = Some(Some(Region {
                                x: self.origin.0 + x as i32,
                                y: self.origin.1 + y as i32,
                                width: w,
                                height: h,
                            }));
                        }
                        _ => {
                            self.start = None;
                            self.current = None;
                            self.draw();
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

impl ShmHandler for Picker {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Picker {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

smithay_client_toolkit::delegate_registry!(Picker);
smithay_client_toolkit::delegate_dispatch2!(Picker);

// ---------------------------------------------------------------- caption overlay

enum Msg {
    Text(String),
    Stop,
}

/// A click-through caption next to the captured `region` (see [`caption_area`]),
/// updated with [`WaylandCaption::set_text`].
/// Closes when dropped.
pub struct WaylandCaption {
    tx: channel::Sender<Msg>,
    thread: Option<JoinHandle<()>>,
}

impl WaylandCaption {
    pub fn show(region: Region) -> Result<Self, String> {
        let (tx, rx) = channel::channel::<Msg>();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let thread = std::thread::Builder::new()
            .name("interpres-caption".into())
            .spawn(move || {
                if let Err(e) = run_caption(region, rx, &ready_tx) {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .map_err(err)?;
        ready_rx.recv().map_err(err)??;
        Ok(Self {
            tx,
            thread: Some(thread),
        })
    }

    pub fn set_text(&self, text: impl Into<String>) {
        let _ = self.tx.send(Msg::Text(text.into()));
    }
}

impl Drop for WaylandCaption {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Stop);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn run_caption(
    region: Region,
    rx: channel::Channel<Msg>,
    ready: &std::sync::mpsc::Sender<Result<(), String>>,
) -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(err)?;
    let (globals, mut queue) = registry_queue_init(&conn).map_err(err)?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).map_err(err)?;
    let layer_shell = LayerShell::bind(&globals, &qh).map_err(err)?;
    let shm = Shm::bind(&globals, &qh).map_err(err)?;

    let mut state = Caption {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        pool: SlotPool::new((region.width * region.height * 4) as usize, &shm).map_err(err)?,
        shm,
        layer: None,
        region,
        scale: 1,
        text: String::new(),
        painter: Painter::new(),
        configured: false,
        exit: false,
    };
    // Learn the outputs, so the caption goes on the one that holds the region.
    queue.roundtrip(&mut state).map_err(err)?;
    let output = state.output_state.outputs().find(|o| {
        state
            .output_state
            .info(o)
            .is_some_and(|i| match (i.logical_position, i.logical_size) {
                (Some((ox, oy)), Some((ow, oh))) => {
                    region.x >= ox && region.y >= oy && region.x < ox + ow && region.y < oy + oh
                }
                _ => false,
            })
    });
    let info = output.as_ref().and_then(|o| state.output_state.info(o));
    let origin = info
        .as_ref()
        .and_then(|i| i.logical_position)
        .unwrap_or((0, 0));
    let size = info
        .as_ref()
        .and_then(|i| i.logical_size)
        .unwrap_or((i32::MAX / 2, i32::MAX / 2));
    let screen = Region {
        x: origin.0,
        y: origin.1,
        width: size.0 as u32,
        height: size.1 as u32,
    };
    let region = caption_area(region, screen);
    state.region = region;

    let surface = compositor.create_surface(&qh);
    // Empty input region: clicks go through to the game.
    let empty = WlRegion::new(&compositor).map_err(err)?;
    surface.set_input_region(Some(empty.wl_region()));
    let layer = layer_shell.create_layer_surface(
        &qh,
        surface,
        Layer::Overlay,
        Some("interpres-caption"),
        output.as_ref(),
    );
    layer.set_anchor(Anchor::TOP | Anchor::LEFT);
    layer.set_margin(region.y - origin.1, 0, 0, region.x - origin.0);
    layer.set_exclusive_zone(-1);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.set_size(region.width, region.height);
    layer.commit();
    state.layer = Some(layer);

    let mut event_loop: EventLoop<Caption> = EventLoop::try_new().map_err(err)?;
    WaylandSource::new(conn.clone(), queue)
        .insert(event_loop.handle())
        .map_err(err)?;
    event_loop
        .handle()
        .insert_source(rx, |event, _, state| match event {
            channel::Event::Msg(Msg::Text(text)) => {
                state.text = text;
                state.draw();
            }
            channel::Event::Msg(Msg::Stop) | channel::Event::Closed => state.exit = true,
        })
        .map_err(err)?;
    let _ = ready.send(Ok(()));
    while !state.exit {
        event_loop
            .dispatch(Duration::from_millis(200), &mut state)
            .map_err(err)?;
    }
    Ok(())
}

struct Caption {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    pool: SlotPool,
    layer: Option<LayerSurface>,
    region: Region,
    scale: i32,
    text: String,
    painter: Painter,
    configured: bool,
    exit: bool,
}

impl Caption {
    fn draw(&mut self) {
        let Some(layer) = &self.layer else {
            return;
        };
        if !self.configured {
            return;
        }
        let scale = self.scale.max(1) as u32;
        let (w, h) = (self.region.width * scale, self.region.height * scale);
        let pixels = self.painter.render(&self.text, w, h, scale as f32);
        let Ok((buffer, canvas)) =
            self.pool
                .create_buffer(w as i32, h as i32, w as i32 * 4, wl_shm::Format::Argb8888)
        else {
            return;
        };
        // The pool may hand out a slightly larger slice (alignment).
        canvas[..pixels.len()].copy_from_slice(&pixels);
        let surface = layer.wl_surface();
        surface.set_buffer_scale(scale as i32);
        surface.damage_buffer(0, 0, w as i32, h as i32);
        if buffer.attach_to(surface).is_ok() {
            layer.commit();
        }
    }
}

impl CompositorHandler for Caption {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        factor: i32,
    ) {
        // Render at the output's integer scale so the text stays sharp.
        if factor != self.scale {
            self.scale = factor;
            self.draw();
        }
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for Caption {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for Caption {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.exit = true;
    }
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        _: LayerSurfaceConfigure,
        _: u32,
    ) {
        self.configured = true;
        self.draw();
    }
}

impl ShmHandler for Caption {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Caption {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}

smithay_client_toolkit::delegate_registry!(Caption);
smithay_client_toolkit::delegate_dispatch2!(Caption);
