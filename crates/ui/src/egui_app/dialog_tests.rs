//! The reorganised operation dialog (user review 2026-11-06), driven
//! through the real page code: the bottom action bar by label, the
//! Heights groups still editing through Apply's command, Lead in / Lead
//! out's progressive disclosure and the Auto toggle writing the model's
//! 0, and the Boundary summary / no-effect notices.
//! Provenance: our own page law; no vendor material consulted.

use egui::Vec2;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use camcraft_app::commands::{AddTool, CreateOperation, ImportModel};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{
    BoundaryMode, BoundaryShape, Containment, CornerStyle, ExtraBoundary, Height, OpParams, OpType, SrcRef,
};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;

use crate::boundary_summary;
use crate::state::{CommandKind, State, UiTarget};

pub(super) fn pocket(entry: Entry3D) -> OpParams {
    OpParams::Pocket { pass_start: Default::default(), rest: None,
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        tilt: None,
        stepover: 1.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(12.0),
        bottom_z: Height::fixed(9.0),
        stepdown: 1.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        corner_relief: Default::default(),
        picks: Default::default(),
    }
}

pub(super) fn face() -> OpParams {
    OpParams::Face { holes: Default::default(),
        blend_turns: false, blend_radius: None,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(10.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

/// Stock 60x60x20, a 40x40x10 part, a 4 mm end mill and one operation
/// named "Op" on the stock, opened on the page.
pub(super) fn opened(params: OpParams) -> State {
    let mut state = State::new();
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    state.drain_actions();
    let em = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog entry");
    state.history.execute(&state.world, AddTool::new(&state.world, "EM-0400-2FL", em).expect("tool"));
    state.history.execute(
        &state.world,
        ImportModel::from_mesh("part", cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 10.0))),
    );
    state.history.execute(
        &state.world,
        CreateOperation::new(&state.world, "Op", "EM-0400-2FL", SrcRef::Stock, params).expect("operation"),
    );
    state.pending.push_back(CommandKind::Select(Some("Op".into())));
    state.drain_actions();
    state.ui_target = UiTarget::Operation("Op".into());
    state.sync_ops_ui();
    state
}

pub(super) fn page(state: State) -> Harness<'static, State> {
    Harness::builder()
        .with_size(Vec2::new(620.0, 1400.0))
        .build_ui_state(|ui, st: &mut State| super::pages::properties(ui, st, &super::theme::current()), state)
}

fn live(state: &State) -> OpParams {
    model::find_by_name(&state.world, "Op")
        .and_then(|e| model::operation_of(&state.world, &e))
        .map(|o| o.params)
        .expect("the op")
}

/// The control on the same row as `label`: the node of `role` whose
/// vertical centre falls inside the label's rect.
fn on_row<'a>(h: &'a Harness<'static, State>, label: &str, role: Role) -> egui_kittest::Node<'a> {
    let lab = h.get_by_label(label).rect();
    h.query_all_by(move |n| n.role() == role)
        .find(|n| {
            let c = n.rect().center().y;
            c >= lab.top() - 2.0 && c <= lab.bottom() + 2.0
        })
        .expect("a control on the label's row")
}

#[test]
fn the_action_bar_carries_the_four_verbs() {
    let mut harness = page(opened(pocket(Entry3D::Plunge)));
    harness.run();
    for verb in ["Generate", "Apply", "Clear toolpath", "Delete operation"] {
        assert!(harness.query_by_label(verb).is_some(), "the bar shows {verb}");
    }
    assert!(harness.query_by_label("Apply Parameters").is_none(), "the old footer is gone");
    // Nothing generated and nothing edited: the reasons are hovers, not lines.
    assert!(harness.query_by_label_contains("no parameter differs").is_none(), "no extra sentence");
    harness.get_by_label("Delete operation").click();
    harness.run();
    assert!(
        matches!(harness.state().pending.back(), Some(CommandKind::DeleteOperation(n)) if n == "Op"),
        "Delete rides its command"
    );
}

