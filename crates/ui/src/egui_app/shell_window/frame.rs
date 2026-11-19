//! The native shell's frame (#31 split it out of shell.rs): one draw per
//! redraw request and the winit `ApplicationHandler` that schedules them.
//! Native only, like its parent `shell_window`.
//! Provenance: our own code, moved unchanged from shell.rs (M-a, from our
//! own spike), specced from the winit/egui-winit/egui-wgpu docs.

use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::WindowId;

use egui_wgpu::wgpu;
use egui_wgpu::{Renderer, ScreenDescriptor};

use super::super::shell::*;
use super::super::{icons, view_pass, world};
use super::{resize_frame_deadline, Shell};


impl Shell {
    fn draw(&mut self) {
        if self.failed || self.occluded {
            return;
        }
        let now = std::time::Instant::now;
        let followup_redraw = std::mem::take(&mut self.followup_redraw);
        self.frame_watch.begin(now());
        // #2262: the frame-rate rule logs on a level change only.
        if let Some(line) = self.state.frame_rate.frame(now()) {
            diag_warn!("{line}");
        }
        // Commands the panels queued last frame land before the scene is
        // collected, so this frame draws what the user's click did.
        self.state.drain_actions();
        let title = self.state.window_title();
        if title != self.window_title {
            if let Some(window) = &self.window {
                window.set_title(&title);
            }
            self.window_title = title;
        }
        // #866: a modal file dialog's open time is the user's, not ours.
        self.frame_watch.excuse(crate::file_dialog::take_waited(), now());
        self.frame_watch.mark("commands", now());
        let scene = world::collect(&self.state);

        let Self {
            window,
            device,
            queue,
            surface,
            config,
            renderer,
            winit_state,
            egui_ctx,
            dock_state,
            view,
            stock_eye,
            view_tex,
            icons,
            viewport_points,
            left_press,
            scene_digest,
            exit_requested,
            frame_watch,
            next_repaint,
            state,
            ..
        } = self;
        frame_watch.mark("scene", now());
        let (
            Some(window),
            Some(device),
            Some(queue),
            Some(surface),
            Some(conf),
            Some(renderer),
            Some(winit_state),
            Some(view),
            Some(tex_id),
        ) = (
            window.as_ref(),
            device.as_ref(),
            queue.as_ref(),
            surface.as_ref(),
            config.as_ref(),
            renderer.as_mut(),
            winit_state.as_mut(),
            view.as_mut(),
            view_tex.as_ref().copied(),
        ) else {
            return;
        };

        view_pass::sync_scene(view, device, queue, &scene, scene_digest);

        let scale = window.scale_factor().clamp(0.1, 8.0);
        let raw_input = winit_state.take_egui_input(window);

        // Size the 3D target from last frame's layout, like the imgui shell.
        let want = view_pass::target_extent(*viewport_points, scale);
        if view_pass::fit_target(view, device, want) {
            // Rebind the SAME egui texture id to the recreated view.
            renderer.update_egui_texture_from_wgpu_texture(
                device,
                view.color_view(),
                wgpu::FilterMode::Linear,
                tex_id,
            );
        }

        // Icons decode once, on the first frame that reaches this point -
        // the GPU-backed one. The deltas they queue ride the drain below
        // like the font atlas does.
        if icons.is_none() {
            *icons = Some(icons::IconSet::load(egui_ctx));
        }

        // UI logic first: the dock reports the viewport tab's size, the GPU
        // passes follow (the imgui shell's ordering law - panels drive
        // extent).
        let mut full_output = egui_ctx.run_ui(raw_input, |ui| {
            shell_layout(
                ui,
                state,
                dock_state,
                tex_id,
                icons.as_ref(),
                viewport_points,
                left_press,
            );
        });
        let egui_delay = full_output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(std::time::Duration::MAX, |v| v.repaint_delay);
        let delay = view_pass::state_redraw_delay(state, egui_delay, followup_redraw);
        *next_repaint = delay.and_then(|d| now().checked_add(d));
        drain_textures(renderer, device, queue, &mut full_output);
        winit_state.handle_platform_output(window, full_output.platform_output);

        let clipped: Vec<_> = egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point)
            .into_iter()
            .filter(renderable_paint_job)
            .collect();
        // Panels open file dialogs while they draw, so collect that wait here too.
        frame_watch.excuse(crate::file_dialog::take_waited(), now());
        frame_watch.mark("ui", now());

