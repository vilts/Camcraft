//! #2201 reproduction & pin: selected edge/patch highlight draw count, build
//! count across idle/orbit/hover frames, and multi-op 45k-tri UI stability.
//! Provenance: our own reproduction fixture and shape/build-count assertions over
//! `shell_layout`, `pick_ink`, `pick_highlight`, and `world::collect`.

use camcraft_app::commands::{ImportModel, LoadMachine};
use camcraft_app::geometry::{Mesh, Point3, Triangle, cylinder, feature};
use camcraft_app::operation::{FeatureSel, GeoSel, OpType, SrcRef};

use super::*;

/// Closed watertight extruded rounded-lobe prism (~4,300+ triangles) whose top
/// rim splits into distinct multi-chord curved runs and straight stretches in
/// `feature::edge_runs`.
fn chamfered_multi_run_solid() -> Mesh {
    let mut ring: Vec<[f64; 2]> = Vec::new();
    let lobes = [
        ([20.0, 0.0], -60.0_f64, 60.0_f64, 15.0_f64),
        ([0.0, 30.0], 60.0_f64, 180.0_f64, 15.0_f64),
        ([-20.0, 0.0], 180.0_f64, 300.0_f64, 15.0_f64),
    ];
    for (center, a0_deg, a1_deg, r) in lobes {
        let steps = 30usize;
        for k in 0..=steps {
            let deg = a0_deg + (a1_deg - a0_deg) * (k as f64) / (steps as f64);
            let a = deg.to_radians();
            ring.push([center[0] + r * a.cos(), center[1] + r * a.sin()]);
        }
    }
    let n = ring.len();
    let z_levels = [0.0_f64, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 19.0, 20.0];
    let mut tris = Vec::new();
    for w in z_levels.windows(2) {
        let (z0, z1) = (w[0], w[1]);
        let inset0 = if z0 >= 18.0 { (z0 - 18.0) * 0.5 } else { 0.0 };
        let inset1 = if z1 >= 18.0 { (z1 - 18.0) * 0.5 } else { 0.0 };
        let pt = |i: usize, z: f64, inset: f64| -> Point3 {
            let [x, y] = ring[i];
            let scale = if inset > 0.0 {
                let d = (x * x + y * y).sqrt().max(1.0);
                (d - inset) / d
            } else {
                1.0
            };
            Point3::new(x * scale, y * scale, z)
        };
        for i in 0..n {
            let j = (i + 1) % n;
            let a0 = pt(i, z0, inset0);
            let b0 = pt(j, z0, inset0);
            let b1 = pt(j, z1, inset1);
            let a1 = pt(i, z1, inset1);
            tris.push(Triangle::new(a0, b0, b1));
            tris.push(Triangle::new(a0, b1, a1));
        }
    }
    let bot_c = Point3::new(0.0, 10.0, 0.0);
    let top_c = Point3::new(0.0, 10.0, 20.0);
    let top_inset = 1.0_f64;
    for i in 0..n {
        let j = (i + 1) % n;
        let ba = Point3::new(ring[i][0], ring[i][1], 0.0);
        let bb = Point3::new(ring[j][0], ring[j][1], 0.0);
        tris.push(Triangle::new(bot_c, bb, ba));

        let scale_i = ((ring[i][0] * ring[i][0] + ring[i][1] * ring[i][1]).sqrt().max(1.0) - top_inset)
            / (ring[i][0] * ring[i][0] + ring[i][1] * ring[i][1]).sqrt().max(1.0);
        let scale_j = ((ring[j][0] * ring[j][0] + ring[j][1] * ring[j][1]).sqrt().max(1.0) - top_inset)
            / (ring[j][0] * ring[j][0] + ring[j][1] * ring[j][1]).sqrt().max(1.0);
        let ta = Point3::new(ring[i][0] * scale_i, ring[i][1] * scale_i, 20.0);
        let tb = Point3::new(ring[j][0] * scale_j, ring[j][1] * scale_j, 20.0);
        tris.push(Triangle::new(top_c, ta, tb));
    }
    Mesh::from_facets(tris)
}

