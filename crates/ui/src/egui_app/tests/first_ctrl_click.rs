//! #1726's frame test: the first Ctrl+click on a second edge adds it, in
//! a perspective view where edges at different heights are under the
//! cursor. A box beside the seed's block, a 2D Offset open in Edge mode:
//! click the near top edge, Ctrl+click the bottom edge under it (the two
//! meet seen from above, and the pick door used to refuse the second),
//! and expect two edges in the operation, two rows in the Geometry list
//! and two chips. A plain click on an upright edge is a pick too. Every
//! change is the undoable geometry command: one undo takes one pick back.
//! Provenance: our own pick law test over our own shell, from the
//! maintainer's hand-test report; no vendor material consulted.

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::operation::{self, GeoSel, OpType};

use super::*;

const X0: f64 = 200.0;
const SIZE: f64 = 20.0;
const TOP: f64 = 10.0;

fn px_of(sh: &ShellUi, q: [f64; 3]) -> egui::Pos2 {
    let ([x0, y0], [x1, y1]) = sh.state.viewport_rect;
    let (w, hgt) = (x1 - x0, y1 - y0);
    let uv = interact::project_uv(&sh.state.cam.mvp(f64::from(w / hgt)), q).expect("on screen");
    egui::pos2(x0 + uv[0] * w, y0 + uv[1] * hgt)
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

fn picks(h: &Harness<'static, ShellUi>) -> Vec<GeoSel> {
    let st = &h.state().state;
    let op = model::find_by_name(&st.world, "Op2").expect("op");
    match model::operation_of(&st.world, &op).map(|o| o.src) {
        Some(SrcRef::Selection(items)) => items,
        _ => Vec::new(),
    }
}

/// The box beside the block, a 2D Offset `Op2` open in Edge mode, seen in
/// perspective from the front, the right and above.
fn harness() -> Harness<'static, ShellUi> {
    let mut h = shell_harness();
    {
        let st = &mut h.state_mut().state;
        let far = Point3::new(X0 + SIZE, SIZE, TOP);
        st.history.execute(&st.world, ImportModel::from_mesh("box", cube(Point3::new(X0, 0.0, 0.0), far)));
        let seed = model::find_by_name(&st.world, "Contour1").and_then(|e| model::operation_of(&st.world, &e)).expect("seed op");
        st.pending.push_back(CommandKind::CreateOperation {
            name: "Op2".into(),
            tool: seed.tool.clone(),
            src: SrcRef::Stock,
            params: operation::retype_params(&seed.params, OpType::Offset2D.ordinal()),
        });
        st.drain_actions();
        crate::actions::target(st, UiTarget::Operation("Op2".into()));
    }
    h.run();
    let st = &mut h.state_mut().state;
    st.geo_pick = GeoPick::Edge;
    st.cam = crate::viewport::Camera::frame([X0 + SIZE * 0.5, SIZE * 0.5, TOP * 0.5], 30.0).toward([0.5, -0.6, 0.62]);
    h.run();
    h
}

#[test]
fn the_first_ctrl_click_adds_the_edge_under_the_first_one() {
    let mut h = harness();
    let mid = X0 + SIZE * 0.5;
    let top = px_of(h.state(), [mid, 0.0, TOP]);
    click(&mut h, top, Modifiers::NONE);
    let one = picks(&h);
    assert_eq!(one.len(), 1, "a plain click takes one edge: {one:?} / {}", h.state().state.status);
    let under = px_of(h.state(), [mid, 0.0, 0.0]);
    click(&mut h, under, Modifiers::COMMAND);
    let two = picks(&h);
    assert_eq!(two.len(), 2, "the first Ctrl+click adds the second edge: {two:?} / {}", h.state().state.status);
    assert_eq!(two[0], one[0], "the first pick stays");
    assert_ne!(two[1], one[0], "the second pick is the other edge");
    assert_eq!(h.query_all_by_label("×").count(), 2, "one list row for each edge");
    assert_eq!(h.state().state.geo_chips().len(), 2, "one chip for each edge");
    // The add was one undoable command: undo takes exactly it back.
    let st = &mut h.state_mut().state;
    assert!(st.history.undo(&st.world));
    assert_eq!(picks(&h), one);
}

#[test]
fn a_plain_click_takes_an_upright_edge_and_ctrl_click_adds_to_it() {
    let mut h = harness();
    let upright = px_of(h.state(), [X0 + SIZE, 0.0, TOP * 0.5]);
    click(&mut h, upright, Modifiers::NONE);
    let one = picks(&h);
    assert_eq!(one.len(), 1, "an upright edge is a pick: {one:?} / {}", h.state().state.status);
    let top = px_of(h.state(), [X0 + SIZE * 0.5, 0.0, TOP]);
    click(&mut h, top, Modifiers::COMMAND);
    let two = picks(&h);
    assert_eq!(two.len(), 2, "Ctrl+click adds, it does not swap: {two:?} / {}", h.state().state.status);
    assert_eq!(two[0], one[0], "the first pick stays");
    assert_eq!(h.query_all_by_label("×").count(), 2, "one list row for each edge");
}