#[test]
fn a_heights_offset_edits_the_draft_and_apply_commits_it() {
    let mut state = opened(pocket(Entry3D::Plunge));
    state.op_tab = super::op_tabs::SEC_HEIGHTS;
    let mut harness = page(state);
    harness.run();
    assert!(harness.query_by_label("Clearance Z").is_some(), "the height is a titled group");
    // The first Offset row is the clearance's.
    let first = harness
        .query_all_by(|n| n.role() == Role::Label && n.value().as_deref() == Some("Offset"))
        .next()
        .map(|n| n.rect())
        .or_else(|| {
            harness
                .query_all_by(|n| n.role() == Role::Label && n.value().as_deref() == Some("Value"))
                .next()
                .map(|n| n.rect())
        })
        .expect("an offset row");
    let spin = harness
        .query_all_by(|n| n.role() == Role::SpinButton)
        .find(|n| (n.rect().center().y - first.center().y).abs() < 12.0)
        .map(|n| n.rect())
        .expect("its number on the same row");
    harness.query_all_by(|n| n.role() == Role::SpinButton).find(|n| n.rect() == spin).expect("spin").click();
    harness.run();
    harness
        .query_all_by(|n| n.role() == Role::SpinButton)
        .find(|n| (n.rect().top() - spin.top()).abs() < 1.0)
        .expect("editing")
        .type_text("30");
    harness.run();
    let draft = match &harness.state().op_ui.params {
        OpParams::Pocket { clearance_z, .. } => clearance_z.offset,
        _ => f64::NAN,
    };
    assert!((draft - 30.0).abs() < 1e-9, "the draft took the typed value ({draft})");
    assert!(matches!(live(harness.state()), OpParams::Pocket { clearance_z, .. } if clearance_z.offset == 25.0), "the world waits for Apply");
    harness.get_by_label("Apply").click();
    harness.run();
    harness.state_mut().drain_actions();
    assert!(matches!(live(harness.state()), OpParams::Pocket { clearance_z, .. } if clearance_z.offset == 30.0), "Apply committed it");
    let st = harness.state_mut();
    st.history.undo(&st.world);
    assert!(matches!(live(harness.state()), OpParams::Pocket { clearance_z, .. } if clearance_z.offset == 25.0), "and Ctrl+Z puts it back");
}

#[test]
fn lead_in_shows_only_the_chosen_entry_types_fields() {
    let mut state = opened(pocket(Entry3D::Ramp));
    state.op_tab = super::op_tabs::SEC_LEADS;
    let mut harness = page(state);
    harness.run();
    assert!(harness.query_by_label("Ramp angle").is_some(), "Ramp shows its own fields");
    for helix in ["Helix pitch", "Helix angle", "Minimum radius", "Clockwise (G2)", "Helix radius"] {
        assert!(harness.query_by_label(helix).is_none(), "{helix} hides under Ramp");
    }
    assert!(harness.query_by_label_contains("0 =").is_none(), "no sentinel magic in the labels");
    if let OpParams::Pocket { entry, .. } = &mut harness.state_mut().op_ui.params {
        *entry = Entry3D::Helix;
    }
    harness.run();
    assert!(harness.query_by_label("Helix pitch").is_some(), "Helix shows its fields");
    assert!(harness.query_by_label("Ramp angle").is_none(), "and hides Ramp's");
    for g in ["Entry", "Linking", "Plunge", "Entry feeds", "Engagement"] {
        assert!(harness.query_by_label(g).is_some(), "the {g} group");
    }
}

#[test]
fn the_auto_toggle_writes_the_models_zero_and_back() {
    let mut params = pocket(Entry3D::Helix);
    if let OpParams::Pocket { links, .. } = &mut params {
        links.approach.helix_pitch = 1.5;
    }
    let mut state = opened(params);
    state.op_tab = super::op_tabs::SEC_LEADS;
    let mut harness = page(state);
    harness.run();
    let pitch = |h: &Harness<'static, State>| match &h.state().op_ui.params {
        OpParams::Pocket { links, .. } => links.approach.helix_pitch,
        _ => f64::NAN,
    };
    on_row(&harness, "Helix pitch", Role::CheckBox).click();
    harness.run();
    assert_eq!(pitch(&harness), 0.0, "Auto on writes 0");
    assert!(harness.query_by_label("1 turn / helix Ø").is_some(), "and shows what it derives");
    on_row(&harness, "Helix pitch", Role::CheckBox).click();
    harness.run();
    assert_eq!(pitch(&harness), 1.0, "Auto off seeds a value to edit");
}

