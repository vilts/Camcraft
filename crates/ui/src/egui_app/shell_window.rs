//! The native windowed shell (#31 split it out of shell.rs): winit's event
//! loop, the window, the surface and the GPU bring-up, plus the native
//! refusal dialog. Native only - a browser has no winit event loop of this
//! kind and no synchronous message box, so the wasm32 build leaves this
//! module out and keeps the shared frame (`shell::shell_layout` and the
//! dock tabs) that every shell draws. `frame` holds the per-frame draw and
//! the winit `ApplicationHandler`.
//! Provenance: our own code, moved unchanged from shell.rs (M-a, from our
//! own spike), specced from the winit/egui-winit/egui-wgpu docs.

use std::sync::Arc;

use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::Window;

use egui_dock::DockState;
use egui_wgpu::wgpu;
use egui_wgpu::{Renderer, RendererOptions};

use crate::state::State;
use crate::viewport::View3D;

use super::frame_watch::FrameWatch;
use super::shell::*;
use super::{gpu, gpu_init, icons, theme, world};

mod frame;

/// The windowed shell (the `--ui egui` entry without `--headless`); returns
/// the process exit code.
pub(crate) fn run_windowed(pick: gpu::GpuPick, want: Option<String>, mcp_port: Option<u16>) -> i32 {
    let event_loop = match EventLoop::new() {
        Ok(el) => el,
        Err(err) => {
            eprintln!("cannot create event loop: {err}");
            return 1;
        }
    };
    event_loop.set_control_flow(ControlFlow::Wait);
    // #144: macOS's terminate path (Cmd+Q, the app menu's Quit) asks
    // through the guarded Quit instead of ending the process.
    let _ = super::os_quit::install();
    let mut shell = Shell::new(pick, want);
    // #183: the opt-in live MCP server. Its waker posts an empty user
    // event, which wakes the idle loop for a frame that answers the call.
    if let Some(port) = mcp_port {
        let proxy = event_loop.create_proxy();
        let wake: camcraft_app::mcp::Wake = Box::new(move || {
            let _ = proxy.send_event(());
        });
        match camcraft_app::mcp::Inbox::listen(port, wake) {
            Ok(inbox) => {
                if let Some(addr) = inbox.addr() {
                    eprintln!("mcp: listening on {addr} (connect with: camcraft-cli mcp --connect {addr})");
                }
                shell.state.mcp = Some(inbox);
            }
            Err(err) => {
                eprintln!("mcp: cannot listen on 127.0.0.1:{port}: {err}");
                shell.state.status = format!("MCP server not started: 127.0.0.1:{port}: {err}");
            }
        }
    }
    let code = match event_loop.run_app(&mut shell) {
        Ok(()) => shell.exit_code,
        Err(err) => {
            eprintln!("event loop failed: {err}");
            1
        }
    };
    shell.shutdown();
    code
}

/// Everything the loop owns. Field order matters at shutdown: the surface
/// borrows the window's raw handle, so it must drop before `window`, and
/// `state`'s world holds Rust-owned components whose delete hooks touch no
/// GPU resource - dropping it last is safe and keeps the ECS alive for
/// every other field's drop.
struct Shell {
    // Set on first resumed(); None until GPU init succeeds.
    window: Option<Arc<Window>>,
    window_title: String,
    device: Option<wgpu::Device>,
    queue: Option<wgpu::Queue>,
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
    renderer: Option<Renderer>,
    winit_state: Option<egui_winit::State>,
    egui_ctx: egui::Context,
    dock_state: DockState<Tab>,
    view: Option<View3D>,
    /// M7.2 B3b: the traced-stock eye; `None` where the device cannot
    /// composite it (the stock sheet then stands, said once at init).
    stock_eye: Option<crate::trace::eye::StockEye>,
    view_tex: Option<egui::TextureId>,
    /// The bundled glyphs, built once on the first GPU-backed frame (the
    /// retired imgui loader's law: lazy, and `None` means painted fallback,
    /// never blank strips).
    icons: Option<icons::IconSet>,
    /// Last laid-out viewport size in logical points; drives the View3D
    /// extent for the NEXT frame (imgui's shell uses the same deferral).
    viewport_points: [f32; 2],
    /// Where the last primary press landed (viewport panel coords), until its
    /// release decides pick-vs-orbit - the imgui App's `left_press` plumbed
    /// into the frame builder the same way `viewport_points` is (M-e).
    left_press: Option<[f32; 2]>,
    /// Content digest of the scene last uploaded to View3D; re-uploads are
    /// gated on it (App::sync_gpu's law, mirrored).
    scene_digest: u64,
    exit_code: i32,
    failed: bool,
    exit_requested: bool,
    /// M4.2: the validated GPU backend pick; drives init_gpu's attempts
    /// chain (preferred mask first, then auto, then all).
    gpu_pick: gpu::GpuPick,
    /// #74: the `--gpu` / `CAMCRAFT_GPU` adapter-name substring, tried first.
    gpu_want: Option<String>,
    /// #74: per-frame phase timing; a frame over budget is logged by phase.
    frame_watch: FrameWatch,
    /// Next redraw requested by egui or an active background task.
    next_repaint: Option<std::time::Instant>,
    /// One extra frame lets input-driven viewport changes reach the scene.
    followup_redraw: bool,
    /// A hidden surface has no useful frame timer.
    occluded: bool,
    /// Resize events can arrive faster than a GPU frame. Configure only for
    /// the newest size when the next resize frame is due.
    pending_resize: Option<PhysicalSize<u32>>,
    last_resize_frame: Option<std::time::Instant>,
    state: State,
}

