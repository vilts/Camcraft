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

/// Size the 3D target from last frame's layout. True when the target was
/// recreated: the caller then rebinds its egui texture id to the new view.
pub(crate) fn fit_target(view: &mut View3D, device: &wgpu::Device, want: (u32, u32)) -> bool {
    if view.extent() == want {
        return false;
    }
    view.resize(device, VIEW3D_FORMAT, want.0, want.1);
    true
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
) {
    let (ew, eh) = view.extent();
    let mvp = state.cam.mvp(f64::from(ew.max(1)) / f64::from(eh.max(1)));
    let eye = state.cam.eye_f32();
    // The traced stock (M7.2 B3b) takes the sheet's slot when the eye
    // has a frame; the field view is walked once per Deviate arming.
    let mut traced = false;
    if let Some(se) = stock_eye.as_mut().filter(|_| !state.active_stock_hidden()) {
        if state.display != crate::state::DisplayMode::Wireframe {
            crate::actions::ensure_field_view(state);
        }
        let fv = match &state.field_view {
            Some(Ok(fv)) if state.display != crate::state::DisplayMode::Wireframe => Some(fv),
            _ => None,
        };
        match se.prepare(device, queue, encoder, fv, &state.cam, &mvp, (ew, eh)) {
            Ok(t) => traced = t,
            Err(err) => {
                state.say_once(&format!("the cut stock cannot be drawn, the uncut stock shows: {err}"));
            }
        }
    }
    // M13.4: both verdict painters wear the viewer's palette.
    let pal = super::colorvision::palette_of(&state.world);
    view.set_palette(pal);
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
    use super::{redraw_delay, target_extent};
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
    fn a_browser_frame_with_no_egui_timer_still_follows_the_document() {
        // eframe owns egui's own repaint timer on the web, so the browser
        // shell passes Duration::MAX and only the document's needs remain.
        assert_eq!(redraw_delay(Duration::MAX, false, false, false, false), None);
        assert_eq!(redraw_delay(Duration::MAX, false, false, false, true), Some(Duration::from_millis(50)));
        assert_eq!(redraw_delay(Duration::MAX, true, true, true, true), Some(Duration::ZERO));
    }
}