fn count_pick_highlight_shapes(h: &Harness<'static, ShellUi>) -> usize {
    let pick_rgb = crate::egui_app::ground_ink::overlay(crate::egui_app::ground_ink::Overlay::Pick);
    let pick_color = egui::Color32::from_rgb(
        (pick_rgb[0] * 255.0).clamp(0.0, 255.0) as u8,
        (pick_rgb[1] * 255.0).clamp(0.0, 255.0) as u8,
        (pick_rgb[2] * 255.0).clamp(0.0, 255.0) as u8,
    );
    let halo_color = egui::Color32::from_black_alpha(crate::pick_ink::HALO_ALPHA);
    let ([vx0, vy0], [vx1, vy1]) = h.state().state.viewport_rect;
    let vp_rect = egui::Rect::from_min_max(egui::pos2(vx0, vy0), egui::pos2(vx1, vy1));

    fn walk(shape: &egui::Shape, pick_color: egui::Color32, halo_color: egui::Color32) -> usize {
        match shape {
            egui::Shape::LineSegment { stroke, .. } => usize::from(
                (stroke.color == pick_color && (stroke.width - crate::pick_ink::SELECTED_PX).abs() < 0.01)
                    || (stroke.color == halo_color
                        && (stroke.width - (crate::pick_ink::SELECTED_PX + crate::pick_ink::HALO_EXTRA_PX)).abs()
                            < 0.01),
            ),
            egui::Shape::Path(p) => usize::from(
                (p.stroke.color == egui::epaint::ColorMode::Solid(pick_color)
                    && (p.stroke.width - crate::pick_ink::SELECTED_PX).abs() < 0.01)
                    || (p.stroke.color == egui::epaint::ColorMode::Solid(halo_color)
                        && (p.stroke.width - (crate::pick_ink::SELECTED_PX + crate::pick_ink::HALO_EXTRA_PX)).abs()
                            < 0.01),
            ),
            egui::Shape::Mesh(m) => {
                usize::from(m.vertices.iter().any(|v| v.color == pick_color || v.color == halo_color))
            }
            egui::Shape::Vec(v) => v.iter().map(|s| walk(s, pick_color, halo_color)).sum(),
            _ => 0,
        }
    }

    h.output()
        .shapes
        .iter()
        .filter(|cs| cs.clip_rect.intersects(vp_rect))
        .map(|cs| walk(&cs.shape, pick_color, halo_color))
        .sum()
}

fn setup_adaptive_with_part(mesh: Mesh) -> (ShellUi, Vec<feature::Chain>) {
    let mut sh = ShellUi::blank();
    let st = &mut sh.state;
    st.history.execute(&st.world, ImportModel::from_mesh("Part", mesh));
    st.doc_version += 1;
    let tool = camcraft_app::tools::catalog().into_iter().find(|(n, _)| *n == "EM-0800-4FL").expect("tool").1;
    st.pending.push_back(CommandKind::StockFit { margin: 2.0, shape: 0 });
    st.pending.push_back(CommandKind::AddTool { name: "EM-0800-4FL".into(), tool });
    let adaptive_params = crate::operations::quick_params(OpType::Adaptive2D.ordinal());
    st.pending.push_back(CommandKind::CreateOperation {
        name: "2D Adaptive1".into(),
        tool: "EM-0800-4FL".into(),
        src: SrcRef::Stock,
        params: adaptive_params,
    });
    st.pending.push_back(CommandKind::Select(Some("2D Adaptive1".into())));
    st.pending.push_back(CommandKind::FrameAll);
    st.settle();
    st.ui_target = UiTarget::Operation("2D Adaptive1".into());
    st.geo_pick = GeoPick::Edge;
    let part = model::find_by_name(&st.world, "Part").expect("Part");
    let features = model::features_of(&st.world, &part).expect("features");
    let mut runs = feature::edge_runs(&features);
    runs.sort_by_key(|a| std::cmp::Reverse(a.edges.len()));
    (sh, runs)
}

