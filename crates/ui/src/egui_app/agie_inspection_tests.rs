//! A synthetic Agie inspection request through the saved Wire Contour editor.
//! Gate map (M4.4): covers ui::egui_app::op_page, ui::state_per_op, app::operation::agie_inspection.

use super::super::dialog_tests::page;
use crate::state::{CommandKind, State};
use camcraft_app::commands::{DxfOptions, GenerateOperation, ImportDxf};
use camcraft_app::operation::OpParams;
use camcraft_app::{machine, model, project, sketch, stock::StockShape, wire_tools};
use egui::accesskit::Role;
use egui_kittest::{Harness, kittest::Queryable};

const BUTTON: &str = "Export Agie inspection setup";
const DXF: &str = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
  0\nLWPOLYLINE\n8\nnotch\n90\n6\n70\n1\n10\n-7\n20\n-5\n10\n7\n20\n-5\n10\n7\n20\n5\n10\n2\n20\n5\n42\n-1\n10\n-2\n20\n5\n10\n-7\n20\n5\n\
  0\nLINE\n8\nnotch\n10\n7\n20\n-5\n11\n-7\n21\n-5\n\
  0\nCIRCLE\n8\nframe\n10\n0\n20\n0\n40\n16\n0\nCIRCLE\n8\nother\n10\n12\n20\n0\n40\n2\n0\nENDSEC\n0\nEOF\n";

const AGIE_MCH: &str = include_str!("../../../../assets/machines/agiecut-classic-2s.mch");
const WIRE_ISO_MCH: &str = include_str!("../../../app/tests/edm/wire-iso-fixture.mch");

/// A document with `mch` mounted, a 0.25 mm wire, stock and the placed
/// drawing; returns the id of its closed `notch` opening.
fn drawing_state(mch: &str) -> (State, camcraft_app::geometry::sketch_chains::ChainId) {
    let mut st = State::new();
    let machine = machine::parse_mch(mch).expect("machine");
    st.pending.push_back(CommandKind::EditMachineApply {
        path: "wire.mch".into(),
        machine: Box::new(machine),
    });
    st.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box {
            dx: 1.0,
            dy: 1.0,
            dz: 10.0,
        },
        pos: [0.0, 0.0, -10.0],
        yaw_deg: 0.0,
    });
    st.pending.push_back(CommandKind::AddTool {
        name: "Wire".into(),
        tool: wire_tools::wire_tool(0.25),
    });
    st.drain_actions();
    st.history.execute(
        &st.world,
        ImportDxf::from_bytes(
            &st.world,
            None,
            "shape.dxf",
            DXF.as_bytes(),
            &DxfOptions::default(),
        )
        .expect("profile"),
    );
    let id = sketch::find(&st.world, "shape")
        .expect("sketch")
        .1
        .data
        .chains()
        .by_layer("notch")[0]
        .id();
    (st, id)
}

fn inspection_page() -> (State, String) {
    let (mut st, id) = drawing_state(AGIE_MCH);
    st.pending.push_back(CommandKind::CreateAgieInspection {
        name: "Opening".into(),
        tool: "Wire".into(),
        drawing: "shape".into(),
        chain: id,
        start: [0.0, -1.0],
        setup: camcraft_app::setup::active(&st.world),
    });
    st.drain_actions();
    assert!(
        model::find_by_name(&st.world, "Opening").is_some(),
        "{}",
        st.status
    );
    st.sync_ops_ui();
    (st, id.to_string())
}

#[test]
fn unassigned_inspection_opening_survives_undo_redo_and_project_reopen() {
    let (mut st, id) = inspection_page();
    assert!(st.history.undo(&st.world));
    assert!(model::find_by_name(&st.world, "Opening").is_none());
    assert!(st.history.redo(&st.world));
    let doc = serde_json::to_string(&project::collect(&st.world)).unwrap();
    let world = camcraft_app::world::new_world();
    project::open(&world, &doc).unwrap();
    let entity = model::find_by_name(&world, "Opening").unwrap();
    let op = model::operation_of(&world, &entity).unwrap();
    let OpParams::WireContour(w) = op.params else {
        panic!("wire contour")
    };
    assert!(w.condition.is_empty() && !w.capture);
    assert_eq!(
        w.picks.first(camcraft_app::path_picks::PickRole::Start).map(|p| [p.xy.x, p.xy.y]),
        Some([0.0, -1.0])
    );
    let camcraft_app::operation::SrcRef::Selection(picks) = op.src else {
        panic!("selection")
    };
    let camcraft_app::operation::FeatureSel::SketchChain(selected) = picks[0].feature else {
        panic!("one chain")
    };
    assert_eq!(id, selected.to_string());
    assert!(GenerateOperation::new(&world, "Opening").is_err());
}

