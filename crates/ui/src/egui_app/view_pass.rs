//! The viewport's 3D pass and the repaint law, shared by every shell (#31):
//! the native window (`shell_window::frame`) and the browser shell
//! (`shell_web`) upload a changed scene, size the offscreen target and
//! encode the same View3D + traced-stock pass through these functions, so
//! the two cannot disagree about what the viewport shows or when the next
//! frame is due.
//! Provenance: our own code, moved unchanged from the native frame
//! (shell.rs at M-a, from our own spike), specced from the wgpu/egui-wgpu
//! docs.

use egui_wgpu::wgpu;

use crate::state::State;
use crate::trace::eye::StockEye;
use crate::viewport::View3D;

use super::shell::VIEW3D_FORMAT;
use super::world;

/// Re-upload only a scene whose content actually changed (the imgui
/// shell's digest gate; camera motion costs zero traffic).
pub(crate) fn sync_scene(
    view: &mut View3D,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &world::Scene,
    scene_digest: &mut u64,
) {
    let d = scene.digest();
    if d != *scene_digest {
        view.set_document(device, queue, &scene.parts);
        view.set_shade(device, queue, &scene.shade);
        view.set_overlay(device, queue, &scene.overlay);
        *scene_digest = d;
    }
}

/// The pixel ratio a shell may size the 3D target with: what the window or
/// the browser reports, held to 0.1..=8 (1 when it is not a number), so a
/// zero or absurd ratio can neither collapse nor explode the target.
pub(crate) fn target_scale(reported: f64) -> f64 {
    if reported.is_nan() { 1.0 } else { reported.clamp(0.1, 8.0) }
}

/// The 3D target's pixel size for a laid-out viewport of `points` at
/// `scale` pixels per point (held by `target_scale`): never zero, never
/// past what a u32 holds.
pub(crate) fn target_extent(points: [f32; 2], scale: f64) -> (u32, u32) {
    let scale = target_scale(scale);
    let px = |p: f32| (f64::from(p) * scale).max(1.0) as u32;
    (px(points[0]), px(points[1]))
}

/// `want`, held to the longest texture side the device makes
/// (`Limits::max_texture_dimension_2d`): a longer side is a fatal wgpu
/// validation error, so a huge window or a degenerate layout must not reach
/// `create_texture` (#3582). The limit comes from the device, so it holds on
/// any GPU. Never zero.
pub(crate) fn limit_extent(want: (u32, u32), max_side: u32) -> (u32, u32) {
    let max_side = max_side.max(1);
    (want.0.min(max_side), want.1.min(max_side))
}

/// The viewport size to keep after a frame's layout. A window with no area
/// (minimized) gets egui's default 10000 x 10000 point screen, so that
/// layout is not the viewport's size: keep the last real one (#3582).
pub(crate) fn viewport_after_layout(before: [f32; 2], laid_out: [f32; 2], window_px: (u32, u32)) -> [f32; 2] {
    if window_px.0 == 0 || window_px.1 == 0 { before } else { laid_out }
}

/// Size the 3D target from last frame's layout. True when the target was
/// recreated: the caller then rebinds its egui texture id to the new view.
pub(crate) fn fit_target(view: &mut View3D, device: &wgpu::Device, want: (u32, u32)) -> bool {
    let want = limit_extent(want, device.limits().max_texture_dimension_2d);
    if view.extent() == want {
        return false;
    }
    view.resize(device, VIEW3D_FORMAT, want.0, want.1);
    true
}

/// #2958: wear the viewer's line width and anti-aliasing. The width is a
/// uniform (nothing re-uploads); a new sample count rebuilds the pass's
/// pipelines and multisample targets once, the composite's with them. A
/// kept count that this device does not offer falls back to the highest
/// offered count below it, said once on the status line.
pub(crate) fn apply_quality(
    state: &mut State,
    view: &mut View3D,
    stock_eye: &mut Option<StockEye>,
    device: &wgpu::Device,
    scale: f64,
) {
    view.set_line_px(crate::viewport::line_px(state.line_width.points(), target_scale(scale)));
    let want = state.antialias.samples();
    let samples = crate::viewport::effective_samples(want, &state.samples_offered);
    if samples != want {
        state.say_once(&format!(
            "anti-aliasing {} is not offered by this graphics device - {} samples per pixel are used",
            state.antialias.word(),
            samples
        ));
    }
    if view.set_samples(device, samples) {
        diag_info!("viewport: {samples} samples per pixel");
    }
    if let Some(se) = stock_eye.as_mut() {
        se.set_samples(device, samples);
    }
}

