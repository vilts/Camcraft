//! #1863: a DXF profile is placed on the stock of a setup from the UI -
//! the Stock page's "Add DXF profile…" control and a viewport drop, the
//! units prompt for a drawing that states none, the profile list with its
//! placement, the browser row, the chains drawn on the stock top with
//! open ends marked, and the operation page on a setup with no model.
//! Structural (AccessKit labels, world state, overlay vertices); no pixels.
//! Provenance: our own tests over our own shell and the #1862 commands.
//!
//! Gate map (#1863): covers ui::state (state_sketch), ui::egui_app
//! (stock_profiles, dxf_units_dialog, file_drop, browser_tree, op_page),
//! ui::sketch_overlay.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::*;
use camcraft_app::sketch;
use camcraft_app::stock::StockShape;

use crate::state::DxfSource;

/// One closed 40 x 20 rectangle on OUTLINE and one open line on GUIDE.
fn drawing(insunits: Option<i32>) -> Vec<u8> {
    let mut s = String::new();
    if let Some(code) = insunits {
        s.push_str(&format!("0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n{code}\n0\nENDSEC\n"));
    }
    s.push_str(concat!(
        "0\nSECTION\n2\nENTITIES\n",
        "0\nLWPOLYLINE\n8\nOUTLINE\n90\n4\n70\n1\n",
        "10\n0.0\n20\n0.0\n",
        "10\n40.0\n20\n0.0\n",
        "10\n40.0\n20\n20.0\n",
        "10\n0.0\n20\n20.0\n",
        "0\nLINE\n8\nGUIDE\n10\n60.0\n20\n0.0\n11\n80.0\n21\n10.0\n",
        "0\nENDSEC\n0\nEOF\n",
    ));
    s.into_bytes()
}

fn bytes(name: &str, insunits: Option<i32>) -> DxfSource {
    DxfSource::Bytes { name: name.to_owned(), data: Arc::from(drawing(insunits)) }
}

/// A blank shell with a 100 x 50 x 20 box stock and no model.
fn stock_only() -> Harness<'static, ShellUi> {
    let mut h = Harness::builder().with_size(Vec2::new(1280.0, 900.0)).build_ui_state(frame, ShellUi::blank());
    {
        let st = &mut h.state_mut().state;
        let shape = StockShape::Box { dx: 100.0, dy: 50.0, dz: 20.0 };
        st.pending.push_back(CommandKind::SetStock { shape, pos: [0.0, 0.0, 0.0], yaw_deg: 0.0 });
        st.drain_actions();
    }
    h.run();
    h
}

fn place(h: &mut Harness<'static, ShellUi>, source: DxfSource, units: Option<Unit>) {
    h.state_mut().state.pending.push_back(CommandKind::ImportDxf { source, units });
    h.run();
}

#[test]
fn the_stock_page_adds_lists_places_and_deletes_a_dxf_profile() {
    let mut h = stock_only();
    h.state_mut().state.ui_target = UiTarget::Stock;
    h.run();
    assert!(h.query_by_label("Add DXF profile…").is_some(), "the Stock page offers the DXF door");
    place(&mut h, bytes("plate.dxf", Some(4)), None);
    let status = h.state().state.status.clone();
    assert!(sketch::find(&h.state().state.world, "plate").is_some(), "placed: {status}");
    for word in ["plate", "Millimeters", "2 layers", "1 closed", "1 open", "0 skipped"] {
        assert!(status.contains(word), "the summary names {word}: {status}");
    }
    h.run();
    assert!(h.query_all_by_label("plate").next().is_some(), "the profile list names the profile");
    for label in ["Profile X", "Profile Y", "Profile angle (deg)", "Place profile", "Hide profile", "Delete profile"] {
        assert!(h.query_by_label(label).is_some(), "the profile list shows {label}");
    }
    // Placement is a command: MoveSketch through the page's queue, undone by Ctrl+Z.
    h.state_mut().state.pending.push_back(CommandKind::MoveSketch { name: "plate".into(), x: 10.0, y: 5.0, angle_deg: 90.0 });
    h.run();
    let at = |h: &Harness<'static, ShellUi>| {
        let w = &h.state().state.world;
        sketch::find(w, "plate").and_then(|(e, _)| model::transform_of(w, &e)).and_then(sketch::face_placement)
    };
    assert_eq!(at(&h), Some((10.0, 5.0, 90.0)));
    h.state_mut().state.pending.push_back(CommandKind::Undo);
    h.run();
    assert_eq!(at(&h), Some((0.0, 0.0, 0.0)), "undo puts the profile back at the stock origin");
    h.get_by_label("Delete profile").click();
    h.run();
    assert!(sketch::find(&h.state().state.world, "plate").is_none(), "Delete removes the profile");
    h.state_mut().state.pending.push_back(CommandKind::Undo);
    h.run();
    assert!(sketch::find(&h.state().state.world, "plate").is_some(), "undo brings it back");
    h.state_mut().state.pending.push_back(CommandKind::Undo);
    h.run();
    assert!(sketch::find(&h.state().state.world, "plate").is_none(), "undo takes the import away");
}