        let st = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(st)
            | wgpu::CurrentSurfaceTexture::Suboptimal(st) => st,
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Validation => {
                *next_repaint = now().checked_add(std::time::Duration::from_millis(16));
                return;
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                // Some platforms report this through wgpu without sending
                // WindowEvent::Occluded. Wait for the next window event.
                *next_repaint = None;
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                surface.configure(device, conf);
                *next_repaint = now().checked_add(std::time::Duration::from_millis(16));
                return;
            }
        };
        frame_watch.mark("acquire", now());
        let target = st.texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Pass 1: the 3D offscreen target (shared with the browser shell).
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("egui-shell-frame"),
        });
        view_pass::encode(state, view, stock_eye, device, queue, &mut encoder);

        // Pass 2: egui over the surface.
        let sd = ScreenDescriptor {
            size_in_pixels: [conf.width, conf.height],
            pixels_per_point: scale as f32,
        };
        let mut buffer_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("egui-shell-buffers"),
        });
        let extra = renderer.update_buffers(device, queue, &mut buffer_encoder, &clipped, &sd);
        // SAFETY of forget_lifetime: everything the pass encodes against
        // (target view, egui textures, buffer binds) outlives this block -
        // the pass is finished when the encoder closes the scope.
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            })
            .forget_lifetime();
        renderer.render(&mut pass, &clipped, &sd);
        drop(pass);
        frame_watch.mark("encode", now());
        let mut submits = extra;
        submits.push(buffer_encoder.finish());
        submits.push(encoder.finish());
        queue.submit(submits);
        queue.present(st);
        frame_watch.mark("submit+present", now());
        if let Some(line) = frame_watch.end(now()) {
            diag_warn!("{line}");
        }

        if state.request_quit {
            *exit_requested = true;
        }
    }
}

fn drain_textures(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    out: &mut egui::FullOutput,
) {
    for (id, deltas) in out.textures_delta.set.drain() {
        for delta in deltas {
            renderer.update_texture(device, queue, id, &delta);
        }
    }
    for id in out.textures_delta.free.drain() {
        renderer.free_texture(&id);
    }
}

impl winit::application::ApplicationHandler for Shell {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_none() && !self.failed {
            self.init_gpu(el);
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    /// #183: the MCP listener's wake-up - draw a frame, which answers it.
    fn user_event(&mut self, _el: &ActiveEventLoop, _event: ()) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let WindowEvent::Occluded(occluded) = &event {
            self.occluded = *occluded;
        }
        // Feed egui first (pointer/keyboard/text/scale), like the imgui
        // shell's platform.prepare_frame; the EventResponse (child-viewport
        // control flow) is inert for our single root viewport.
        let repaint = match (self.winit_state.as_mut(), self.window.as_ref()) {
            (Some(winit_state), Some(window)) => winit_state.on_window_event(window, &event).repaint,
            _ => false,
        };
        if repaint && !self.occluded
            && !matches!(event, WindowEvent::RedrawRequested | WindowEvent::Resized(_))
            && let Some(window) = &self.window
        {
            self.followup_redraw = true;
            window.request_redraw();
        }
        match &event {
            // #107: the window X is a Quit like Ctrl+Q, so unsaved changes
            // ask first. A shell whose GPU failed draws (and drains)
            // nothing, so it closes at once.
            WindowEvent::CloseRequested if self.failed => el.exit(),
            WindowEvent::CloseRequested => {
                crate::actions::quit(&mut self.state);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::Resized(size) => {
                self.pending_resize = Some(*size);
            }
            WindowEvent::Occluded(false) | WindowEvent::Focused(true) => {
                if let Some(window) = &self.window && !self.occluded {
                    self.followup_redraw = true;
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                let resized = if let Some(size) = self.pending_resize {
                    let deadline = resize_frame_deadline(self.last_resize_frame);
                    if std::time::Instant::now() < deadline {
                        return;
                    }
                    self.pending_resize = None;
                    self.on_resize(size);
                    self.followup_redraw = true;
                    true
                } else {
                    false
                };
                self.draw();
                if resized {
                    let done = std::time::Instant::now();
                    self.last_resize_frame = Some(done);
                    // The layout's View3D size lands on a follow-up frame.
                    // Keep that frame from immediately blocking the next
                    // batch of pointer and resize events.
                    if let (Some(next), Some(earliest)) = (
                        self.next_repaint.as_mut(),
                        done.checked_add(std::time::Duration::from_millis(16)),
                    ) {
                        *next = (*next).max(earliest);
                    }
                }
                if self.exit_requested || self.state.request_quit {
                    el.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        // #144: an OS quit (macOS Cmd+Q / app-menu Quit) is a Quit like
        // the window X; the window comes forward so the prompt is seen.
        if super::super::os_quit::route_pending(&mut self.state) {
            if self.failed {
                el.exit();
                return;
            }
            if let Some(window) = &self.window {
                window.set_minimized(false);
                window.focus_window();
                window.request_redraw();
            }
        }
        if self.occluded {
            el.set_control_flow(ControlFlow::Wait);
            return;
        }
        let resize_due = self.pending_resize.map(|_| resize_frame_deadline(self.last_resize_frame));
        // A pending resize carries the repaint too. Its frame deadline
        // supersedes older egui timers, including an immediate follow-up.
        let next = resize_due.or(self.next_repaint);
        match (next, self.window.as_ref()) {
            (Some(deadline), Some(window)) if deadline <= std::time::Instant::now() => {
                self.next_repaint = None;
                window.request_redraw();
                el.set_control_flow(ControlFlow::Wait);
            }
            (Some(deadline), Some(_)) => el.set_control_flow(ControlFlow::WaitUntil(deadline)),
            _ => el.set_control_flow(ControlFlow::Wait),
        }
    }
}
