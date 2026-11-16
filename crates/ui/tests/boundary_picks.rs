//! The Boundary group's pick UX (M11.5, the user's hand-test: open
//! boundary selection unclear, an edge pick closed the loop, no way to
//! break a closed selection, vague visualization): an open chain picked
//! into an Include is caught at the pick (and refused at Apply) and one
//! role change routes it to the open boundary; a plain edge click takes
//! one edge, Shift closes the loop, Ctrl takes an edge back out (opening
//! a picked loop or face); the one list greys roles with the reason; the
//! viewport marks carry role-coloured edges, chain ends and arrows into
//! the machined side; the commit is one undoable edit.
//! Provenance: our own tests, specced from the page's contract.
//!
//! Gate map (M11.5): covers ui::boundary_draft, ui::boundary_viz,
//! ui::interact (queue_boundary_pick), ui::state (BoundaryPick),
//! ui::egui_app::colorvision (bound_* roles), app::operation
//! (boundary_picks), app::commands (EditOperationParams::with_area).

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::model;
use camcraft_app::operation::{
    self, AreaControls, BoundaryMode, BoundaryShape, ChainSide, ExtraBoundary, FeatureSel, GeoSel, Height,
    OpParams, PickShape, SrcRef,
};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_ui::boundary_draft::{self, EntryRef, Role};
use camcraft_ui::boundary_viz::{self, MarkKind};
use camcraft_ui::egui_app::colorvision::palette_of;
use camcraft_ui::interact::{Mods, queue_boundary_pick};
use camcraft_ui::state::{BoundaryPick, CommandKind, OpUi, State};

fn face_params() -> OpParams {
    OpParams::Face { holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(10.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// A 40 x 40 x 10 block ("part") centred in a 60 x 60 x 20 stock, a Face
/// op on the stock open on the page.
fn block_state() -> State {
    let mut state = State::new();
    state.pending.push_back(CommandKind::SetStock {
        shape: StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 },
        pos: [0.0, 0.0, 0.0],
        yaw_deg: 0.0,
    });
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("tool");
    state.pending.push_back(CommandKind::AddTool { name: "EM-0400-2FL".into(), tool });
    state.pending.push_back(CommandKind::CreateOperation {
        name: "Face1".into(),
        tool: "EM-0400-2FL".into(),
        src: SrcRef::Stock,
        params: face_params(),
    });
    state.drain_actions();
    let part = ImportModel::from_mesh("part", cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 10.0)));
    state.history.execute(&state.world, part);
    state.doc_version += 1;
    let e = model::find_by_name(&state.world, "Face1").expect("op");
    let op = model::operation_of(&state.world, &e).expect("op");
    state.op_ui = OpUi::from_op("Face1", &op);
    state
}

/// The run a plain viewport click on the block's top rim side takes:
/// `side` picks the side by its constant coordinate.
fn rim(state: &State, side: impl Fn([f64; 3]) -> bool) -> GeoSel {
    let e = model::find_by_name(&state.world, "part").expect("part");
    let f = model::features_of(&state.world, &e).expect("features");
    let i = f
        .edges
        .iter()
        .position(|t| {
            let (a, b) = (f.verts[t.a as usize], f.verts[t.b as usize]);
            a[2] == 10.0 && b[2] == 10.0 && side(a) && side(b)
        })
        .expect("a rim edge");
    operation::run_pick(&state.world, "part", i as u32).expect("its run")
}

fn north(state: &State) -> GeoSel {
    rim(state, |p| p[1] == 20.0)
}

fn east(state: &State) -> GeoSel {
    rim(state, |p| p[0] == 20.0)
}

fn top_face(state: &State) -> GeoSel {
    let e = model::find_by_name(&state.world, "part").expect("part");
    let f = model::features_of(&state.world, &e).expect("features");
    // Normals are winding-free: the top is the horizontal face at Z 10.
    let face = f
        .faces
        .iter()
        .find(|x| x.normal[2].abs() > 0.99 && x.loops.iter().flat_map(|l| &l.verts).all(|&v| f.verts[v as usize][2] == 10.0))
        .expect("the top face");
    GeoSel { entity: "part".into(), feature: FeatureSel::Face(face.id) }
}

const PLAIN: Mods = Mods { command: false, shift: false, alt: false };
const CMD: Mods = Mods { command: true, shift: false, alt: false };
const SHIFT: Mods = Mods { command: false, shift: true, alt: false };

fn live_area(state: &State) -> AreaControls {
    let e = model::find_by_name(&state.world, "Face1").expect("op");
    model::operation_of(&state.world, &e).expect("op").area
}