#[test]
fn three_selected_edge_pieces_draw_as_a_single_batched_highlight() {
    let (mut sh, runs) = setup_adaptive_with_part(chamfered_multi_run_solid());
    assert!(runs.len() >= 9, "expected >=9 edge runs, got {}", runs.len());
    let picks3: Vec<GeoSel> =
        runs.iter().take(3).map(|r| GeoSel { entity: "Part".into(), feature: FeatureSel::Chain(r.id) }).collect();
    sh.state.pending.push_back(CommandKind::SetOperationGeometry { name: "2D Adaptive1".into(), items: picks3 });
    sh.state.settle();
    sh.state.sync_ops_ui();

    let mut h = Harness::builder().with_size(Vec2::new(1280.0, 720.0)).build_ui_state(frame, sh);
    h.run();
    let draws3 = count_pick_highlight_shapes(&h);
    assert!((1..=2).contains(&draws3), "3 pieces must draw as 1-2 batched shapes, got {draws3}");

    let picks9: Vec<GeoSel> =
        runs.into_iter().take(9).map(|r| GeoSel { entity: "Part".into(), feature: FeatureSel::Chain(r.id) }).collect();
    {
        let st = &mut h.state_mut().state;
        st.pending.push_back(CommandKind::SetOperationGeometry { name: "2D Adaptive1".into(), items: picks9 });
        st.settle();
        st.sync_ops_ui();
    }
    h.step();
    let draws9 = count_pick_highlight_shapes(&h);
    assert!((1..=2).contains(&draws9), "9 pieces must draw as 1-2 batched shapes, got {draws9}");
}

#[test]
fn selected_highlight_builds_once_at_selection_and_zero_times_across_orbit_idle_and_hover() {
    let (sh, runs) = setup_adaptive_with_part(chamfered_multi_run_solid());
    let mut h = Harness::builder().with_size(Vec2::new(1280.0, 720.0)).build_ui_state(frame, sh);
    h.run();
    assert_eq!(crate::pick_highlight::selection_builds(&h.state().state), 0, "0 picks -> 0 builds");
    let picks: Vec<GeoSel> =
        runs.into_iter().take(3).map(|r| GeoSel { entity: "Part".into(), feature: FeatureSel::Chain(r.id) }).collect();
    {
        let st = &mut h.state_mut().state;
        st.pending.push_back(CommandKind::SetOperationGeometry { name: "2D Adaptive1".into(), items: picks });
        st.settle();
        st.sync_ops_ui();
    }
    h.step();
    assert_eq!(crate::pick_highlight::selection_builds(&h.state().state), 1, "1 build on select");
    assert!((1..=2).contains(&count_pick_highlight_shapes(&h)));
    let ([x0, y0], [x1, y1]) = h.state().state.viewport_rect;
    let center = egui::pos2(0.5 * (x0 + x1), 0.5 * (y0 + y1));
    for i in 0..5 {
        if i % 2 == 0 {
            h.state_mut().state.cam.orbit(12.0, -4.0);
        } else {
            h.input_mut().events.push(Event::PointerMoved(center + egui::vec2(i as f32 * 5.0, 0.0)));
        }
        h.step();
        assert_eq!(crate::pick_highlight::selection_builds(&h.state().state), 1, "0 extra builds on frame {i}");
        assert!((1..=2).contains(&count_pick_highlight_shapes(&h)));
    }
}