/// Pass 1 of a frame: the 3D offscreen target (its own bind group, mvp
/// from the camera the panels' FrameAll and the orbit gestures just moved).
pub(crate) fn encode(
    state: &mut State,
    view: &mut View3D,
    stock_eye: &mut Option<StockEye>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    scale: f64,
) {
    apply_quality(state, view, stock_eye, device, scale);
    let (ew, eh) = view.extent();
    let mvp = state.cam.mvp(f64::from(ew.max(1)) / f64::from(eh.max(1)));
    let eye = state.cam.eye_f32();
    // The traced stock (M7.2 B3b) takes the sheet's slot when the eye
    // has a frame; the field view is walked once per Deviate arming.
    let mut traced = false;
    if let Some(se) = stock_eye.as_mut().filter(|_| !state.active_stock_hidden()) {
        // #3336: a playback that starts now may cut on the GPU layer only
        // for an eye that draws the packed frame (`play_layer`).
        state.layer_play.eye = se.traces_on_gpu();
        if !state.display.is_wireframe() {
            crate::actions::ensure_field_view(state);
        }
        let fv = match &state.field_view {
            Some(Ok(fv)) if !state.display.is_wireframe() => Some(fv),
            _ => None,
        };
        match se.prepare(device, queue, encoder, fv, &state.cam, &mvp, (ew, eh)) {
            Ok(t) => traced = t,
            Err(err) => {
                state.say_once(&format!("the cut stock cannot be drawn, the uncut stock shows: {err}"));
            }
        }
        // #3336: the eye took the layer frame but marches it on the CPU,
        // from a stock that lacks the layer's cut: the CPU playback runs.
        let held = state.field_view.as_ref().and_then(|r| r.as_ref().ok()).and_then(|fv| se.holds_on_gpu(fv));
        if state.layer_play.must_end(held.is_some(), held == Some(true)) {
            crate::play_layer::end(state);
            traced = false;
        }
    }
    // M13.4: both verdict painters wear the viewer's palette.
    let pal = super::colorvision::palette_of(&state.world);
    view.set_palette(pal);
    view.set_effects(state.effects());
    if let Some(se) = stock_eye.as_mut() {
        se.set_palette(pal);
    }
    match stock_eye.as_ref().filter(|_| traced) {
        Some(se) => {
            let draw = |pass: &mut wgpu::RenderPass<'_>| se.draw(pass);
            view.render_with(queue, encoder, &mvp, eye, state.display, Some(&draw));
        }
        None => view.render(queue, encoder, &mvp, eye, state.display),
    }
}

/// Idle documents need no redraw. Active egui animation, queued commands,
/// imports and simulation each name the next useful frame.
pub(crate) fn redraw_delay(
    egui_delay: std::time::Duration,
    pending: bool,
    importing: bool,
    sim_playing: bool,
    field_play_pending: bool,
) -> Option<std::time::Duration> {
    use std::time::Duration;
    let mut delay = egui_delay;
    if pending {
        delay = Duration::ZERO;
    }
    if importing || field_play_pending {
        delay = delay.min(Duration::from_millis(50));
    }
    if sim_playing {
        delay = delay.min(Duration::from_millis(16));
    }
    (delay != Duration::MAX).then_some(delay)
}

/// What the document asks of the frame timer, whatever egui itself wants:
/// `redraw_delay` fed from the state both shells read.
pub(crate) fn state_redraw_delay(
    state: &State,
    egui_delay: std::time::Duration,
    followup: bool,
) -> Option<std::time::Duration> {
    // #2463: a linked model's file is checked about once a second.
    let egui_delay = if crate::model_link_watch::wants_ticks(state) {
        egui_delay.min(crate::model_link_watch::INTERVAL)
    } else {
        egui_delay
    };
    redraw_delay(
        egui_delay,
        followup || !state.pending.is_empty() || state.layout_request.is_some(),
        state.import_in_progress() || state.gen_jobs.busy(),
        state.sim.is_some_and(|sim| sim.playing),
        state.field_play.as_ref().is_some_and(|play| play.needs_poll()),
    )
}

