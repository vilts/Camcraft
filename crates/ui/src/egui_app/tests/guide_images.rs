//! Reproducible off-screen UI + 3D viewport capture for User Guide images (#1902).
//!
//! Provenance: original guide image harness driving `ShellUi` + `View3D` over
//! CamCraft's own demo projects with zero external or vendor images.

use std::path::{Path, PathBuf};

use camcraft_app::links::Links;
use camcraft_app::machining2d::{Lead, PocketStrategy};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::OpParams;
use egui::{ColorImage, TextureOptions};
use image::ImageEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};

use super::super::op_tabs::{SEC_GEO, SEC_LEADS, SEC_PASSES};
use super::super::world;
use super::guide_projects::{projects_dir, workspace_root, write_all_demo_projects};
use super::*;
use crate::actions;
use crate::state::{CommandKind, SetupPage, UiTarget};
use crate::viewport::{Camera, View3D};

const SHOT_SIZE: Vec2 = Vec2::new(1200.0, 760.0);

pub(super) const GUIDE_FILES: [&str; 13] = [
    "setup-overview.png",
    "workholding-vise-toolpath.png",
    "op-2d-pocket-params.png",
    "op-2d-stepover-before.png",
    "op-2d-stepover-after.png",
    "op-2d-entry-before.png",
    "op-2d-entry-after.png",
    "op-2d-lead-before.png",
    "op-2d-lead-after.png",
    "op-3d-pocket-roughing.png",
    "op-3d-finish-parallel.png",
    "op-multiaxis-swarf.png",
    "op-indexed-3plus2.png",
];

fn out_dir() -> PathBuf {
    std::env::var_os("CAMCRAFT_GUIDE_IMAGES_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("camcraft-guide-images"))
}

fn new_harness(project_path: &Path) -> Harness<'static, ShellUi> {
    let mut sh = ShellUi::blank();
    sh.state
        .pending
        .push_back(CommandKind::OpenProjectFile(project_path.to_path_buf()));
    let mut h = Harness::builder()
        .with_size(SHOT_SIZE)
        .with_pixels_per_point(1.0)
        .build_ui_state(frame, sh);
    h.run_steps(4);
    h
}

fn focus_model_camera(state: &mut State, model_name: &str, dir: [f64; 3], zoom_scale: f64) {
    let entity = model::find_by_name(&state.world, model_name)
        .unwrap_or_else(|| panic!("missing model {model_name}"));
    let facets = model::facets_of(&state.world, &entity).expect("model facets");
    let loc = model::transform_of(&state.world, &entity).unwrap_or_default();
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for tri in facets.iter() {
        for p in [tri.a, tri.b, tri.c] {
            let wp = [p.x + loc.pos.x, p.y + loc.pos.y, p.z + loc.pos.z];
            for axis in 0..3 {
                min[axis] = min[axis].min(wp[axis]);
                max[axis] = max[axis].max(wp[axis]);
            }
        }
    }
    let center = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    let dx = max[0] - min[0];
    let dy = max[1] - min[1];
    let dz = max[2] - min[2];
    let radius = ((dx * dx + dy * dy + dz * dz).sqrt() * 0.5).max(5.0);
    let mut cam = Camera::frame(center, radius).toward(dir);
    cam.dist *= zoom_scale;
    state.cam = cam;
}

fn hide_all_fixtures(state: &mut State) {
    for (name, entity) in model::model_entities(&state.world) {
        if model::is_fixture(&state.world, &entity) && !state.hidden_models.contains(&name) {
            state.hidden_models.push(name);
        }
    }
}