#[test]
fn selected_welded_patch_draws_as_batched_highlight_with_single_build() {
    let mut sh = ShellUi::blank();
    let patch_id = {
        let st = &mut sh.state;
        st.history.execute(&st.world, ImportModel::from_mesh("Cyl", cylinder(60.0, 20.0, 48)));
        st.doc_version += 1;
        let tool = camcraft_app::tools::catalog()
            .into_iter()
            .find(|(n, _)| *n == "EM-0800-4FL")
            .expect("EM-0800-4FL")
            .1;
        st.pending.push_back(CommandKind::StockFit { margin: 2.0, shape: 0 });
        st.pending.push_back(CommandKind::AddTool { name: "EM-0800-4FL".into(), tool });
        let params = crate::operations::quick_params(OpType::Contour.ordinal());
        st.pending.push_back(CommandKind::CreateOperation {
            name: "Contour1".into(),
            tool: "EM-0800-4FL".into(),
            src: SrcRef::Stock,
            params,
        });
        st.pending.push_back(CommandKind::Select(Some("Contour1".into())));
        st.pending.push_back(CommandKind::FrameAll);
        st.settle();
        let cyl = model::find_by_name(&st.world, "Cyl").expect("Cyl");
        let features = model::features_of(&st.world, &cyl).expect("features");
        model::patches(&features).first().expect("welded patch").id
    };
    sh.state.ui_target = UiTarget::Operation("Contour1".into());
    sh.state.pending.push_back(CommandKind::SetOperationGeometry {
        name: "Contour1".into(),
        items: vec![GeoSel { entity: "Cyl".into(), feature: FeatureSel::Patch(patch_id) }],
    });
    sh.state.settle();
    sh.state.sync_ops_ui();

    let mut h = Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, sh);
    h.run();
    assert_eq!(crate::pick_highlight::selection_builds(&h.state().state), 1);
    let draws = count_pick_highlight_shapes(&h);
    assert!((1..=2).contains(&draws), "welded patch must draw as 1-2 batched shapes, got {draws}");

    for _ in 0..3 {
        h.state_mut().state.cam.orbit(8.0, 3.0);
        h.step();
        assert_eq!(crate::pick_highlight::selection_builds(&h.state().state), 1);
    }
}

fn dense_stacked_cylinder() -> Mesh {
    let segs = 120usize;
    let bands = 188usize;
    let r = 30.0_f64;
    let h = 12.0_f64;
    let ring: Vec<[f64; 2]> = (0..segs)
        .map(|i| {
            let a = std::f64::consts::TAU * (i as f64) / (segs as f64);
            [r * a.cos(), r * a.sin()]
        })
        .collect();
    let mut tris = Vec::with_capacity(segs * 2 * (bands + 1));
    for b in 0..bands {
        let z0 = h * (b as f64) / (bands as f64);
        let z1 = h * ((b + 1) as f64) / (bands as f64);
        for i in 0..segs {
            let j = (i + 1) % segs;
            let a0 = Point3::new(ring[i][0], ring[i][1], z0);
            let b0 = Point3::new(ring[j][0], ring[j][1], z0);
            let b1 = Point3::new(ring[j][0], ring[j][1], z1);
            let a1 = Point3::new(ring[i][0], ring[i][1], z1);
            tris.push(Triangle::new(a0, b0, b1));
            tris.push(Triangle::new(a0, b1, a1));
        }
    }
    let bot_c = Point3::new(0.0, 0.0, 0.0);
    let top_c = Point3::new(0.0, 0.0, h);
    for i in 0..segs {
        let j = (i + 1) % segs;
        let ba = Point3::new(ring[i][0], ring[i][1], 0.0);
        let bb = Point3::new(ring[j][0], ring[j][1], 0.0);
        tris.push(Triangle::new(bot_c, bb, ba));
        let ta = Point3::new(ring[i][0], ring[i][1], h);
        let tb = Point3::new(ring[j][0], ring[j][1], h);
        tris.push(Triangle::new(top_c, ta, tb));
    }
    Mesh::from_facets(tris)
}

