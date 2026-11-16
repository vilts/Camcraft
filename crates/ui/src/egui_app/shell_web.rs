//! The browser shell (#31): eframe's web runner over a `<canvas>`, drawing
//! the same `shell::shell_layout` frame and the same `view_pass` 3D pass the
//! native window draws. wasm32 only - the native window is `shell_window`.
//!
//! eframe owns what a browser makes different: the DOM events, the canvas
//! size and pixel ratio, the animation-frame loop and the GPU start-up,
//! which it *awaits* (adapter, device, surface) where the native bring-up
//! blocks. The traced-stock eye is awaited too, after the first frame, and
//! joins the shell through a slot when the device has answered.
//!
//! Nothing here panics: a missing canvas, a refused adapter or surface and
//! a viewport pipeline the device will not build all end in a message on
//! the page (`#camcraft-status`) and in the console, never a blank tab.
//! Provenance: our own code, specced from the eframe 0.36 web docs
//! (`WebRunner::start`, `App::ui`, `Frame::wgpu_render_state`), the
//! wasm-bindgen guide and the egui-wgpu `Renderer` texture API.

use std::cell::RefCell;
use std::rc::Rc;

use eframe::wasm_bindgen::JsCast as _;
use eframe::web_sys;
use egui_dock::DockState;
use egui_wgpu::wgpu;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::state::State;
use crate::trace::eye::StockEye;
use crate::viewport::View3D;

use super::shell::{build_dock, shell_layout, Tab, VIEW3D_FORMAT};
use super::{gpu, icons, theme, view_pass, world};

/// The page element that carries start-up state and refusals.
const STATUS_ID: &str = "camcraft-status";

/// Where the awaited stock eye lands; the frame takes it when it is there.
type EyeSlot = Rc<RefCell<Option<Result<StockEye, String>>>>;

/// Start the shell on the canvas with this element id. Called from the
/// page's script once the module is instantiated; returns at once - the
/// start-up itself runs on the browser's event loop.
#[wasm_bindgen]
pub fn camcraft_start(canvas_id: String) {
    report("starting", "CamCraft is starting...");
    wasm_bindgen_futures::spawn_local(async move {
        match start(&canvas_id).await {
            Ok(()) => console_log("camcraft-web: runner started"),
            Err(why) => refuse(&why),
        }
    });
}

async fn start(canvas_id: &str) -> Result<(), String> {
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| "this page has no document to draw into".to_string())?;
    let canvas = document
        .get_element_by_id(canvas_id)
        .ok_or_else(|| format!("the page has no element with id \"{canvas_id}\""))?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| format!("the element \"{canvas_id}\" is not a <canvas>"))?;
    let runner = eframe::WebRunner::new();
    runner
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(|cc| match WebShell::new(cc) {
                Ok(shell) => Ok(Box::new(shell) as Box<dyn eframe::App>),
                Err(why) => Err(why.into()),
            }),
        )
        .await
        .map_err(|err| {
            let said = err.as_string().unwrap_or_else(|| format!("{err:?}"));
            format!("the browser gave no usable graphics adapter or surface ({said})")
        })
}

/// The shell could not start: say why on the page and in the console.
fn refuse(why: &str) {
    let line = format!(
        "CamCraft cannot start in this browser: {why}. It needs WebGPU or WebGL2 - try a current Chrome, Edge or Firefox."
    );
    web_sys::console::error_1(&line.as_str().into());
    report("refused", &line);
}

/// A plain note for the console, through the crate's diagnostics seam.
fn console_log(line: &str) {
    crate::diag::info(line);
}

/// Put `text` and a machine-readable `data-state` on the status element.
/// A page without one still gets the console line - never an error here.
fn report(state: &str, text: &str) {
    let Some(el) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id(STATUS_ID)) else {
        return;
    };
    el.set_text_content(Some(text));
    if el.set_attribute("data-state", state).is_err() {
        console_log("camcraft-web: the status element refused its state attribute");
    }
}

/// Everything the browser frame owns. The device, queue and egui renderer
/// are eframe's (`Frame::wgpu_render_state`); the shell keeps only what it
/// draws with them.
struct WebShell {
    dock_state: DockState<Tab>,
    view: View3D,
    stock_eye: Option<StockEye>,
    eye_slot: EyeSlot,
    view_tex: egui::TextureId,
    icons: Option<icons::IconSet>,
    viewport_points: [f32; 2],
    left_press: Option<[f32; 2]>,
    scene_digest: u64,
    title: String,
    first_frame_said: bool,
    state: State,
}