#[test]
fn saved_selected_opening_exports_zero_steps_through_the_desktop_action() {
    let (st, id) = inspection_page();
    let mut h = page(st);
    h.run();
    h.get_by_label(BUTTON).click();
    h.run_steps(2);
    let st = h.state_mut();
    let Some(CommandKind::ExportAgieInspection(name)) = st.pending.pop_front() else {
        panic!("inspection action");
    };
    assert_eq!(name, "Opening");
    let parent = std::env::temp_dir().join(format!("camcraft-agie-view-{}", std::process::id()));
    std::fs::create_dir(&parent).expect("new directory");
    let dest = parent.join("CHECK");
    st.export_agie_inspection_to(&name, &dest);
    assert!(
        st.status.contains("0 working steps; inspection only"),
        "{}",
        st.status
    );
    assert!(st.status.contains(&id) && st.status.contains("6 nominal spans"));
    let iso = std::fs::read_to_string(dest.join("CHECK.ISO")).expect("ISO");
    assert_eq!(iso.matches(" G02 ").count(), 1);
    let sbl = std::fs::read_to_string(dest.join("CHECK.SBL")).expect("script");
    assert!(sbl.contains("ASDELETE") && !sbl.contains("ASCREATE"));
    assert!(GenerateOperation::new(&st.world, &name).is_err());
    st.export_agie_inspection_to("missing", &parent.join("REFUSED"));
    assert!(st.status.contains("refused"));
    assert!(!parent.join("REFUSED").exists());
    std::fs::remove_dir_all(parent).expect("remove test bundle");
}

#[test]
fn setup_post_inspection_fields_and_submit_fit_the_visible_panel() {
    let (state, _) = inspection_page();
    let mut h = Harness::builder()
        .with_size(egui::vec2(750.0, 830.0))
        .build_ui_state(
            |ui, (st, panel): &mut (State, egui::Rect)| {
                *panel = ui.clip_rect().intersect(ui.available_rect_before_wrap());
                super::super::setup_pages::page(ui, st, "", crate::state::SetupPage::Post);
            },
            (state, egui::Rect::NOTHING),
        );
    h.run();
    h.get_by_label_contains("Agie nominal inspection").click();
    h.run();

    let panel = h.state().1;
    let mut previous_bottom = panel.top();
    for (name, role) in [
        ("Operation name", Role::TextInput),
        ("Wire tool", Role::ComboBox),
        ("DXF opening", Role::ComboBox),
        ("Start X (world mm)", Role::TextInput),
        ("Start Y (world mm)", Role::TextInput),
    ] {
        let label = h.get_by_role_and_label(Role::Label, name).rect();
        let control = h
            .query_all_by_role(role)
            .map(|n| n.rect())
            .find(|r| label.y_range().contains(r.center().y))
            .unwrap_or_else(|| panic!("missing control on the {name} row"));
        assert!(control.left() >= label.right(), "{name} overlaps its label");
        let row = label.union(control);
        assert!(
            panel.contains_rect(row),
            "{name} outside {panel:?}: {row:?}"
        );
        assert!(
            row.top() >= previous_bottom,
            "{name} overlaps the preceding row"
        );
        previous_bottom = row.bottom();
    }
    let submit = h.get_by_role_and_label(Role::Button, "Create inspection opening");
    assert!(panel.contains_rect(submit.rect()) && submit.rect().top() >= previous_bottom);
    submit.click();
    h.run();
    assert!(h.state().0.status.contains("Enter an operation name"));
}