#[test]
fn dense_45k_model_with_multiple_ops_and_many_limit_findings_stays_stable_across_frames() {
    let mesh = dense_stacked_cylinder();
    assert!(mesh.facets.len() >= 45_000, "expected >=45k tris, got {}", mesh.facets.len());
    let mut sh = ShellUi::blank();
    {
        let st = &mut sh.state;
        st.history.execute(&st.world, ImportModel::from_mesh("Dense", mesh));
        st.doc_version += 1;
        let tool = camcraft_app::tools::catalog()
            .into_iter()
            .find(|(n, _)| *n == "EM-0600-4FL")
            .expect("EM-0600-4FL")
            .1;
        st.pending.push_back(CommandKind::StockFit { margin: 2.0, shape: 0 });
        st.pending.push_back(CommandKind::AddTool { name: "EM-0600-4FL".into(), tool });
        let tight = camcraft_app::machine::Machine {
            model: "TightTravel".into(),
            axes: vec![
                camcraft_app::machine::AxisLimits { name: "X".into(), travel: Some((0.0, 5.0)), ..Default::default() },
                camcraft_app::machine::AxisLimits { name: "Y".into(), travel: Some((0.0, 5.0)), ..Default::default() },
                camcraft_app::machine::AxisLimits { name: "Z".into(), travel: Some((-100.0, 100.0)), ..Default::default() },
            ],
            ..Default::default()
        };
        let cmd = LoadMachine::new(&st.world, "tight.mch".into(), Some(tight)).expect("load machine");
        st.history.execute(&st.world, cmd);
        st.doc_version += 1;

        let face_params = crate::operations::quick_params(OpType::Face.ordinal());
        let mut contour_params = crate::operations::quick_params(OpType::Contour.ordinal());
        if let camcraft_app::operation::OpParams::Contour { stepdown, .. } = &mut contour_params {
            *stepdown = 1.0;
        }
        let dense = model::find_by_name(&st.world, "Dense").expect("Dense");
        let features = model::features_of(&st.world, &dense).expect("features");
        let rim = feature::edge_runs(&features)
            .into_iter()
            .filter(|r| r.closed)
            .max_by_key(|r| r.edges.len())
            .expect("closed rim run");
        st.pending.push_back(CommandKind::CreateOperation {
            name: "Face1".into(),
            tool: "EM-0600-4FL".into(),
            src: SrcRef::Stock,
            params: face_params,
        });
        st.pending.push_back(CommandKind::CreateOperation {
            name: "2D Contour1".into(),
            tool: "EM-0600-4FL".into(),
            src: SrcRef::Selection(vec![GeoSel { entity: "Dense".into(), feature: FeatureSel::Chain(rim.id) }]),
            params: contour_params,
        });
        st.pending.push_back(CommandKind::GenerateOperation("Face1".into()));
        st.pending.push_back(CommandKind::GenerateOperation("2D Contour1".into()));
        st.pending.push_back(CommandKind::Select(Some("2D Contour1".into())));
        st.pending.push_back(CommandKind::FrameAll);
        st.settle();
        st.ui_target = UiTarget::Operation("2D Contour1".into());
        st.sync_ops_ui();

        let face_ent = model::find_by_name(&st.world, "Face1").expect("Face1");
        let op_ent = model::find_by_name(&st.world, "2D Contour1").expect("Contour1");
        assert!(model::toolpath_nonempty(&st.world, &face_ent), "Face1 path: {}", st.status);
        assert!(model::toolpath_nonempty(&st.world, &op_ent), "2D Contour1 path: {}", st.status);
        let (_, mounted) = camcraft_app::machine::of(&st.world);
        let check = camcraft_app::machine::check_document(&st.world, mounted.as_ref().expect("machine"))
            .expect("machine check");
        let total = check.problems.len() + check.more;
        assert!(total > 600, "expected >600 machine limit findings, got {total}");
    }

    let mut h = Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, sh);
    h.run();

    let builds_before = crate::pick_highlight::selection_builds(&h.state().state);
    assert_eq!(builds_before, 1);
    for _ in 0..5 {
        h.step();
    }
    assert_eq!(crate::pick_highlight::selection_builds(&h.state().state), 1);
    assert!((1..=2).contains(&count_pick_highlight_shapes(&h)));
    assert!(
        crate::egui_app::view_pass::state_redraw_delay(
            &h.state().state,
            std::time::Duration::MAX,
            false,
        )
        .is_none(),
        "idle 45k multi-op document must not request continuous repaints"
    );
}