impl WebShell {
    fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, String> {
        let rs = cc
            .wgpu_render_state
            .as_ref()
            .ok_or_else(|| "the web runner started without a wgpu device".to_string())?;
        let mut state = State::new();
        let gpu_line = gpu::describe(&rs.adapter).log_line();
        console_log(&gpu_line);
        state.gpu_line = Some(gpu_line);

        let viewport_points = [600.0, 400.0];
        // `target_extent` holds the pixel ratio to 0.1..=8 like every later
        // frame: a ratio the browser reports as zero or absurd still gives a
        // usable first target.
        let (vw, vh) = view_pass::target_extent(viewport_points, f64::from(cc.egui_ctx.pixels_per_point()));
        let mut view = View3D::new(&rs.device, VIEW3D_FORMAT, vw, vh);
        let scene = world::collect(&state);
        let scene_digest = scene.digest();
        view.set_document(&rs.device, &rs.queue, &scene.parts);
        view.set_overlay(&rs.device, &rs.queue, &scene.overlay);
        let view_tex =
            rs.renderer
                .write()
                .register_native_texture(&rs.device, view.color_view(), wgpu::FilterMode::Linear);
        theme::apply(&cc.egui_ctx);
        // A file the async picker reads wakes the next frame to land it.
        crate::file_pick::set_repaint(&cc.egui_ctx);

        // The traced-stock eye: awaited on the event loop, not blocked on.
        // Until it lands (or if the device refuses it) the sheet stands.
        let eye_slot: EyeSlot = Rc::new(RefCell::new(None));
        {
            let (slot, device, ctx) = (Rc::clone(&eye_slot), rs.device.clone(), cc.egui_ctx.clone());
            wasm_bindgen_futures::spawn_local(async move {
                let built = StockEye::new_async(&device, VIEW3D_FORMAT).await;
                if let Ok(mut s) = slot.try_borrow_mut() {
                    *s = Some(built);
                }
                ctx.request_repaint();
            });
        }

        Ok(Self {
            dock_state: build_dock(),
            view,
            stock_eye: None,
            eye_slot,
            view_tex,
            icons: None,
            viewport_points,
            left_press: None,
            scene_digest,
            title: String::new(),
            first_frame_said: false,
            state,
        })
    }

    /// The awaited eye, once: taken into the shell or refused by name.
    fn take_eye(&mut self) {
        let landed = self.eye_slot.try_borrow_mut().ok().and_then(|mut s| s.take());
        match landed {
            Some(Ok(eye)) => self.stock_eye = Some(eye),
            Some(Err(err)) => diag_warn!("stock eye: {err} - the deviate sheet stands in"),
            None => {}
        }
    }

    fn sync_title(&mut self) {
        let title = self.state.window_title();
        if title != self.title {
            if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                document.set_title(&title);
            }
            self.title = title;
        }
    }
}

impl eframe::App for WebShell {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // Commands the panels queued last frame land before the scene is
        // collected, so this frame draws what the user's click did.
        self.state.drain_actions();
        self.sync_title();
        self.take_eye();
        let Some(rs) = frame.wgpu_render_state() else {
            ui.label("CamCraft: the graphics device is gone - reload the page.");
            return;
        };
        let (device, queue) = (&rs.device, &rs.queue);
        let scene = world::collect(&self.state);
        view_pass::sync_scene(&mut self.view, device, queue, &scene, &mut self.scene_digest);

        // Size the 3D target from last frame's layout, like the native shell.
        let scale = f64::from(ui.ctx().pixels_per_point()).clamp(0.1, 8.0);
        let want = view_pass::target_extent(self.viewport_points, scale);
        if view_pass::fit_target(&mut self.view, device, want) {
            // Rebind the SAME egui texture id to the recreated view.
            rs.renderer.write().update_egui_texture_from_wgpu_texture(
                device,
                self.view.color_view(),
                wgpu::FilterMode::Linear,
                self.view_tex,
            );
        }
        if self.icons.is_none() {
            self.icons = Some(icons::IconSet::load(ui.ctx()));
        }

        shell_layout(
            ui,
            &mut self.state,
            &mut self.dock_state,
            self.view_tex,
            self.icons.as_ref(),
            &mut self.viewport_points,
            &mut self.left_press,
        );

        // The 3D pass is submitted here, before eframe paints egui over the
        // canvas, so the viewport texture this frame shows is this frame's.
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("egui-shell-web-frame"),
        });
        view_pass::encode(&mut self.state, &mut self.view, &mut self.stock_eye, device, queue, &mut encoder);
        queue.submit([encoder.finish()]);

        // eframe keeps egui's own repaint timer; the document adds its own
        // (queued commands, an import, a playing simulation).
        if let Some(delay) = view_pass::state_redraw_delay(&self.state, std::time::Duration::MAX, false) {
            ui.ctx().request_repaint_after(delay);
        }
        // #31: a click that asked for a file chooser runs its action now,
        // still inside the browser's pointerup handler (eframe runs the
        // frame that sees a release there), where Safari lets it open.
        // Last in the frame, after the 3D pass, like the next frame's drain.
        let released = ui.input(|i| i.pointer.any_released());
        if self.state.run_click_actions(released) {
            ui.ctx().request_repaint();
        }
        // A tab has no window to close: Quit is answered, not obeyed.
        if std::mem::take(&mut self.state.request_quit) {
            self.state.status = "Quit: close this browser tab to leave CamCraft".into();
        }
        if !self.first_frame_said {
            self.first_frame_said = true;
            let (w, h) = self.view.extent();
            console_log(&format!("camcraft-web: first frame drawn (viewport target {w}x{h})"));
            report("running", "CamCraft: first frame drawn");
        }
    }
}