fn edit_and_generate(h: &mut Harness<'static, ShellUi>, op: &str, params: OpParams) {
    let st = &mut h.state_mut().state;
    st.pending.push_back(CommandKind::EditOperationParams {
        name: op.into(),
        params,
    });
    st.pending.push_back(CommandKind::GenerateOperation(op.into()));
    st.drain_actions();
    let entity = model::find_by_name(&st.world, op).expect("op entity");
    let moves = model::toolpath_of(&st.world, &entity).map_or(0, |tp| tp.path.moves.len());
    assert!(moves > 0, "regenerating {op} produced empty toolpath: {}", st.status);
    st.sync_ops_ui();
    h.run_steps(2);
}

fn read_op_params(h: &Harness<'_, ShellUi>, op: &str) -> OpParams {
    let entity = model::find_by_name(&h.state().state.world, op).expect("op entity");
    model::operation_of(&h.state().state.world, &entity)
        .expect("operation component")
        .params
}

fn paint_and_capture(h: &mut Harness<'static, ShellUi>) -> Vec<u8> {
    h.run_steps(2);
    let gpu = crate::headless_gpu::device().expect("headless software/GPU adapter required");
    let sh = &mut h.state_mut();
    let [w, height] = sh.viewport_points.map(|n| n.max(1.0).round() as u32);
    let mut view = View3D::new(&gpu.device, super::super::shell::VIEW3D_FORMAT, w, height);
    let scene = world::collect(&sh.state);
    view.set_document(&gpu.device, &gpu.queue, &scene.parts);
    view.set_overlay(&gpu.device, &gpu.queue, &scene.overlay);

    let (vw, vh) = view.extent();
    let mvp = sh.state.cam.mvp(f64::from(vw.max(1)) / f64::from(vh.max(1)));
    let mut enc = gpu
        .device
        .create_command_encoder(&egui_wgpu::wgpu::CommandEncoderDescriptor {
            label: Some("guide-images-3d"),
        });
    view.render(
        &gpu.queue,
        &mut enc,
        &mvp,
        sh.state.cam.eye_f32(),
        sh.state.display,
    );
    gpu.queue.submit([enc.finish()]);

    let (rgba, rw, rh) = view.snapshot_rgba(&gpu.device, &gpu.queue).expect("readback");
    let bg = rgba.first_chunk::<4>().copied().expect("non-empty readback");
    let non_bg = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| **px != bg)
        .count();
    assert!(non_bg > 200, "3D viewport rendered blank ({non_bg} non-bg pixels)");

    let vp_img = ColorImage::from_rgba_unmultiplied([rw as usize, rh as usize], &rgba);
    sh.viewport_tex
        .as_mut()
        .expect("shell initialized")
        .set(vp_img, TextureOptions::LINEAR);
    h.run_steps(2);

    let full = h.render().expect("egui shell render");
    let mut png = Vec::new();
    let encoder = PngEncoder::new_with_quality(&mut png, CompressionType::Best, FilterType::Adaptive);
    encoder
        .write_image(
            full.as_raw(),
            full.width(),
            full.height(),
            image::ExtendedColorType::Rgba8,
        )
        .expect("encode optimised PNG");
    assert!(
        (1000..=1400).contains(&full.width()) && full.height() > 400,
        "unexpected guide screenshot dimensions {}x{}",
        full.width(),
        full.height()
    );
    assert!(
        (5_000..=650_000).contains(&png.len()),
        "guide PNG {} bytes outside 5 KB..650 KB",
        png.len()
    );
    png
}