#[test]
fn the_summary_says_what_an_open_face_boundary_machines() {
    let mut state = opened(face());
    let f = model::find_by_name(&state.world, "part").and_then(|e| model::features_of(&state.world, &e)).expect("features");
    let i = f
        .edges
        .iter()
        .position(|t| {
            let (a, b) = (f.verts[t.a as usize], f.verts[t.b as usize]);
            a[2] == 10.0 && b[2] == 10.0 && a[1] == 20.0 && b[1] == 20.0
        })
        .expect("the north rim edge");
    let edge = camcraft_app::operation::run_pick(&state.world, "part", i as u32).expect("its run");
    state.op_ui.area.open =
        Some(camcraft_app::operation::OpenBoundary { picks: vec![edge], side: camcraft_app::operation::ChainSide::Inside });
    crate::boundary_draft::warm(&mut state);
    crate::boundary_draft::refresh_preview(&mut state);
    let s = boundary_summary::summarize(&state);
    assert_eq!(
        s.lines[0],
        "Faces the stock top on the −Y side (side A) of the picked open edge, up to that edge. Rows run parallel to the edge, starting from the far side and working in toward it."
    );
    assert!(s.notices.is_empty(), "{:?}", s.notices);
    // The viewport fill is Face's swept band, not the bare region.
    let p = state.boundary_view.preview.clone().expect("preview");
    let (band, region) = (p.footprint.expect("footprint"), p.region.expect("region"));
    assert!(band.area().abs() > region.area().abs(), "the band overruns the region by the tool's reach");
}

#[test]
fn the_summary_says_a_contour_rides_its_open_boundary() {
    let contour = crate::operations::retype(&face(), camcraft_app::operation::OpType::Contour as usize);
    let mut state = opened(contour);
    let f = model::find_by_name(&state.world, "part").and_then(|e| model::features_of(&state.world, &e)).expect("features");
    let i = f
        .edges
        .iter()
        .position(|t| {
            let (a, b) = (f.verts[t.a as usize], f.verts[t.b as usize]);
            a[2] == 10.0 && b[2] == 10.0 && a[1] == 20.0 && b[1] == 20.0
        })
        .expect("the north rim edge");
    let edge = camcraft_app::operation::run_pick(&state.world, "part", i as u32).expect("its run");
    state.op_ui.area.open =
        Some(camcraft_app::operation::OpenBoundary { picks: vec![edge], side: camcraft_app::operation::ChainSide::Outside });
    state.op_ui.area.extension = 2.0;
    let s = boundary_summary::summarize(&state);
    assert!(
        s.lines[0].starts_with("Contours along the picked open edge as one open pass on side B, the side holding less of"),
        "{:?}",
        s.lines
    );
    assert!(s.lines[1].starts_with("Tool edge on the picked edge: its centre runs one tool radius"), "{:?}", s.lines);
    // The extension runs the pass past the chain's ends - it reaches
    // past no "exterior edge", and crosses no boundary.
    assert_eq!(
        s.lines.get(2).map(String::as_str),
        Some("The pass runs 2.000 mm past each end of the picked edge's chain, straight on along it."),
        "{:?}",
        s.lines
    );
    assert!(!s.lines.iter().any(|l| l.contains("exterior edges")), "{:?}", s.lines);
    assert!(s.notices.is_empty(), "no no-effect notice: {:?}", s.notices);
}

#[test]
fn the_summary_says_a_closed_loop_inside_with_an_offset() {
    let mut state = opened(pocket(Entry3D::Plunge));
    state.op_ui.area.extra.push(ExtraBoundary { mode: BoundaryMode::Include, shape: BoundaryShape::Picks(Vec::new()) });
    state.op_ui.area.containment = Some(Containment::Inside);
    state.op_ui.area.offset = 2.0;
    let s = boundary_summary::summarize(&state);
    assert_eq!(s.lines[0], "Clears the stock plus 1 included boundary.");
    assert_eq!(
        s.lines[1],
        "Tool stays inside the picked loop, offset +2.000 mm: its edge touches it, its centre runs one tool radius, 2.000 mm in."
    );
    assert_eq!(
        s.notices,
        vec![format!(
            "{} already keeps the whole tool inside its area, so Inside changes nothing here - pick On or Outside to reach further.",
            OpType::Pocket
        )]
    );
}