#[test]
fn a_drawing_without_units_asks_millimeters_or_inches_and_never_guesses() {
    let mut h = stock_only();
    place(&mut h, bytes("free.dxf", None), None);
    assert!(sketch::find(&h.state().state.world, "free").is_none(), "nothing lands before the answer");
    assert!(h.state().state.dxf_units_prompt.is_some(), "the units prompt is open");
    h.run();
    for text in ["Millimeters", "Inches", "Cancel"] {
        assert!(h.query_by_label(text).is_some(), "the prompt offers {text}");
    }
    assert!(h.query_by_label_contains("80 x 20 mm").is_some(), "the size if read in millimeters");
    assert!(h.query_by_label_contains("80 x 20 in").is_some(), "the size if read in inches");
    h.get_by_label("Inches").click();
    h.run();
    let w = &h.state().state.world;
    let (_, s) = sketch::find(w, "free").expect("placed after the answer");
    let (lo, hi) = s.data.bounds().expect("bounds");
    assert!(((hi.x - lo.x) - 80.0 * 25.4).abs() < 1e-6, "read in inches");
    assert!(h.state().state.dxf_units_prompt.is_none(), "the prompt closes");
    h.state_mut().state.pending.push_back(CommandKind::Undo);
    h.run();
    assert!(sketch::find(&h.state().state.world, "free").is_none(), "one Ctrl+Z takes it away");

    // Cancel places nothing.
    place(&mut h, bytes("free.dxf", None), None);
    h.run();
    h.get_by_label("Cancel").click();
    h.run();
    assert!(sketch::find(&h.state().state.world, "free").is_none());
    assert!(h.state().state.dxf_units_prompt.is_none());
}

fn temp_dxf(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft-dxf-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("make the test folder");
    let p = d.join("drop.dxf");
    std::fs::write(&p, drawing(Some(4))).expect("write the drawing");
    p
}

#[derive(Debug)]
struct Fake(PathBuf);

