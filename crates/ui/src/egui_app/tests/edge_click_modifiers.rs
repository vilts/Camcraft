//! #1505's frame test: edge picking by real viewport clicks, the same law
//! in every edge-picking operation. A prism whose top rim has two edges
//! bending 20 degrees (one tangent chain, two edges) beside the seed's
//! block: a plain click takes exactly the edge under the cursor (never the
//! tangent chain it lies on), Ctrl+click adds one more or takes one back
//! out, Shift+click takes the whole loop, and a plain click afterwards
//! replaces it all. Run on Contour1 and on a 2D Pocket.
//! Provenance: our own pick law test over our own shell; no vendor
//! material consulted.

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::operation::{self, GeoSel, OpType};

use super::*;

const X0: f64 = 200.0;
const TOP: f64 = 10.0;

/// Counter-clockwise outline: A B (flat, 40 long), B C (10 long, bends 20 degrees; the lengths differ by more than a run's ratio, so the two are two runs), then up
/// the far side and back along the near one. A-B and B-C share one tangent
/// chain; every other corner is square.
fn outline() -> [[f64; 2]; 5] {
    let rise = 10.0 * 20.0_f64.to_radians().tan();
    [[X0, 0.0], [X0 + 40.0, 0.0], [X0 + 50.0, rise], [X0 + 50.0, 40.0], [X0, 40.0]]
}

fn prism() -> Mesh {
    let o = outline();
    let p = |q: [f64; 2], z: f64| Point3::new(q[0], q[1], z);
    let mut t = Vec::new();
    for i in 1..o.len() - 1 {
        t.push(Triangle::new(p(o[0], TOP), p(o[i], TOP), p(o[i + 1], TOP)));
        t.push(Triangle::new(p(o[0], 0.0), p(o[i + 1], 0.0), p(o[i], 0.0)));
    }
    for i in 0..o.len() {
        let (a, b) = (o[i], o[(i + 1) % o.len()]);
        t.push(Triangle::new(p(a, 0.0), p(b, 0.0), p(b, TOP)));
        t.push(Triangle::new(p(a, 0.0), p(b, TOP), p(a, TOP)));
    }
    Mesh::from_facets(t)
}

fn px_of(sh: &ShellUi, q: [f64; 3]) -> egui::Pos2 {
    let ([x0, y0], [x1, y1]) = sh.state.viewport_rect;
    let (w, hgt) = (x1 - x0, y1 - y0);
    let uv = interact::project_uv(&sh.state.cam.mvp(f64::from(w / hgt)), q).expect("on screen");
    egui::pos2(x0 + uv[0] * w, y0 + uv[1] * hgt)
}

/// The pixel over the middle of top-rim side `i` (outline `i` to `i + 1`).
fn side(sh: &ShellUi, i: usize) -> egui::Pos2 {
    let o = outline();
    let (a, b) = (o[i], o[(i + 1) % o.len()]);
    px_of(sh, [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, TOP])
}

/// A press and release at `p` with `m` held, then the frames that drain
/// what it queued.
fn click(h: &mut Harness<'static, ShellUi>, p: egui::Pos2, m: Modifiers) {
    h.input_mut().events.push(Event::ModifiersChanged(m));
    h.input_mut().events.push(Event::PointerMoved(p));
    h.run();
    for pressed in [true, false] {
        h.input_mut().events.push(Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: m });
        h.run();
    }
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
}

/// The prism beside the block, `op` open in Edge mode, the Top view in
/// orthographic framing it.
fn harness_for(kind: Option<OpType>) -> (Harness<'static, ShellUi>, String) {
    let mut h = shell_harness();
    let mut name = "Contour1".to_owned();
    {
        let st = &mut h.state_mut().state;
        st.history.execute(&st.world, ImportModel::from_mesh("prism", prism()));
        if let Some(kind) = kind {
            let seed = model::find_by_name(&st.world, "Contour1").and_then(|e| model::operation_of(&st.world, &e)).expect("seed op");
            name = "Op2".to_owned();
            st.pending.push_back(CommandKind::CreateOperation {
                name: name.clone(),
                tool: seed.tool.clone(),
                src: SrcRef::Stock,
                params: operation::retype_params(&seed.params, kind.ordinal()),
            });
            st.drain_actions();
        }
        crate::actions::target(st, UiTarget::Operation(name.clone()));
    }
    h.run();
    let st = &mut h.state_mut().state;
    st.geo_pick = GeoPick::Edge;
    let cam = crate::viewport::Camera::frame([X0 + 30.0, 20.0, 5.0], 60.0).toward([0.0, 0.0, 1.0]);
    st.cam = crate::viewport::Camera { projection: crate::viewport::ProjectionMode::Orthographic, ..cam };
    h.run();
    (h, name)
}

fn picks(h: &Harness<'static, ShellUi>, name: &str) -> Vec<GeoSel> {
    let st = &h.state().state;
    let op = model::find_by_name(&st.world, name).expect("op");
    match model::operation_of(&st.world, &op).map(|o| o.src) {
        Some(SrcRef::Selection(items)) => items,
        _ => Vec::new(),
    }
}

/// The edges (outline sides) a stored pick list covers on the prism's top rim.
fn count_on(h: &Harness<'static, ShellUi>, name: &str) -> usize {
    picks(h, name).iter().filter(|g| g.entity == "prism").count()
}