#[cfg(test)]
mod tests {
    use super::{limit_extent, redraw_delay, target_extent, viewport_after_layout};
    use std::time::Duration;

    #[test]
    fn target_extent_is_never_zero_and_scales_with_the_pixel_ratio() {
        assert_eq!(target_extent([600.0, 400.0], 1.0), (600, 400));
        assert_eq!(target_extent([600.0, 400.0], 1.5), (900, 600));
        // A collapsed or not-yet-laid-out viewport still gets a 1x1 target.
        assert_eq!(target_extent([0.0, -5.0], 2.0), (1, 1));
        assert_eq!(target_extent([f32::NAN, 10.0], 1.0), (1, 10));
        // The first browser frame passes the reported pixel ratio straight
        // in: zero, negative, absurd or not-a-number must still size a
        // usable target (the clamp the frame paths apply, 0.1..=8).
        assert_eq!(target_extent([600.0, 400.0], 0.0), (60, 40));
        assert_eq!(target_extent([600.0, 400.0], -3.0), (60, 40));
        assert_eq!(target_extent([600.0, 400.0], 1.0e9), (4800, 3200));
        assert_eq!(target_extent([600.0, 400.0], f64::INFINITY), (4800, 3200));
        assert_eq!(target_extent([600.0, 400.0], f64::NAN), (600, 400));
        assert_eq!(target_extent([0.0, 0.0], 0.0), (1, 1));
    }

    #[test]
    fn limit_extent_holds_each_side_to_the_device_limit() {
        // The minimized-window layout of #3582: 6078 x 9838 points at a 2.5
        // pixel ratio, on a device whose longest texture side is 8192.
        assert_eq!(limit_extent((15195, 24595), 8192), (8192, 8192));
        // Any limit, not only 8192.
        assert_eq!(limit_extent((3000, 500), 2048), (2048, 500));
        assert_eq!(limit_extent((15195, 24595), 16384), (15195, 16384));
        // Inside the limit nothing changes; a zero limit never gives zero.
        assert_eq!(limit_extent((1500, 1000), 8192), (1500, 1000));
        assert_eq!(limit_extent((1500, 1000), 0), (1, 1));
    }

    #[test]
    fn a_window_with_no_area_keeps_the_last_real_viewport_size() {
        let real = [1500.0, 1000.0];
        let degenerate = [6078.0, 9838.0];
        // Minimized: the window has no pixels, egui laid out its default screen.
        assert_eq!(viewport_after_layout(real, degenerate, (0, 0)), real);
        assert_eq!(viewport_after_layout(real, degenerate, (0, 800)), real);
        assert_eq!(viewport_after_layout(real, degenerate, (1200, 0)), real);
        // A window with an area takes its own layout.
        assert_eq!(viewport_after_layout(real, [900.0, 600.0], (1280, 800)), [900.0, 600.0]);
    }

    #[test]
    fn fit_target_never_asks_the_device_for_a_texture_past_its_limit() {
        // Without the clamp, wgpu's fatal validation error ends the process
        // ("Dimension X value ... exceeds the limit") - #3582.
        let Some(gpu) = crate::headless_gpu::test_gpu("view_pass").expect("adapter") else { return }; // #3605
        let limit = gpu.device.limits().max_texture_dimension_2d;
        let mut view = crate::viewport::View3D::new(&gpu.device, super::VIEW3D_FORMAT, 64, 48);
        assert!(super::fit_target(&mut view, &gpu.device, (limit + 1, 16)));
        assert_eq!(view.extent(), (limit, 16));
    }

    #[test]
    fn a_browser_frame_with_no_egui_timer_still_follows_the_document() {
        // eframe owns egui's own repaint timer on the web, so the browser
        // shell passes Duration::MAX and only the document's needs remain.
        assert_eq!(redraw_delay(Duration::MAX, false, false, false, false), None);
        assert_eq!(redraw_delay(Duration::MAX, false, false, false, true), Some(Duration::from_millis(50)));
        assert_eq!(redraw_delay(Duration::MAX, true, true, true, true), Some(Duration::ZERO));
    }
}
