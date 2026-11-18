//! #744 in the shell: creating, DXF-importing, editing, and DXF-exporting
//! spun/extruded profile fixtures, plus preserving STEP fixtures as source with
//! optional reference profile derivation. Driven through kittest `Harness`.
//! Provenance: clean-room UI tests for #744.

use std::path::Path;

use egui::Vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use camcraft_app::commands::{ImportFixture, ImportModel};
use camcraft_app::fixture;
use camcraft_app::fixture_profile::{ExtrudeAxis, FixtureProfile};
use camcraft_app::geometry::Triangle;
use camcraft_app::model;
use camcraft_app::step_source;

use crate::state::{CommandKind, FixtureLibDialog, ProfileKindChoice, State};

use super::fixture_library_dialog::window;

fn dialog(state: State) -> Harness<'static, State> {
    Harness::builder().with_size(Vec2::new(900.0, 1000.0)).build_ui_state(
        |ui, st: &mut State| {
            st.drain_actions();
            window(ui.ctx(), st);
        },
        state,
    )
}

fn page_harness(state: State) -> Harness<'static, State> {
    Harness::builder().with_size(Vec2::new(700.0, 1200.0)).build_ui_state(
        |ui, st: &mut State| {
            st.drain_actions();
            egui::ScrollArea::vertical().show(ui, |ui| super::pages::fixtures_section(ui, st, None));
        },
        state,
    )
}

fn box_step_bytes() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/box.step");
    std::fs::read(&path).expect("read box.step")
}

fn spun_dxf_bytes() -> Vec<u8> {
    concat!(
        "0\nSECTION\n2\nENTITIES\n",
        "0\nLINE\n8\nCENTRELINE\n10\n-5.0\n20\n0.0\n11\n45.0\n21\n0.0\n",
        "0\nLWPOLYLINE\n8\nPROFILE\n90\n4\n70\n0\n",
        "10\n0.0\n20\n0.0\n",
        "10\n0.0\n20\n30.0\n",
        "10\n20.0\n20\n30.0\n",
        "10\n20.0\n20\n0.0\n",
        "0\nENDSEC\n0\nEOF\n",
    )
    .as_bytes()
    .to_vec()
}

fn extruded_dxf_bytes() -> Vec<u8> {
    concat!(
        "0\nSECTION\n2\nENTITIES\n",
        "0\nLWPOLYLINE\n8\nOUTLINE\n90\n4\n70\n1\n",
        "10\n0.0\n20\n0.0\n",
        "10\n50.0\n20\n0.0\n",
        "10\n50.0\n20\n25.0\n",
        "10\n0.0\n20\n25.0\n",
        "0\nTEXT\n8\nDEPTH\n1\nDEPTH=15\n",
        "0\nENDSEC\n0\nEOF\n",
    )
    .as_bytes()
    .to_vec()
}

#[test]
fn fixtures_page_button_opens_profile_dialog_and_inserts_dxf_fixtures() {
    let mut h = page_harness(State::new());
    h.run();
    h.get_by_label("Add DXF / profile…").click();
    h.run();
    assert!(matches!(
        &h.state().fixture_lib,
        Some(FixtureLibDialog::Profile(d)) if d.setup.is_empty()
    ));

    // Open dialog directly and load an extruded DXF profile along Y.
    let mut st = State::new();
    st.pending.push_back(CommandKind::ProfileFixtureDialog(String::new()));
    let mut h = dialog(st);
    h.run();
    if let Some(FixtureLibDialog::Profile(d)) = h.state_mut().fixture_lib.as_mut() {
        d.kind = ProfileKindChoice::Extruded;
        d.axis = ExtrudeAxis::Y;
    }
    h.state_mut().load_profile_dxf_bytes("Vise jaw", &extruded_dxf_bytes());
    h.run();
    h.get_by_label("Insert").click();
    h.run();
    h.run();

    let e = model::find_by_name(&h.state().world, "Vise jaw").expect("inserted extruded DXF");
    let def = fixture::def_of(&h.state().world, &e).expect("fixture def");
    assert!(matches!(
        def.profile,
        Some(FixtureProfile::Extruded { axis: ExtrudeAxis::Y, depth, .. }) if (depth - 15.0).abs() < 1e-6
    ));
    assert!(step_source::of(&h.state().world, &e).is_none());

    // Spun DXF chuck also inserts from dialog and undoes cleanly.
    h.state_mut().pending.push_back(CommandKind::ProfileFixtureDialog(String::new()));
    h.run();
    if let Some(FixtureLibDialog::Profile(d)) = h.state_mut().fixture_lib.as_mut() {
        d.kind = ProfileKindChoice::Spun;
    }
    h.state_mut().load_profile_dxf_bytes("Chuck 3-jaw", &spun_dxf_bytes());
    h.run();
    h.get_by_label("Insert").click();
    h.run();
    h.run();
    let chuck = model::find_by_name(&h.state().world, "Chuck 3-jaw").expect("inserted spun DXF");
    let cdef = fixture::def_of(&h.state().world, &chuck).expect("chuck def");
    assert!(matches!(cdef.profile, Some(FixtureProfile::Spun { .. })));

    {
        let st = h.state_mut();
        st.history.undo(&st.world);
    }
    assert!(model::find_by_name(&h.state().world, "Chuck 3-jaw").is_none());
}