fn law(kind: Option<OpType>) {
    let (mut h, name) = harness_for(kind);
    // Plain click on A-B: that one edge, not the A-B-C tangent chain.
    let ab = side(h.state(), 0);
    click(&mut h, ab, Modifiers::NONE);
    let one = picks(&h, &name);
    assert_eq!(one.len(), 1, "a plain click takes one edge: {one:?} / {}", h.state().state.status);
    let first = one[0].clone();
    // Ctrl+click on B-C adds exactly that edge.
    let bc = side(h.state(), 1);
    click(&mut h, bc, Modifiers::COMMAND);
    let two = picks(&h, &name);
    assert_eq!(two.len(), 2, "Ctrl+click adds one edge: {two:?} / {} / {first:?}", h.state().state.status);
    assert_eq!(two[0], first, "the first pick stays");
    // Ctrl+click on it again takes it back out.
    click(&mut h, bc, Modifiers::COMMAND);
    assert_eq!(picks(&h, &name), vec![first.clone()], "Ctrl+click toggles");
    // Shift+click on a rim edge takes the whole loop of the top face.
    let ed = side(h.state(), 3);
    click(&mut h, ed, Modifiers::SHIFT);
    assert_eq!(count_on(&h, &name), 5, "Shift takes the loop's five runs: {:?}", picks(&h, &name));
    // A plain click replaces everything with that one edge.
    click(&mut h, ab, Modifiers::NONE);
    assert_eq!(picks(&h, &name), vec![first], "a plain click replaces the selection");
}

#[test]
fn contour_click_ctrl_and_shift_pick_the_same_every_time() {
    law(None);
}

#[test]
fn pocket_click_ctrl_and_shift_pick_the_same_every_time() {
    law(Some(OpType::Pocket));
}

/// #1507: the pointer over a Geometry-list row's remove button (not only
/// its label) names that row's pick as the viewport's list hover, and
/// nothing once the pointer leaves. Hover is view state: no command runs.
#[test]
fn hovering_a_selection_rows_remove_button_highlights_that_pick() {
    let (mut h, name) = harness_for(None);
    let ab = side(h.state(), 0);
    click(&mut h, ab, Modifiers::NONE);
    let want = picks(&h, &name)[0].clone();
    h.run_steps(2);
    let at = h.get_by_label("×").rect().center();
    h.input_mut().events.push(Event::PointerMoved(at));
    h.run_steps(2);
    assert_eq!(h.state().state.geo_hover, Some(want), "the row's pick is the highlighted one");
    let history = h.state().state.history.is_dirty();
    h.input_mut().events.push(Event::PointerMoved(egui::pos2(1.0, 1.0)));
    h.run_steps(2);
    assert_eq!(h.state().state.geo_hover, None, "the highlight goes when the pointer leaves");
    assert_eq!(h.state().state.history.is_dirty(), history, "hover is not a command");
}

/// The open boundary's picks in the page draft.
fn open_picks(st: &crate::state::State) -> Vec<GeoSel> {
    st.op_ui.area.open.as_ref().map(|o| o.picks.clone()).unwrap_or_default()
}

/// #1727: with the toolbar in Faces mode, an armed boundary row still takes
/// the edge under the cursor (it took faces only, so no edge could bound
/// the cut), and Ctrl adds a second, separate edge. Apply commits the two
/// edges as one undoable edit; undo takes them back out. The click goes
/// through the viewport's own feature-click route (`feature_click::click`)
/// at the pixel over each edge, between frames, as the shell calls it.
#[test]
fn an_armed_boundary_row_takes_edges_in_faces_mode() {
    let (mut h, name) = harness_for(Some(OpType::Face));
    h.set_size(egui::vec2(1440.0, 1100.0));
    h.run_steps(2);
    let boundary = h.get_by_label("Boundary").rect();
    click(&mut h, egui::pos2(boundary.left() - 10.0, boundary.center().y), Modifiers::NONE);
    h.run_steps(2);
    let (near, far) = (side(h.state(), 0), side(h.state(), 3));
    let st = &mut h.state_mut().state;
    st.geo_pick = GeoPick::Face;
    st.boundary_pick = Some(crate::state::BoundaryPick::Open);
    h.run_steps(2);
    let st = &mut h.state_mut().state;
    let ctrl = crate::interact::Mods { command: true, ..Default::default() };
    crate::feature_click::click(st, [near.x, near.y], crate::interact::Mods::default());
    let one = open_picks(st);
    assert_eq!(one.len(), 1, "a click takes one pick: {one:?} / {}", st.status);
    assert!(matches!(one[0].feature, operation::FeatureSel::Chain(_)), "the pick is an edge, not the face: {one:?}");
    crate::feature_click::click(st, [far.x, far.y], ctrl);
    let two = open_picks(st);
    assert_eq!(two.len(), 2, "Ctrl adds the far edge: {two:?} / {}", st.status);
    assert!(two.iter().all(|g| matches!(g.feature, operation::FeatureSel::Chain(_))), "both picks are edges: {two:?}");
    assert!(st.status.contains("2 open chains"), "the entry reads as two chains: {}", st.status);
    h.run_steps(2);
    let texts = super::structural::painted(&h);
    assert!(texts.iter().any(|(s, _)| s == "2 edges, 2 open chains"), "the Boundary row renders the two-chain entry: {texts:?}");
    assert_eq!(h.state().state.geo_pick, GeoPick::Face, "boundary picking keeps Faces mode");
    let st = &mut h.state_mut().state;
    st.apply_operation_params(&name);
    st.drain_actions();
    let stored = |st: &crate::state::State| {
        model::find_by_name(&st.world, &name).and_then(|e| model::operation_of(&st.world, &e)).and_then(|o| o.area.open).map(|o| o.picks)
    };
    assert_eq!(stored(st), Some(two), "Apply commits the two edges: {}", st.status);
    st.pending.push_back(CommandKind::Undo);
    st.drain_actions();
    assert_eq!(stored(st), None, "undo takes the boundary back out");
}