/// An ordinary die cut on one DXF chain, with a machining condition, on a
/// machine that is not the Agie machine. It has the same source shape as an
/// inspection opening (one selected sketch chain, die side).
fn plain_dxf_wire_page() -> State {
    use camcraft_app::operation::{FeatureSel, GeoSel, Height, HeightReference, SrcRef, WireContourOp, WireSide};
    let (mut st, id) = drawing_state(WIRE_ISO_MCH);
    let params = OpParams::WireContour(WireContourOp {
        condition: "C110".into(),
        top_z: Height::of(HeightReference::TopOfStock, 0.0),
        bottom_z: Height::of(HeightReference::BottomOfStock, 0.0),
        side: WireSide::Die,
        picks: camcraft_app::path_picks::PathPicks {
            start: vec![camcraft_app::path_picks::PathPick::xy(camcraft_app::geo2d::Point2::new(0.0, -1.0))],
            exit: Vec::new(),
        },
        tabs: None,
        capture: true,
        both_pieces_restrained: false,
        stock_centered_in_xy_travel: false,
        taper_deg: 0.0,
        burnout: None,
        clearance: camcraft_app::operation::DEFAULT_CLEARANCE,
        skims: Vec::new(),
        four_axis: None,
        sharp_corners: false,
        lead_arc_radius: None,
    });
    let src = SrcRef::Selection(vec![GeoSel { entity: "shape".into(), feature: FeatureSel::SketchChain(id) }]);
    let cmd = camcraft_app::commands::CreateOperation::new(&st.world, "Die", "Wire", src, params).expect("die cut");
    st.history.execute(&st.world, cmd);
    st.pending.push_back(CommandKind::Select(Some("Die".into())));
    st.ui_target = crate::state::UiTarget::Operation("Die".into());
    st.drain_actions();
    st.sync_ops_ui();
    st
}

/// The maintainer saw the Agie export on every wire job: a plain DXF die
/// cut on another wire machine shows no Agie action.
#[test]
fn plain_dxf_wire_cut_on_another_machine_shows_no_agie_export() {
    let st = plain_dxf_wire_page();
    let op = model::find_by_name(&st.world, "Die")
        .and_then(|e| model::operation_of(&st.world, &e))
        .expect("die cut");
    assert!(
        camcraft_app::operation::has_agie_inspection_source(&op),
        "the source shape alone qualifies; the machine must decide"
    );
    let mut h = page(st);
    h.run();
    assert!(h.query_by_label("Delete operation").is_some(), "the operation page shows");
    assert!(h.query_by_label(BUTTON).is_none(), "no Agie export on a non-Agie job");
}

#[test]
fn agie_inspection_opening_still_shows_its_export() {
    let (st, _) = inspection_page();
    let mut h = page(st);
    h.run();
    assert!(h.query_by_label(BUTTON).is_some());
}

fn post_page(state: State, ctx: String) -> Harness<'static, State> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(750.0, 830.0))
        .build_ui_state(
            move |ui, st: &mut State| {
                super::super::setup_pages::page(ui, st, &ctx, crate::state::SetupPage::Post);
            },
            state,
        );
    h.run();
    h
}

fn with_setup(mut st: State) -> (State, String) {
    st.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    st.drain_actions();
    let name = camcraft_app::setup::active(&st.world).expect("a setup");
    (st, name)
}

/// The Post page of a setup on another machine draws no Agie section.
#[test]
fn non_agie_setup_post_page_has_no_agie_inspection_section() {
    let (st, _) = drawing_state(WIRE_ISO_MCH);
    let (st, name) = with_setup(st);
    let h = post_page(st, name);
    assert!(h.query_by_label_contains("Agie nominal inspection").is_none());
    assert!(h.query_by_label_contains("AGIECUT").is_none());
    let (st, _) = drawing_state(WIRE_ISO_MCH);
    let h = post_page(st, String::new());
    assert!(h.query_by_label_contains("Agie nominal inspection").is_none());
}

/// A setup that mounts the Agie machine keeps the section.
#[test]
fn agie_setup_post_page_keeps_the_agie_inspection_section() {
    let (st, _) = drawing_state(AGIE_MCH);
    let (st, name) = with_setup(st);
    let h = post_page(st, name);
    assert!(h.query_by_label_contains("Agie nominal inspection").is_some());
}