impl Shell {
    fn new(pick: gpu::GpuPick, want: Option<String>) -> Self {
        // M-d: startup is a plain State::new() - byte-identical in shape to
        // imgui's App::new. The demo seed left the window path; world::seed
        // is what the harnesses and the headless proof call directly now.
        // File work is the real thing: New/Model/Project/Save/Save As on the ribbon
        // queue the shared command kinds, rfd picks the path, State::run
        // does the work - the machinery was never egui's to rebuild.
        let mut state = State::new();
        // #662: the user's recent projects and models, read once.
        state.load_recent();
        // #645: the user's view preferences (the #727 name toggle), read once.
        state.load_view();
        // #877: the user's theme and colour-vision mode, read once.
        state.load_prefs();
        let dock_state = build_dock();
        Self {
            window: None,
            window_title: state.window_title(),
            device: None,
            queue: None,
            surface: None,
            config: None,
            renderer: None,
            winit_state: None,
            egui_ctx: egui::Context::default(),
            dock_state,
            view: None,
            stock_eye: None,
            view_tex: None,
            icons: None,
            viewport_points: [600.0, 400.0],
            left_press: None,
            scene_digest: 0,
            exit_code: 0,
            failed: false,
            exit_requested: false,
            gpu_pick: pick,
            gpu_want: want,
            frame_watch: FrameWatch::new(std::time::Instant::now()),
            next_repaint: None,
            followup_redraw: false,
            occluded: false,
            pending_resize: None,
            last_resize_frame: None,
            state,
        }
    }

    fn fail(&mut self, el: &ActiveEventLoop, what: &str, err: impl std::fmt::Display) {
        eprintln!("egui shell: {what}: {err}");
        self.failed = true;
        self.exit_code = 1;
        el.exit();
    }

