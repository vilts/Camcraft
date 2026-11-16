//! PR #1710 review evidence, not snapshot goldens. Shipping shell and GPU viewport.
use super::*;
use crate::viewport::View3D;
use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube, cylinder};

fn origin(st: &State) -> [f64; 3] {
    let e = model::find_by_name(&st.world, "G54").expect("placed WCS");
    let p = model::transform_of(&st.world, &e).expect("placed").pos;
    [p.x, p.y, p.z]
}

fn scene(centre: bool) -> ShellUi {
    let mut sh = ShellUi::blank();
    let mesh = if centre {
        cylinder(20.0, 40.0, 48)
    } else {
        cube(Point3::ZERO, Point3::new(20.0, 20.0, 40.0))
    };
    sh.state
        .history
        .execute(&sh.state.world, ImportModel::from_mesh("part", mesh));
    sh.state.pending.push_back(CommandKind::SetWcs {
        pos: [1.0, 2.0, 3.0],
        roll_deg: 0.0,
        pitch_deg: 0.0,
        yaw_deg: 0.0,
    });
    sh.state.drain_actions();
    crate::actions::target(&mut sh.state, UiTarget::Wcs);
    sh.state.wcs_ui.anchor_z = 0;
    sh.state.wcs_ui.pick_anchor = true;
    sh.state.cam = crate::viewport::Camera {
        target: if centre {
            [0.0, 0.0, 20.0]
        } else {
            [10.0, 10.0, 20.0]
        },
        yaw: 0.3,
        pitch: 0.9,
        dist: 120.0,
        ..Default::default()
    };
    sh
}

fn viewport(h: &mut Harness<'static, ShellUi>) {
    let gpu = crate::headless_gpu::device().expect("review evidence requires an adapter");
    let sh = h.state_mut();
    let [w, height] = sh.viewport_points.map(|n| n.max(1.0).round() as u32);
    let mut view = View3D::new(&gpu.device, super::super::shell::VIEW3D_FORMAT, w, height);
    let scene = world::collect(&sh.state);
    view.set_document(&gpu.device, &gpu.queue, &scene.parts);
    view.set_overlay(&gpu.device, &gpu.queue, &scene.overlay);
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    view.render(
        &gpu.queue,
        &mut enc,
        &sh.state.cam.mvp(f64::from(w) / f64::from(height)),
        sh.state.cam.eye_f32(),
        sh.state.display,
    );
    gpu.queue.submit([enc.finish()]);
    let (rgba, w, height) = view
        .snapshot_rgba(&gpu.device, &gpu.queue)
        .expect("viewport readback");
    let img = ColorImage::from_rgba_unmultiplied([w as usize, height as usize], &rgba);
    sh.viewport_tex
        .as_mut()
        .expect("shell initialized")
        .set(img, TextureOptions::LINEAR);
    h.run_steps(2);
}

/// BEFORE is for the actual parent picker and hint source. Never writes goldens.
#[test]
#[ignore = "manual PR review evidence capture; requires GPU and writes evidence PNGs"]
fn review_1710_wcs_page_and_viewport_evidence() {
    let _guard = snapshot_guard();
    let out = std::env::temp_dir().join("camcraft-review-1710");
    let before =
        std::env::var_os("PR1710_BEFORE").is_some() || out.join(".before-capture").is_file();
    std::fs::create_dir_all(&out).expect("evidence directory");
    for centre in [false, true] {
        let mut h = Harness::builder()
            .with_size(Vec2::new(1440.0, 960.0))
            .build_ui_state(frame, scene(centre));
        h.run_steps(3);
        let target = if centre {
            [0.0, 0.0, 40.0]
        } else {
            [20.0, 20.0, 40.0]
        };
        let st = &mut h.state_mut().state;
        let (min, max) = st.viewport_rect;
        let size = [max[0] - min[0], max[1] - min[1]];
        let uv = interact::project_uv(&st.cam.mvp(f64::from(size[0] / size[1])), target)
            .expect("target visible");
        let p = [min[0] + uv[0] * size[0], min[1] + uv[1] * size[1]];
        assert!(
            interact::anchor_hit(st, p, 12.0).is_none(),
            "target must miss box markers"
        );
        interact::pick_at(st, p, interact::Mods::default());
        st.drain_actions();
        let expected = if before { [1.0, 2.0, 3.0] } else { target };
        assert!(
            origin(st)
                .iter()
                .zip(expected)
                .all(|(a, b)| (a - b).abs() < 1e-6),
            "click at {target:?}: origin {:?}, expected {expected:?}",
            origin(st)
        );
        h.run_steps(3);
        let texts = structural::painted(&h);
        assert!(
            texts
                .iter()
                .any(|(t, _)| t.contains("Origin and orientation")),
            "WCS page visible"
        );
        assert!(
            texts
                .iter()
                .any(|(t, _)| t.contains("Click a yellow marker")),
            "armed picker hint visible"
        );
        viewport(&mut h);
        let name = format!(
            "{}-{}.png",
            if centre { "centre" } else { "corner" },
            if before { "before" } else { "after" }
        );
        h.render()
            .expect("shell render")
            .save(out.join(name))
            .expect("evidence PNG");
    }
}