#[test]
fn an_open_chain_picked_into_an_include_is_caught_at_the_pick_and_routed_to_open() {
    let mut state = block_state();
    state.boundary_pick = Some(BoundaryPick::Include);
    let edge = north(&state);
    queue_boundary_pick(&mut state, edge.clone(), PLAIN);
    assert!(state.pending.is_empty(), "a draft write, nothing queued");
    assert_eq!(state.boundary_pick, Some(BoundaryPick::Extra(0)), "the new entry is the target now");
    let es = boundary_draft::entries(&state);
    assert_eq!(es.len(), 1);
    assert_eq!(es[0].summary, "1 edge, open chain");
    let issue = es[0].issue.clone().expect("caught at the pick");
    assert!(issue.contains("open chain") && issue.contains("Use as open boundary"), "{issue}");
    assert!(state.status.contains("open chain"), "the status says it too: {}", state.status);
    // Apply refuses it by name - never first at Generate.
    state.apply_operation_params("Face1");
    state.drain_actions();
    assert!(state.status.contains("is an open chain"), "{}", state.status);
    assert_eq!(live_area(&state), AreaControls::default());
    // Generate stops at the same door, before anything is queued.
    state.generate_operation("Face1");
    assert!(state.pending.is_empty(), "nothing generates from the committed boundary behind the user's back");
    assert!(state.status.starts_with("not generated:"), "{}", state.status);
    // One role change routes it to the open boundary, then Apply commits.
    boundary_draft::set_role(&mut state, EntryRef::Extra(0), Role::OpenA).expect("open takes an open chain");
    assert!(state.op_ui.area.extra.is_empty());
    assert_eq!(state.op_ui.area.open.as_ref().map(|o| (o.picks.clone(), o.side)), Some((vec![edge], ChainSide::Inside)));
    assert_eq!(state.boundary_pick, Some(BoundaryPick::Open), "the target followed the entry");
    state.apply_operation_params("Face1");
    state.drain_actions();
    assert_eq!(live_area(&state), state.op_ui.area);
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert_eq!(live_area(&state), AreaControls::default(), "one undo");
}

#[test]
fn a_plain_click_takes_one_edge_shift_closes_the_loop_ctrl_opens_it() {
    let mut state = block_state();
    state.boundary_pick = Some(BoundaryPick::Open);
    let n = north(&state);
    queue_boundary_pick(&mut state, n.clone(), PLAIN);
    let picks = |s: &State| s.op_ui.area.open.as_ref().map(|o| o.picks.clone()).unwrap_or_default();
    assert_eq!(picks(&state), vec![n.clone()], "one edge, no loop");
    assert!(boundary_draft::shape_of(&state, &picks(&state)).is_open());
    // A second plain click replaces it; Ctrl adds.
    let e = east(&state);
    queue_boundary_pick(&mut state, e.clone(), PLAIN);
    assert_eq!(picks(&state), vec![e.clone()]);
    queue_boundary_pick(&mut state, n.clone(), CMD);
    assert_eq!(picks(&state), vec![e.clone(), n.clone()]);
    assert!(matches!(boundary_draft::shape_of(&state, &picks(&state)), PickShape::Open { picks: 2, .. }));
    // Shift closes it around the top face's loop.
    queue_boundary_pick(&mut state, n.clone(), SHIFT);
    assert_eq!(picks(&state).len(), 4);
    assert!(boundary_draft::shape_of(&state, &picks(&state)).is_closed());
    // Ctrl on a picked edge takes it out: the loop is open again.
    queue_boundary_pick(&mut state, e.clone(), CMD);
    assert_eq!(picks(&state).len(), 3);
    assert!(!picks(&state).contains(&e));
    assert!(matches!(boundary_draft::shape_of(&state, &picks(&state)), PickShape::Open { picks: 3, .. }));
    assert!(state.status.contains("removed"), "{}", state.status);
}

#[test]
fn ctrl_on_an_edge_of_a_picked_face_breaks_its_loop_open() {
    let mut state = block_state();
    state.boundary_pick = Some(BoundaryPick::Exclude);
    let g = top_face(&state);
    queue_boundary_pick(&mut state, g, PLAIN);
    let es = boundary_draft::entries(&state);
    assert_eq!((es[0].role, es[0].summary.as_str()), (Role::Exclude, "1 face loop, closed"));
    assert!(es[0].issue.is_none());
    let g = north(&state);
    queue_boundary_pick(&mut state, g, CMD);
    let es = boundary_draft::entries(&state);
    assert_eq!(es[0].summary, "3 edges, open chain", "the face became its three other sides");
    assert!(es[0].issue.is_some());
    // Extend to loop closes it again.
    boundary_draft::extend(&mut state, EntryRef::Extra(0)).expect("closes");
    assert_eq!(boundary_draft::entries(&state)[0].summary, "4 edges, closed");
}