    fn init_gpu(&mut self, el: &ActiveEventLoop) {
        let attrs = Window::default_attributes()
            .with_title(self.window_title.clone())
            .with_inner_size(LogicalSize::new(1280.0, 800.0));
        let window = match el.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(err) => {
                self.fail(el, "window creation", err);
                return;
            }
        };
        // #1067: the native pickers take the shell window as their owner, so
        // an Open/Save dialog opens in front of the app rather than behind it.
        crate::file_dialog::set_parent(&window);
        // A window launched from a terminal or a script does not always get
        // the foreground: ask for it once, so the shell is not born behind
        // whatever launched it.
        window.focus_window();
        // M4.2 + #74: the honest fallback chain, adapter by adapter inside
        // each backend mask (gpu_init); a refusal names every adapter tried.
        // The View3D pipelines are built per candidate: an adapter that
        // cannot build them fails like one that cannot configure, and the
        // next adapter is tried (the "Intel never renders" path).
        let scale = window.scale_factor().clamp(0.1, 8.0);
        let (vw, vh) = super::view_pass::target_extent(self.viewport_points, scale);
        // View3D renders into its OWN texture (Rgba8Unorm, the format
        // egui's native-texture registration demands), decoupled from the
        // surface format - that is the registered-texture plan.
        let build = |device: &wgpu::Device, _: &wgpu::Queue, _: &wgpu::SurfaceConfiguration| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| View3D::new(device, VIEW3D_FORMAT, vw, vh)))
                .map_err(|_| "viewport3d pipeline creation aborted".to_string())
        };
        let gpu_init::Bring { surface, adapter, device, queue, config, built: mut view } =
            match gpu_init::bring_up(&window, &self.gpu_pick, self.gpu_want.as_deref(), build) {
                Ok(b) => b,
                Err(err) => {
                    refuse_dialog(&err);
                    self.fail(el, "GPU", err);
                    return;
                }
            };
        let gpu_line = gpu::describe(&adapter).log_line();
        println!("{gpu_line}");
        self.state.gpu_line = Some(gpu_line);

        let mut renderer = Renderer::new(&device, config.format, RendererOptions::default());
        let scene = world::collect(&self.state);
        self.scene_digest = scene.digest();
        view.set_document(&device, &queue, &scene.parts);
        view.set_overlay(&device, &queue, &scene.overlay);
        let stock_eye = match crate::trace::eye::StockEye::new(&device, VIEW3D_FORMAT) {
            Ok(e) => Some(e),
            Err(err) => {
                eprintln!("stock eye: {err} - the deviate sheet stands in");
                None
            }
        };
        let tex =
            renderer.register_native_texture(&device, view.color_view(), wgpu::FilterMode::Linear);
        let winit_state = egui_winit::State::new(
            self.egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &*window,
            Some(scale as f32),
            window.theme(),
            Some(device.limits().max_texture_dimension_2d as usize),
        );
        theme::apply(&self.egui_ctx);

        self.window = Some(window);
        self.device = Some(device);
        self.queue = Some(queue);
        self.surface = Some(surface);
        self.config = Some(config);
        self.renderer = Some(renderer);
        self.winit_state = Some(winit_state);
        self.view = Some(view);
        self.stock_eye = stock_eye;
        self.view_tex = Some(tex);
    }

    fn on_resize(&mut self, size: PhysicalSize<u32>) {
        let (w, h) = (size.width.max(1), size.height.max(1));
        let (Some(surface), Some(device), Some(config)) =
            (self.surface.as_ref(), self.device.as_ref(), self.config.as_mut())
        else {
            return;
        };
        if config.width == w && config.height == h {
            return;
        }
        config.width = w;
        config.height = h;
        let started = std::time::Instant::now();
        surface.configure(device, config);
        let elapsed = started.elapsed();
        if elapsed > super::frame_watch::BUDGET {
            eprintln!("gpu: slow resize configure {} ms", elapsed.as_millis());
        }
    }

}

/// The GPU refusal, also as a native dialog: a console-less launch must
/// not end in a window that silently vanishes (Prime Directive).
fn refuse_dialog(err: &str) {
    let dialog = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("CamCraft cannot start the viewport")
        .set_description(format!(
            "{err}

Try --gpu <adapter name> (or CAMCRAFT_GPU) to choose an adapter, or --gpu-backend dx12|vulkan|gl."
        ))
        .set_buttons(rfd::MessageButtons::Ok);
    // #1067: owned by the shell window, so the refusal is in front of the app.
    let dialog = match crate::file_dialog::parent() {
        Some(window) => dialog.set_parent(&*window),
        None => dialog,
    };
    let _ = dialog.show();
}

impl Shell {
    fn shutdown(&mut self) {
        // Surface/renderer first, window last (see the field-order note);
        // `state` (and its world) drops after all GPU objects.
        // #183: the MCP listener stops (and its thread is joined) first,
        // so no wake-up reaches a closing loop.
        self.state.mcp = None;
        self.renderer = None;
        self.view = None;
        self.surface = None;
        self.winit_state = None;
        self.queue = None;
        self.device = None;
        self.config = None;
        self.window = None;
    }
}

/// A resize frame is enough to update the window promptly without making
/// GPU reconfiguration and rendering block every pointer event in the drag.
fn resize_frame_deadline(last: Option<std::time::Instant>) -> std::time::Instant {
    last.and_then(|t| t.checked_add(std::time::Duration::from_millis(16)))
        .unwrap_or_else(std::time::Instant::now)
}

#[cfg(test)]
mod redraw_tests {
    use super::super::view_pass::redraw_delay;
    use std::time::Duration;

    #[test]
    fn idle_document_needs_no_frame_timer() {
        assert_eq!(
            redraw_delay(Duration::MAX, false, false, false, false),
            None
        );
    }

    #[test]
    fn queued_and_live_work_keep_the_window_moving() {
        assert_eq!(
            redraw_delay(Duration::MAX, true, false, false, false),
            Some(Duration::ZERO)
        );
        assert_eq!(
            redraw_delay(Duration::MAX, false, true, false, false),
            Some(Duration::from_millis(50))
        );
        assert_eq!(
            redraw_delay(Duration::MAX, false, false, true, false),
            Some(Duration::from_millis(16))
        );
        assert_eq!(
            redraw_delay(Duration::from_millis(10), false, true, false, false),
            Some(Duration::from_millis(10))
        );
    }
}