fn save_shot(dir: &Path, file: &str, bytes: &[u8]) {
    let path = dir.join(file);
    std::fs::write(&path, bytes).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

fn first_arc_cut_target(h: &Harness<'_, ShellUi>, op: &str, vertical_delta: bool) -> Option<[f64; 3]> {
    let world = &h.state().state.world;
    let entity = model::find_by_name(world, op)?;
    let tp = model::toolpath_of(world, &entity)?;
    tp.path.moves.iter().find_map(|m| {
        if m.arc.is_some() && ((m.from.z - m.to.z).abs() > 1e-4) == vertical_delta {
            Some([
                (m.from.x + m.to.x) * 0.5,
                (m.from.y + m.to.y) * 0.5,
                (m.from.z + m.to.z) * 0.5,
            ])
        } else {
            None
        }
    })
}

fn focus_op(h: &mut Harness<'static, ShellUi>, op: &str, tab: usize, show_all: bool) {
    let st = &mut h.state_mut().state;
    st.show_all_paths = show_all;
    st.op_tab = tab;
    st.pending.push_back(CommandKind::Select(Some(op.into())));
    actions::target(st, UiTarget::Operation(op.into()));
    st.drain_actions();
    st.sync_ops_ui();
}

#[test]
fn guide_image_manifest_has_expected_entries() {
    assert_eq!(GUIDE_FILES.len(), 13);
    for pair_prefix in ["op-2d-stepover", "op-2d-entry", "op-2d-lead"] {
        assert!(GUIDE_FILES.contains(&format!("{pair_prefix}-before.png").as_str()));
        assert!(GUIDE_FILES.contains(&format!("{pair_prefix}-after.png").as_str()));
    }
}

#[test]
#[ignore = "off-screen WGPU guide screenshot renderer invoked by `cargo xtask docs-images`"]
fn render_all_guide_images() {
    let _gpu = crate::headless_gpu::gpu_guard(); // #3605: paint_and_capture makes devices
    let _guard = snapshot_guard();
    let img_dir = out_dir();
    std::fs::create_dir_all(&img_dir).expect("create guide image output dir");
    let proj_dir = projects_dir();
    write_all_demo_projects(&proj_dir);

    let first_part = workspace_root().join("examples/first-part.camproj");
    assert!(first_part.is_file(), "missing {}", first_part.display());

    // 1. Setup overview & 2D pocket parameters from first-part.camproj
    {
        let mut h = new_harness(&first_part);
        let st = &mut h.state_mut().state;
        st.show_all_paths = true;
        actions::target(st, UiTarget::SetupNode(String::new(), SetupPage::Settings));
        focus_model_camera(st, "first-part", [0.68, -0.68, 0.52], 0.92);
        save_shot(&img_dir, "setup-overview.png", &paint_and_capture(&mut h));

        focus_op(&mut h, "2D Pocket1", SEC_PASSES, false);
        focus_model_camera(&mut h.state_mut().state, "first-part", [0.60, -0.72, 0.56], 0.85);
        save_shot(&img_dir, "op-2d-pocket-params.png", &paint_and_capture(&mut h));
    }

    // 2. Workholding fixture + contour/face toolpaths, plus 2D Lead-in before/after
    {
        let wh_proj = proj_dir.join("guide-workholding.camproj");
        let mut h = new_harness(&wh_proj);
        {
            let st = &mut h.state_mut().state;
            st.show_all_paths = true;
            st.pending
                .push_back(CommandKind::Select(Some("2D Contour1".into())));
            actions::target(st, UiTarget::SetupNode("Setup1".into(), SetupPage::Fixtures));
            st.drain_actions();
            focus_model_camera(st, "Workpiece", [0.64, -0.72, 0.46], 1.85);
            st.cam.target = [0.0, 22.0, 2.0];
        }
        save_shot(&img_dir, "workholding-vise-toolpath.png", &paint_and_capture(&mut h));

        // Hide fixtures so Lead-in before/after isolates part wall & contour path.
        hide_all_fixtures(&mut h.state_mut().state);
        focus_op(&mut h, "2D Contour1", SEC_LEADS, false);
        let base_contour = read_op_params(&h, "2D Contour1");
        let OpParams::Contour {
            stock_to_leave, clearance_z, retract_z, top_z, bottom_z, stepdown,
            feed, plunge_feed, climb, side, tolerance, tab_count, tab_width,
            tab_height, entry, helix_radius, blend_turns, blend_radius, links, tilt, ..
        } = base_contour
        else {
            panic!("2D Contour1 should be OpParams::Contour");
        };
        let contour_with_lead = |lead: Lead| OpParams::Contour {
            stock_to_leave, clearance_z, retract_z, top_z, bottom_z, stepdown,
            feed, plunge_feed, climb, side, lead, tolerance, tab_count, tab_width,
            tab_height, entry, helix_radius, blend_turns, blend_radius, links, tilt,
            chain_overrides: Vec::new(),
            picks: Default::default(),
        };
        edit_and_generate(&mut h, "2D Contour1", contour_with_lead(Lead::Arc { radius: 6.0 }));
        let lead_target = first_arc_cut_target(&h, "2D Contour1", false)
            .unwrap_or([45.0, -28.0, 20.0]);
        {
            let st = &mut h.state_mut().state;
            st.cam = Camera::frame(lead_target, 16.0).toward([0.52, -0.68, 0.52]);
        }
        let lead_after = paint_and_capture(&mut h);
        edit_and_generate(&mut h, "2D Contour1", contour_with_lead(Lead::None));
        let lead_before = paint_and_capture(&mut h);
        assert_ne!(lead_before, lead_after, "lead-in before/after renders must differ");
        save_shot(&img_dir, "op-2d-lead-before.png", &lead_before);
        save_shot(&img_dir, "op-2d-lead-after.png", &lead_after);
    }

    // 3. 2D Stepover & Entry before/after pairs on first-part.camproj
    {
        let mut h = new_harness(&first_part);
        let base = read_op_params(&h, "2D Pocket1");
        let pocket_cfg = |strategy, stepover, entry, helix_radius, links| {
            pocket_variant(&base, strategy, stepover, entry, helix_radius, links)
        };

        focus_op(&mut h, "2D Pocket1", SEC_PASSES, false);
        focus_model_camera(&mut h.state_mut().state, "first-part", [0.35, -0.52, 0.80], 0.72);
        edit_and_generate(&mut h, "2D Pocket1", pocket_cfg(PocketStrategy::Zigzag, 4.5, Entry3D::Helix, 1.2, Links::PLANAR));
        let step_before = paint_and_capture(&mut h);
        edit_and_generate(&mut h, "2D Pocket1", pocket_cfg(PocketStrategy::Zigzag, 1.8, Entry3D::Helix, 1.2, Links::PLANAR));
        let step_after = paint_and_capture(&mut h);
        assert_ne!(step_before, step_after, "stepover before/after renders must differ");
        save_shot(&img_dir, "op-2d-stepover-before.png", &step_before);
        save_shot(&img_dir, "op-2d-stepover-after.png", &step_after);

        focus_op(&mut h, "2D Pocket1", SEC_LEADS, false);
        let helix_links = Links { transition: Some(Entry3D::Helix), ..Links::PLANAR };
        edit_and_generate(&mut h, "2D Pocket1", pocket_cfg(PocketStrategy::Waterline, 3.0, Entry3D::Helix, 1.8, helix_links));
        let entry_target = first_arc_cut_target(&h, "2D Pocket1", true)
            .unwrap_or([-45.0, -25.0, 14.0]);
        {
            let st = &mut h.state_mut().state;
            st.cam = Camera::frame(entry_target, 14.0).toward([-0.55, -0.78, 0.34]);
        }
        let entry_after = paint_and_capture(&mut h);
        edit_and_generate(&mut h, "2D Pocket1", pocket_cfg(PocketStrategy::Waterline, 3.0, Entry3D::Plunge, 1.8, Links::PLANAR));
        let entry_before = paint_and_capture(&mut h);
        assert_ne!(entry_before, entry_after, "entry before/after renders must differ");
        save_shot(&img_dir, "op-2d-entry-before.png", &entry_before);
        save_shot(&img_dir, "op-2d-entry-after.png", &entry_after);
    }

    // 4. 3D roughing & finishing from guide-3d.camproj
    {
        let mut h = new_harness(&proj_dir.join("guide-3d.camproj"));
        focus_op(&mut h, "3D Pocket1", SEC_PASSES, false);
        focus_model_camera(&mut h.state_mut().state, "Mold", [0.65, -0.70, 0.52], 0.85);
        save_shot(&img_dir, "op-3d-pocket-roughing.png", &paint_and_capture(&mut h));

        focus_op(&mut h, "3D Finish1", SEC_PASSES, false);
        focus_model_camera(&mut h.state_mut().state, "Mold", [0.65, -0.70, 0.56], 0.82);
        save_shot(&img_dir, "op-3d-finish-parallel.png", &paint_and_capture(&mut h));
    }

    // 5. Continuous 5-axis Swarf & Indexed 3+2 from guide-multiaxis.camproj
    {
        let mut h = new_harness(&proj_dir.join("guide-multiaxis.camproj"));
        {
            let st = &mut h.state_mut().state;
            st.pending
                .push_back(CommandKind::ActivateSetup(Some("Setup1".into())));
            st.drain_actions();
            st.hidden_models = vec!["IndexedBlock".into()];
        }
        focus_op(&mut h, "Swarf1", SEC_PASSES, false);
        focus_model_camera(&mut h.state_mut().state, "Blade", [0.58, -0.75, 0.44], 0.82);
        save_shot(&img_dir, "op-multiaxis-swarf.png", &paint_and_capture(&mut h));

        {
            let st = &mut h.state_mut().state;
            st.pending
                .push_back(CommandKind::ActivateSetup(Some("Setup2".into())));
            st.drain_actions();
            st.hidden_models = vec!["Blade".into()];
        }
        focus_op(&mut h, "Indexed Face1", SEC_GEO, false);
        focus_model_camera(&mut h.state_mut().state, "IndexedBlock", [0.32, -0.82, 0.46], 0.86);
        save_shot(&img_dir, "op-indexed-3plus2.png", &paint_and_capture(&mut h));
    }
}

// Change only the settings illustrated by the before/after pair.
fn pocket_variant(base: &OpParams, strategy: PocketStrategy, stepover: f64,
    entry: Entry3D, helix_radius: f64, links: Links) -> OpParams {
    let OpParams::Pocket { stock_to_leave, clearance_z, retract_z, top_z, bottom_z,
        stepdown, feed, plunge_feed, climb, optimal_load, blend_turns,
        blend_radius, tilt, rest, ref corner_relief, .. } = *base else { panic!("expected guide pocket"); };
    OpParams::Pocket { pass_start: Default::default(), stock_to_leave, clearance_z, retract_z, top_z, bottom_z,
        stepdown, feed, plunge_feed, climb, optimal_load, blend_turns,
            picks: Default::default(),
        blend_radius, tilt, rest, corner_relief: corner_relief.clone(), strategy, stepover, entry, helix_radius, links }
}

#[test]
fn guide_pocket_variants_preserve_rest_parameters() {
    use camcraft_app::machining2d::RestParams;
    let world = flecs_ecs::prelude::World::new();
    camcraft_app::project::load_noted(&world, &workspace_root().join("examples/first-part.camproj")).unwrap();
    let entity = model::find_by_name(&world, "2D Pocket1").unwrap();
    let mut base = model::operation_of(&world, &entity).unwrap().params;
    for expected in [None, Some(RestParams { source: Default::default(), previous_tool_diameter: 12.0,
        previous_corner_radius: 1.0, previous_stock_to_leave: 0.2, allowance: 0.1 })] {
        let OpParams::Pocket { rest, .. } = &mut base else { panic!("guide pocket"); };
        *rest = expected;
        for strategy in [PocketStrategy::Zigzag, PocketStrategy::Waterline] {
            let variant = pocket_variant(&base, strategy, 3.0, Entry3D::Helix, 1.8, Links::PLANAR);
            let OpParams::Pocket { rest, .. } = variant else { panic!("variant pocket"); };
            assert_eq!(rest, expected);
        }
    }
}