#[test]
fn refused_profile_or_bad_dxf_is_named_in_dialog() {
    let mut st = State::new();
    st.pending.push_back(CommandKind::ProfileFixtureDialog(String::new()));
    let mut h = dialog(st);
    h.run();

    h.state_mut().load_profile_dxf_bytes("broken", b"not dxf");
    h.run();
    assert!(h.query_by_label_contains("DXF profile not loaded").is_some());

    if let Some(FixtureLibDialog::Profile(d)) = h.state_mut().fixture_lib.as_mut() {
        d.name.clear();
        d.error = None;
    }
    h.run();
    h.get_by_label("Insert").click();
    h.run();
    h.run();
    let Some(FixtureLibDialog::Profile(d)) = &h.state().fixture_lib else {
        panic!("dialog stays open on refusal");
    };
    assert!(d.error.as_deref().is_some_and(|e| e.contains("blank")), "{:?}", d.error);
}

#[test]
fn editing_placed_profile_row_regenerates_mesh_and_exports_dxf() {
    let mut st = State::new();
    let prof = FixtureProfile::extruded_from_dxf(&extruded_dxf_bytes(), ExtrudeAxis::Z, None).expect("dxf");
    st.pending.push_back(CommandKind::InsertProfileFixture {
        setup: None,
        name: "Parallel block".into(),
        profile: prof,
        pos: [0.0; 3],
        yaw_deg: 0.0,
    });
    let mut h = page_harness(st);
    h.run();

    assert!(h.query_by_label("Extruded fixture profile").is_some());
    assert!(h.query_by_label("Apply profile").is_some());

    if let Some(d) = h.state_mut().profile_edits.get_mut("Parallel block")
        && let FixtureProfile::Extruded { depth, .. } = &mut d.profile
    {
        *depth = 35.0;
    }
    h.get_by_label("Apply profile").click();
    h.run();
    h.run();

    let e = model::find_by_name(&h.state().world, "Parallel block").expect("parallel block");
    let facets = model::facets_of(&h.state().world, &e).expect("facets");
    let max_z = facets
        .iter()
        .flat_map(|t| [t.a.z, t.b.z, t.c.z])
        .fold(f64::NEG_INFINITY, f64::max);
    assert!((max_z - 35.0).abs() < 1e-6, "updated Z height expected 35, got {max_z}");

    let out_dir = std::env::temp_dir().join(format!("camcraft_ui_dxf_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&out_dir);
    let out_path = out_dir.join("parallel.dxf");
    h.state_mut().pending.push_back(CommandKind::ExportFixtureDxf {
        name: "Parallel block".into(),
        path: out_path.clone(),
    });
    h.run();
    let dxf = std::fs::read(&out_path).expect("exported dxf written");
    let reparsed = FixtureProfile::extruded_from_dxf(&dxf, ExtrudeAxis::Z, None).expect("reparse dxf");
    assert!(matches!(reparsed, FixtureProfile::Extruded { depth, .. } if (depth - 35.0).abs() < 1e-3));
    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn step_fixture_keeps_raw_step_and_allows_reference_derive_only() {
    let step_bytes = box_step_bytes();
    let step_text = std::str::from_utf8(&step_bytes).expect("utf-8 STEP").to_owned();
    let mut st = State::new();
    let import = ImportModel::from_bytes("chuck.step", &step_bytes, None).expect("import STEP");
    let cmd = ImportFixture::new(&st.world, import, None).expect("step fixture");
    let name = cmd.name().to_owned();
    st.history.execute(&st.world, cmd);
    st.doc_version += 1;
    let orig_facets: Vec<Triangle> = model::find_by_name(&st.world, &name)
        .and_then(|e| model::facets_of(&st.world, &e))
        .expect("orig facets")
        .to_vec();

    let mut h = page_harness(st);
    h.run();

    assert!(h.query_by_label_contains("STEP solid kept as source").is_some());
    assert!(h.query_by_label("Apply profile").is_none(), "STEP fixture must not offer direct overwrite");
    assert!(h.query_by_label("Derive extruded profile").is_some());

    h.get_by_label("Derive extruded profile").click();
    h.run();
    h.run();

    let e = model::find_by_name(&h.state().world, &name).expect("step chuck");
    assert_eq!(step_source::of(&h.state().world, &e).as_deref(), Some(step_text.as_str()));
    let facets_after = model::facets_of(&h.state().world, &e).expect("facets");
    assert_eq!(facets_after.len(), orig_facets.len(), "STEP mesh facets stay intact");
    assert!(fixture::def_of(&h.state().world, &e).and_then(|d| d.profile).is_some());
}
