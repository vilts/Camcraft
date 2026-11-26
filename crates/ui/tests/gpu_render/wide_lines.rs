//! #2958: the overlay draws as wide anti-aliased lines. On a real device
//! frame: a line that lies on a face keeps its full width (the depth
//! lift), two lines that meet close the corner and do not reach past it,
//! the width follows `set_line_px`, and 4x MSAA smooths the edges that
//! 1x leaves hard. Same device, lock and skip policy as the rest of
//! `gpu_render` (one GPU process for the binary).
//! Provenance: our own tests, specced from the issue's "Done means".

use super::{GPU_TEST_LOCK, SIDE, distinct_colors, model_part, shared_device, test_cam};
use camcraft_ui::state::DisplayMode;
use egui_wgpu::wgpu;
use camcraft_ui::viewport::{Camera, GpuPart, ProjectionMode, View3D};

const RED: [f32; 3] = [1.0, 0.0, 0.0];

/// A flat opaque sheet at z = 0, counter-clockwise from above.
fn floor(h: f32) -> GpuPart {
    let q = [[-h, -h, 0.0], [h, -h, 0.0], [h, h, 0.0], [-h, -h, 0.0], [h, h, 0.0], [-h, h, 0.0]];
    GpuPart { verts: q.to_vec(), selected: false, stock: false, fill: false, classes: Vec::new() }
}

fn seg(a: [f32; 3], b: [f32; 3]) -> [[f32; 6]; 2] {
    [[a[0], a[1], a[2], RED[0], RED[1], RED[2]], [b[0], b[1], b[2], RED[0], RED[1], RED[2]]]
}

/// One SIDE x SIDE frame of `parts` and `lines`; `None` without a device.
fn frame(parts: &[GpuPart], lines: &[[f32; 6]], cam: &Camera, px: f32, samples: u32) -> Option<Vec<u8>> {
    let _gpu_guard = GPU_TEST_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some((device, queue)) = shared_device() else {
        eprintln!("gpu_render wide_lines: no adapter - SKIP");
        return None;
    };
    let mut view = View3D::new(&device, super::FORMAT, SIDE, SIDE);
    assert!(view.set_samples(&device, samples) == (samples != 1), "a new count rebuilds; the same count does not");
    assert_eq!(view.samples(), samples);
    view.set_line_px(px);
    view.set_document(&device, &queue, parts);
    view.set_overlay(&device, &queue, lines);
    assert_eq!(view.overlay_segments() as usize, lines.len() / 2, "one instance per segment");
    let mvp = cam.mvp(1.0);
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("wide-lines") });
    view.render(&queue, &mut enc, &mvp, cam.eye_f32(), DisplayMode::ShadedEdges);
    queue.submit([enc.finish()]);
    let (rgba, w, h) = view.snapshot_rgba(&device, &queue)?;
    assert_eq!((w, h), (SIDE, SIDE));
    Some(rgba)
}

fn is_red(img: &[u8], x: i32, y: i32) -> bool {
    if x < 0 || y < 0 || x >= SIDE as i32 || y >= SIDE as i32 {
        return false;
    }
    let i = ((y as u32 * SIDE + x as u32) * 4) as usize;
    let (r, g, b) = (i32::from(img[i]), i32::from(img[i + 1]), i32::from(img[i + 2]));
    r > g + 100 && r > b + 100
}

fn red_count(img: &[u8]) -> usize {
    (0..SIDE as i32).flat_map(|y| (0..SIDE as i32).map(move |x| (x, y))).filter(|(x, y)| is_red(img, *x, *y)).count()
}

/// Where `p` lands in the frame, in pixels (the mvp is column-major).
fn pixel(cam: &Camera, p: [f32; 3]) -> [f32; 2] {
    let m = cam.mvp(1.0);
    let c: [f32; 4] = std::array::from_fn(|r| m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r]);
    let s = SIDE as f32;
    [(c[0] / c[3] * 0.5 + 0.5) * s, (0.5 - c[1] / c[3] * 0.5) * s]
}