#[test]
fn the_one_list_greys_the_roles_a_shape_cannot_take_and_says_why() {
    let mut state = block_state();
    state.op_ui.area.extra.push(ExtraBoundary { mode: BoundaryMode::Include, shape: BoundaryShape::Stock });
    state.boundary_pick = Some(BoundaryPick::Include);
    let g = north(&state);
    queue_boundary_pick(&mut state, g, PLAIN);
    state.boundary_pick = Some(BoundaryPick::Open);
    let g = east(&state);
    queue_boundary_pick(&mut state, g, PLAIN);
    let es = boundary_draft::entries(&state);
    assert_eq!(es.iter().map(|e| e.at).collect::<Vec<_>>(), vec![EntryRef::Extra(0), EntryRef::Extra(1), EntryRef::Open]);
    assert_eq!(es.iter().map(|e| e.role).collect::<Vec<_>>(), vec![Role::Include, Role::Include, Role::OpenA]);
    assert_eq!(es[0].summary, "stock outline, closed");
    let why = |e, r| boundary_draft::role_block(&state, e, r);
    // The stock outline is no pick: never open.
    assert!(why(&es[0], Role::OpenA).is_some_and(|w| w.contains("picked edges")));
    assert!(why(&es[0], Role::Exclude).is_none());
    // An open chain cannot be an exclude, and the one open slot is taken.
    assert!(why(&es[1], Role::Exclude).is_some_and(|w| w.contains("open chain")));
    assert!(why(&es[1], Role::OpenB).is_some_and(|w| w.contains("one open boundary")));
    // The open entry flips sides, and cannot become an include.
    assert!(why(&es[2], Role::OpenB).is_none());
    assert!(why(&es[2], Role::Include).is_some_and(|w| w.contains("open chain")));
    assert!(boundary_draft::set_role(&mut state, EntryRef::Extra(1), Role::Exclude).is_err(), "refused like the dropdown");
    boundary_draft::set_role(&mut state, EntryRef::Open, Role::OpenB).expect("side B");
    assert_eq!(state.op_ui.area.open.as_ref().map(|o| o.side), Some(ChainSide::Outside));
    boundary_draft::remove(&mut state, EntryRef::Extra(0));
    assert_eq!(boundary_draft::entries(&state).len(), 2);
}

#[test]
fn the_viewport_marks_role_coloured_edges_chain_ends_and_the_machined_side() {
    let mut state = block_state();
    assert!(boundary_viz::boundary_marks(&state, 1.0).is_empty(), "nothing while the group is shut");
    state.boundary_pick = Some(BoundaryPick::Open);
    let n = north(&state);
    queue_boundary_pick(&mut state, n, PLAIN);
    state.boundary_view.shown = true;
    boundary_draft::refresh_preview(&mut state);
    let pal = *palette_of(&state.world);
    let marks = boundary_viz::boundary_marks(&state, 1.0);
    let edges: Vec<_> = marks.iter().filter(|m| m.kind == MarkKind::Edge(Role::OpenA)).collect();
    assert!(!edges.is_empty(), "the picked edge draws");
    assert!(edges.iter().all(|m| m.rgb == pal.bound_open && m.width >= 3.0), "thick, in the open role's colour");
    assert_eq!(marks.iter().filter(|m| m.kind == MarkKind::Endpoint).count(), 8, "a square at each end");
    assert!(marks.iter().any(|m| m.kind == MarkKind::Side), "arrows on the machined side");
    // The arrows point into the side the preview machines: side A is the
    // bulk of the stock, south of the chain at y = 20.
    let p = state.boundary_view.preview.clone().expect("a preview");
    assert!(p.refusal.is_none(), "{:?}", p.refusal);
    let region = p.region.expect("the machined area");
    let Some(PickShape::Open { chain, .. }) = boundary_draft::entries(&state)[0].shape.clone() else {
        panic!("an open chain");
    };
    let arrows = boundary_viz::side_arrows(&chain, &region);
    assert!(!arrows.is_empty() && arrows.iter().all(|(_, d)| d.y < -0.99), "{arrows:?}");
    assert!(boundary_viz::preview_fill(&state).is_some(), "the machined area is filled");
    // Side B flips them.
    boundary_draft::set_role(&mut state, EntryRef::Open, Role::OpenB).expect("side B");
    boundary_draft::refresh_preview(&mut state);
    let region = state.boundary_view.preview.clone().and_then(|p| p.region).expect("side B area");
    assert!(boundary_viz::side_arrows(&chain, &region).iter().all(|(_, d)| d.y > 0.99));
    // An include draws in its own colour.
    state.boundary_pick = Some(BoundaryPick::Include);
    let g = top_face(&state);
    queue_boundary_pick(&mut state, g, PLAIN);
    let marks = boundary_viz::boundary_marks(&state, 1.0);
    assert!(marks.iter().any(|m| m.kind == MarkKind::Edge(Role::Include) && m.rgb == pal.bound_include));
    // Collecting with a hover: the edge a click takes lights up.
    state.hover = Some(camcraft_ui::state::HoverHit { entity: "part".into(), facet: usize::MAX, feature: Some(east(&state).feature) });
    assert!(boundary_viz::boundary_marks(&state, 1.0).iter().any(|m| m.kind == MarkKind::Hover));
    // Shut the group and stop collecting: nothing draws.
    state.boundary_view.shown = false;
    state.boundary_pick = None;
    assert!(boundary_viz::boundary_marks(&state, 1.0).is_empty());
    assert!(boundary_viz::preview_fill(&state).is_none());
}

#[test]
fn a_plain_selection_pick_is_untouched_when_the_group_is_not_collecting() {
    let mut state = block_state();
    let n = north(&state);
    queue_boundary_pick(&mut state, n.clone(), PLAIN);
    assert!(matches!(state.pending.back(), Some(CommandKind::SetOperationGeometry { items, .. }) if *items == vec![n]));
    assert!(state.op_ui.area.is_default());
}