#[test]
fn a_setting_the_kind_ignores_is_said_not_swallowed() {
    let mut state = opened(face());
    state.op_ui.area.corner = CornerStyle::Sharp;
    state.op_ui.area.extra.push(ExtraBoundary { mode: BoundaryMode::Exclude, shape: BoundaryShape::Stock });
    let s = boundary_summary::summarize(&state);
    assert!(s.notices.iter().any(|n| n == "The corner style has no effect while the offset is 0."), "{:?}", s.notices);
    assert!(s.notices.iter().any(|n| n.starts_with("Face clears the whole bounding band")), "{:?}", s.notices);
    // Drawn on the page, in the Boundary group.
    let mut harness = page(state);
    harness.run();
    assert!(harness.query_by_label("The corner style has no effect while the offset is 0.").is_some());
    // A kind with no machining area says so instead of drawing controls.
    let cur = harness.state().op_ui.params.clone();
    let drill = camcraft_app::operation::KindInfo::ALL.iter().position(|k| k.label == "Drill").expect("drill kind");
    harness.state_mut().op_ui.params = crate::operations::retype(&cur, drill);
    harness.run();
    assert!(harness.query_by_label_contains("Boundary settings do not apply to it: Drill rides its hole list").is_some());
}

#[test]
fn the_status_banner_is_one_line() {
    let state = opened(pocket(Entry3D::Plunge));
    let (line, _, details) = super::op_page::status_text(&state, "Op");
    assert_eq!(line, "Status: never generated");
    assert!(details.is_empty());
}

/// Retype the open draft into the kind labelled `label` (the retype
/// menu's own law), on tab `tab`.
fn retyped(label: &str, tab: usize) -> Harness<'static, State> {
    let mut state = opened(pocket(Entry3D::Plunge));
    let k = camcraft_app::operation::KindInfo::ALL.iter().position(|k| k.label == label).expect("the kind");
    state.op_ui.params = crate::operations::retype(&state.op_ui.params, k);
    state.op_tab = tab;
    let mut harness = page(state);
    harness.run();
    harness
}

#[test]
fn contour_side_is_a_same_row_field() {
    let harness = retyped("2D Contour", super::op_tabs::SEC_GEO);
    let lab = harness.get_by_label("Side").rect();
    let combo = on_row(&harness, "Side", Role::ComboBox).rect();
    assert!(combo.left() > lab.right(), "the Side combo sits right of its label, not under it");
}

#[test]
fn heights_loose_rows_live_in_the_levels_group() {
    let mut state = opened(pocket(Entry3D::Plunge));
    state.op_tab = super::op_tabs::SEC_HEIGHTS;
    let mut harness = page(state);
    harness.run();
    let levels = harness.get_by_label("Levels").rect();
    for row in ["Hop below clearance", "Stepdown"] {
        let r = harness.get_by_label(row).rect();
        assert!(r.top() > levels.bottom(), "{row} draws inside Levels, under its header");
    }
    assert!(harness.query_by_label("Retract plane").is_none(), "no caption over the hop row");
}

#[test]
fn drill_peck_auto_writes_zero_and_off_seeds_a_bite() {
    let mut harness = retyped("Drill", super::op_tabs::SEC_PASSES);
    if let OpParams::Drill { peck, .. } = &mut harness.state_mut().op_ui.params {
        *peck = 1.5;
    }
    harness.run();
    let peck = |h: &Harness<'static, State>| match &h.state().op_ui.params {
        OpParams::Drill { peck, .. } => *peck,
        _ => f64::NAN,
    };
    assert!(harness.query_by_label_contains("0 lets").is_none(), "no sentinel wording");
    harness.get_by_label("Auto").click();
    harness.run();
    assert_eq!(peck(&harness), 0.0, "Auto writes the model's 0");
    assert!(harness.query_by_label("by the cycle").is_some(), "the derived figure shows in place of the number");
    harness.get_by_label("Auto").click();
    harness.run();
    assert_eq!(peck(&harness), 2.0, "Auto off seeds the starting bite");
}