impl egui::DroppedFile for Fake {
    fn path(&self) -> &Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

fn drop_file(h: &mut Harness<'static, ShellUi>, path: &Path) {
    h.input_mut().dropped_files.push(std::sync::Arc::new(Fake(path.to_path_buf())));
    h.run_steps(2);
    h.run();
}

#[test]
fn a_dropped_dxf_lands_on_the_stock_or_asks_for_one() {
    let path = temp_dxf("drop");
    let mut h = Harness::builder().with_size(Vec2::new(1280.0, 720.0)).build_ui_state(frame, ShellUi::blank());
    h.run();
    drop_file(&mut h, &path);
    let status = h.state().state.status.clone();
    assert!(sketch::sketches(&h.state().state.world).is_empty(), "no stock, no profile");
    assert!(status.contains("drop.dxf") && status.contains("make a stock first"), "refused by name: {status}");

    let mut h = stock_only();
    drop_file(&mut h, &path);
    assert!(sketch::find(&h.state().state.world, "drop").is_some(), "placed: {}", h.state().state.status);
    let _ = std::fs::remove_dir_all(path.parent().expect("folder"));
}

#[test]
fn the_browser_lists_the_profile_under_its_stock_with_an_eye() {
    let mut h = stock_only();
    place(&mut h, bytes("plate.dxf", Some(4)), None);
    let (rows, leaves) = super::super::browser_tree::browser_rows(&h.state().state);
    let ix = leaves
        .iter()
        .position(|l| matches!(l, panes::Leaf::Sketch(_, n) if n == "plate"))
        .expect("a profile row");
    let stock = leaves.iter().position(|l| matches!(l, panes::Leaf::Stock(_))).expect("stock row");
    assert!(ix > stock && rows[ix].depth == rows[stock].depth + 1, "the profile sits under its stock");
    assert!(rows[ix].suffix.contains("DXF profile"), "{}", rows[ix].suffix);
    assert!(painted::has_eye(rows[ix].kind), "the row can hide");
    super::super::browser_pane::eye(&mut h.state_mut().state, &leaves[ix]);
    assert!(h.state().state.hidden_models.iter().any(|n| n == "plate"), "the eye hides it");
    super::super::browser_pane::click(&mut h.state_mut().state, leaves[ix].clone());
    h.run();
    assert!(matches!(h.state().state.ui_target, UiTarget::Stock), "the row opens the Stock page");
    h.state_mut().state.pending.push_back(CommandKind::DeleteSelected);
    h.run();
    assert!(sketch::find(&h.state().state.world, "plate").is_none(), "Del removes the selected profile");
}

#[test]
fn the_viewport_draws_the_chains_on_the_stock_top_and_marks_open_ends() {
    let mut h = stock_only();
    place(&mut h, bytes("plate.dxf", Some(4)), None);
    let st = &h.state().state;
    let top = model::stock_world_aabb_for(&st.world, None).expect("stock").max.z as f32;
    let lines = crate::sketch_overlay::sketch_lines(st);
    let curve: Vec<_> = lines.iter().filter(|v| v[3..] == crate::sketch_overlay::SKETCH_RGB).collect();
    let marks: Vec<_> = lines.iter().filter(|v| v[3..] == crate::sketch_overlay::OPEN_END_RGB).collect();
    assert!(curve.len() >= 10, "4 rectangle edges and the open line: {}", curve.len());
    assert!(curve.iter().all(|v| (v[2] - top).abs() < 1e-3), "on the stock top at z {top}");
    assert!(!marks.is_empty(), "the open line's ends are marked");
    assert_ne!(crate::sketch_overlay::SKETCH_RGB, crate::sketch_overlay::OPEN_END_RGB);
    let near = |x: f32, y: f32| marks.iter().any(|v| (v[0] - x).abs() < 3.0 && (v[1] - y).abs() < 3.0);
    let (_, s) = sketch::find(&st.world, "plate").expect("profile");
    let open = &s.data.chains().open[0];
    let pose = sketch::world_pose(&st.world, &sketch::find(&st.world, "plate").expect("p").0).expect("pose");
    for p in [open.start, open.end] {
        let w = camcraft_app::geometry::transform_point(camcraft_app::geometry::Point3::new(p.x, p.y, 0.0), pose.pos, pose.rot);
        assert!(near(w.x as f32, w.y as f32), "a mark at {w:?}");
    }
    assert!(interact::collect_overlay(st).len() >= lines.len(), "the overlay carries the profile");
    h.state_mut().state.hidden_models.push("plate".into());
    assert!(crate::sketch_overlay::sketch_lines(&h.state().state).is_empty(), "a hidden profile is not drawn");
}

#[test]
fn the_operation_page_takes_the_profile_as_its_source_without_a_model() {
    let mut h = stock_only();
    place(&mut h, bytes("plate.dxf", Some(4)), None);
    rack(&mut h.state_mut().state, &["EM-0400-2FL"]);
    {
        let st = &mut h.state_mut().state;
        let tool = model::named_entities(&st.world)
            .into_iter()
            .find(|(_, e)| model::is_tool(&st.world, e))
            .map(|(n, _)| n)
            .expect("a tool");
        let kind = camcraft_app::operation::KindInfo::ALL
            .iter()
            .position(|k| k.label == "2D Contour")
            .expect("2D Contour kind");
        let params = camcraft_app::operation::seed_params(kind);
        let sources = super::super::op_source::source_items(&st.world, None, &params);
        assert_eq!(sources, vec!["Stock".to_owned(), "plate".to_owned()], "the profile is a source");
        st.pending.push_back(CommandKind::CreateOperation { name: "cut".into(), tool, src: SrcRef::Entity("plate".into()), params });
        st.drain_actions();
        assert!(model::find_by_name(&st.world, "cut").is_some(), "created: {}", st.status);
        st.ui_target = UiTarget::Operation("cut".into());
    }
    h.run();
    h.run();
    assert_eq!(h.state().state.op_ui.src_name, "plate", "the page keeps the profile source");
    for refusal in ["no model loaded", "needs a solid model", "is not a usable source"] {
        assert!(h.query_by_label_contains(refusal).is_none(), "no '{refusal}' refusal on the page");
    }
    assert!(!h.state().state.status.contains("refused"), "{}", h.state().state.status);
}

#[test]
fn a_still_frame_builds_no_profile_lines_and_a_move_builds_once() {
    let mut h = stock_only();
    place(&mut h, bytes("plate.dxf", Some(4)), None);
    let builds = |h: &Harness<'static, ShellUi>| h.state().state.frame_cache.sketches.builds();
    let _ = crate::sketch_overlay::sketch_lines(&h.state().state);
    let first = builds(&h);
    assert!(first >= 1, "the first draw builds the lines");
    h.run_steps(3);
    let _ = crate::sketch_overlay::sketch_lines(&h.state().state);
    assert_eq!(builds(&h), first, "a still frame builds nothing");
    h.state_mut().state.pending.push_back(CommandKind::MoveSketch { name: "plate".into(), x: 5.0, y: 0.0, angle_deg: 0.0 });
    h.run();
    let moved = crate::sketch_overlay::sketch_lines(&h.state().state);
    assert_eq!(builds(&h), first + 1, "a move builds the lines again, once");
    let _ = crate::sketch_overlay::sketch_lines(&h.state().state);
    assert_eq!(builds(&h), first + 1, "and then reads the cache");
    assert!(!moved.is_empty());
}

#[test]
fn more_than_32_visible_profiles_build_nothing_on_a_still_frame() {
    let mut h = stock_only();
    for index in 0..33 {
        h.state_mut()
            .state
            .pending
            .push_back(CommandKind::ImportDxf {
                source: bytes(&format!("plate-{index}.dxf"), Some(4)),
                units: None,
            });
    }
    h.state_mut().state.drain_actions();
    let builds = |h: &Harness<'static, ShellUi>| h.state().state.frame_cache.sketches.builds();
    let lines = crate::sketch_overlay::sketch_lines(&h.state().state);
    let first = builds(&h);
    assert_eq!(first, 33, "each visible profile builds once");
    assert!(!lines.is_empty());
    for _ in 0..3 {
        let unchanged = crate::sketch_overlay::sketch_lines(&h.state().state);
        assert_eq!(unchanged, lines);
        assert_eq!(builds(&h), first, "all visible profiles stay cached");
    }
    h.state_mut()
        .state
        .pending
        .push_back(CommandKind::MoveSketch {
            name: "plate-0".into(),
            x: 5.0,
            y: 0.0,
            angle_deg: 0.0,
        });
    h.state_mut().state.drain_actions();
    let _ = crate::sketch_overlay::sketch_lines(&h.state().state);
    assert_eq!(builds(&h), first + 1, "only the moved profile rebuilds");
    let _ = crate::sketch_overlay::sketch_lines(&h.state().state);
    assert_eq!(
        builds(&h),
        first + 1,
        "the next unchanged frame builds nothing"
    );
}

#[test]
fn operation_page_source_filters_profiles_to_admitted_kinds_and_keeps_selection() {
    use camcraft_app::commands::ImportModel;
    use camcraft_app::geometry::{Point3, cube};
    use camcraft_app::operation::{FeatureSel, GeoSel, OpType};

    let mut h = stock_only();
    place(&mut h, bytes("plate.dxf", Some(4)), None);
    rack(&mut h.state_mut().state, &["EM-0400-2FL"]);
    {
        let st = &mut h.state_mut().state;
        st.history.execute(
            &st.world,
            ImportModel::from_mesh("part", cube(Point3::new(0.0, 0.0, 0.0), Point3::new(30.0, 30.0, 15.0))),
        );
        let contour = camcraft_app::operation::seed_params(OpType::Contour.ordinal());
        let pocket3d = camcraft_app::operation::seed_params(OpType::Pocket3D.ordinal());
        assert_eq!(
            super::super::op_source::source_items(&st.world, None, &contour),
            vec!["Stock".to_owned(), "part".to_owned(), "plate".to_owned()],
            "2D admitted kind lists parts and placed profiles once"
        );
        assert_eq!(
            super::super::op_source::source_items(&st.world, None, &pocket3d),
            vec!["Stock".to_owned(), "part".to_owned()],
            "3D kind omits DXF profiles from the Source list"
        );
        let tool = model::named_entities(&st.world)
            .into_iter()
            .find(|(_, e)| model::is_tool(&st.world, e))
            .map(|(n, _)| n)
            .expect("a tool");
        for (name, params) in [("Pocket3d1", pocket3d), ("Contour1", contour)] {
            st.pending.push_back(CommandKind::CreateOperation {
                name: name.into(),
                tool: tool.clone(),
                src: SrcRef::Entity("part".into()),
                params,
            });
            st.drain_actions();
            assert!(model::find_by_name(&st.world, name).is_some(), "created {name}: {}", st.status);
        }
        st.ui_target = UiTarget::Operation("Pocket3d1".into());
    }
    h.run_steps(2);

    let plate_nodes = |h: &Harness<'static, ShellUi>| h.query_all_by(|n| n.label().as_deref() == Some("plate")).count();
    let baseline = plate_nodes(&h);
    let click_combo = |h: &mut Harness<'static, ShellUi>| {
        h.get_by(|n| n.role() == egui::accesskit::Role::ComboBox && n.value().as_deref() == Some("part")).click();
        h.run_steps(2);
    };
    click_combo(&mut h);
    assert_eq!(plate_nodes(&h), baseline, "3D Pocket Source dropdown must not offer DXF profile 'plate'");
    // Close the Source popup before switching operations (they share the "op_src" salt).
    click_combo(&mut h);
    h.state_mut().state.ui_target = UiTarget::Operation("Contour1".into());
    h.run_steps(2);
    assert_eq!(plate_nodes(&h), baseline);
    click_combo(&mut h);
    assert_eq!(plate_nodes(&h), baseline + 1, "2D Contour Source dropdown offers DXF profile 'plate' once");
    click_combo(&mut h);
    let face_id = {
        let st = &h.state().state;
        let part = model::find_by_name(&st.world, "part").expect("part");
        model::features_of(&st.world, &part).expect("part features").faces[0].id
    };
    h.state_mut().state.pending.push_back(CommandKind::SetOperationGeometry {
        name: "Contour1".into(),
        items: vec![GeoSel { entity: "part".into(), feature: FeatureSel::Face(face_id) }],
    });
    h.run_steps(3);
    assert!(
        h.query_by(|n| n.role() == egui::accesskit::Role::ComboBox && n.value().as_deref() == Some("Selection")).is_some(),
        "Selection stays shown in the merged header"
    );
}