fn red_at(img: &[u8], at: [f32; 2]) -> bool {
    is_red(img, at[0].floor() as i32, at[1].floor() as i32)
}

fn tilted() -> Camera {
    Camera { target: [0.0, 0.0, 0.0], yaw: 0.9, pitch: 0.5, dist: 80.0, ..Camera::default() }
}

#[test]
fn a_line_on_a_face_keeps_its_full_width() {
    let line = seg([-20.0, 0.0, 0.0], [20.0, 0.0, 0.0]);
    let cam = tilted();
    let Some(free) = frame(&[], &line, &cam, 6.0, 1) else { return };
    let Some(on_face) = frame(&[floor(30.0)], &line, &cam, 6.0, 1) else { return };
    let (free, on_face) = (red_count(&free), red_count(&on_face));
    assert!(free > 300, "a 6 px line over ~100 px paints a band: {free}");
    // Without the lift the face's depth hid about half the band.
    assert!(on_face * 10 >= free * 9, "the band on the face ({on_face} px) is the free band ({free} px)");
}

#[test]
fn the_line_width_setting_widens_the_band() {
    let line = seg([-20.0, 0.0, 0.0], [20.0, 0.0, 0.0]);
    let cam = tilted();
    let Some(thin) = frame(&[], &line, &cam, 2.0, 1) else { return };
    let Some(wide) = frame(&[], &line, &cam, 6.0, 1) else { return };
    let (thin, wide) = (red_count(&thin), red_count(&wide));
    assert!(thin > 0 && wide >= thin * 2, "6 px paints at least twice 2 px: {thin} vs {wide}");
}

#[test]
fn two_lines_that_meet_close_the_corner_and_do_not_reach_past_it() {
    // Straight down, orthographic: the screen is the XY plane.
    let cam = Camera {
        target: [10.0, 10.0, 0.0],
        yaw: 0.0,
        pitch: std::f64::consts::FRAC_PI_2 - 1e-3,
        dist: 60.0,
        projection: ProjectionMode::Orthographic,
        ..Camera::default()
    };
    let (a, b, c) = ([0.0, 0.0, 0.0], [20.0, 0.0, 0.0], [0.0, 20.0, 0.0]);
    let mut lines = seg(a, b).to_vec();
    lines.extend(seg(a, c));
    let Some(img) = frame(&[], &lines, &cam, 6.0, 1) else { return };
    let (pa, pb, pc) = (pixel(&cam, a), pixel(&cam, b), pixel(&cam, c));
    let unit = |from: [f32; 2], to: [f32; 2]| {
        let d = [to[0] - from[0], to[1] - from[1]];
        let l = d[0].hypot(d[1]);
        [d[0] / l, d[1] / l]
    };
    let (ab, ac) = (unit(pa, pb), unit(pa, pc));
    let out = unit([0.0, 0.0], [-(ab[0] + ac[0]), -(ab[1] + ac[1])]);
    let at = |p: [f32; 2], d: [f32; 2], k: f32| [p[0] + d[0] * k, p[1] + d[1] * k];
    // Half width 3 px: the square end of each line fills the corner...
    assert!(red_at(&img, at(pa, out, 2.0)), "the outer corner is closed");
    // ...and stops there: nothing past the corner square or an end.
    assert!(!red_at(&img, at(pa, out, 3.0 * std::f32::consts::SQRT_2 + 2.0)), "no overshoot past the corner");
    assert!(red_at(&img, at(pb, ab, 1.5)), "an end reaches half the width past its point");
    assert!(!red_at(&img, at(pb, ab, 5.5)), "and no further");
}

#[test]
fn msaa_smooths_the_edges_that_one_sample_leaves_hard() {
    let parts = [model_part(40.0)];
    let cam = test_cam();
    let Some(one) = frame(&parts, &[], &cam, 1.5, 1) else { return };
    let Some(four) = frame(&parts, &[], &cam, 1.5, 4) else { return };
    let (one, four) = (distinct_colors(&one), distinct_colors(&four));
    assert!(four > one, "4x blends edge pixels into new shades: {one} colours at 1x, {four} at 4x");
}
