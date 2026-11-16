//! #979's frame test: the viewport box select, through egui's own pointer
//! events. A plate with two square through-cutouts beside the seed's
//! block, seen from the Top in orthographic: a Ctrl + left drag (left to
//! right: a window) around one cutout in Edge mode draws a rubber band
//! instead of orbiting, and its release adds exactly that cutout's four
//! top-rim chains to Contour1 as ONE `SetOperationGeometry` - one undo
//! restores the seed's pick. A plain drag still orbits; a right-to-left
//! drag over one rim side is a crossing box and takes that side alone.
//! The box honours the Pick through toggle as a click does: off, the
//! hidden rims under the top face stay out; on, they are taken too.
//! Provenance: our own gesture test over our own shell; no vendor
//! material consulted.

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::operation::GeoSel;

use super::*;

type P = [f64; 3];

/// The plate's X offset: clear of the seed's 60 x 60 x 30 block.
const X0: f64 = 200.0;
const TOP: f64 = 10.0;

fn quad(out: &mut Vec<Triangle>, q: [P; 4], n: P) {
    let p = |v: P| Point3::new(v[0], v[1], v[2]);
    let c = (p(q[1]) - p(q[0])).cross(p(q[2]) - p(q[0]));
    let q = if c.x * n[0] + c.y * n[1] + c.z * n[2] < 0.0 { [q[0], q[3], q[2], q[1]] } else { q };
    out.push(Triangle::new(p(q[0]), p(q[1]), p(q[2])));
    out.push(Triangle::new(p(q[0]), p(q[2]), p(q[3])));
}

/// 100 x 60 x 10 on a 20 mm grid at X0, cutouts at cells (1, 1) and
/// (3, 1) - the app suite's `box_pick` plate, moved aside.
fn plate() -> Mesh {
    let holes = [(1, 1), (3, 1)];
    let solid = |i: i32, j: i32| (0..5).contains(&i) && (0..3).contains(&j) && !holes.contains(&(i, j));
    let mut t = Vec::new();
    for i in 0..5 {
        for j in 0..3 {
            if !solid(i, j) {
                continue;
            }
            let (x0, x1) = (X0 + 20.0 * f64::from(i), X0 + 20.0 * f64::from(i + 1));
            let (y0, y1) = (20.0 * f64::from(j), 20.0 * f64::from(j + 1));
            let sq = |z: f64| [[x0, y0, z], [x1, y0, z], [x1, y1, z], [x0, y1, z]];
            quad(&mut t, sq(TOP), [0.0, 0.0, 1.0]);
            quad(&mut t, sq(0.0), [0.0, 0.0, -1.0]);
            for (di, dj, a, b) in [(-1, 0, [x0, y0], [x0, y1]), (1, 0, [x1, y0], [x1, y1]), (0, -1, [x0, y0], [x1, y0]), (0, 1, [x0, y1], [x1, y1])] {
                if !solid(i + di, j + dj) {
                    let wall = [[a[0], a[1], 0.0], [b[0], b[1], 0.0], [b[0], b[1], TOP], [a[0], a[1], TOP]];
                    quad(&mut t, wall, [f64::from(di), f64::from(dj), 0.0]);
                }
            }
        }
    }
    Mesh::from_facets(t)
}

fn px_of(sh: &ShellUi, q: P) -> egui::Pos2 {
    let ([x0, y0], [x1, y1]) = sh.state.viewport_rect;
    let (w, hgt) = (x1 - x0, y1 - y0);
    let uv = interact::project_uv(&sh.state.cam.mvp(f64::from(w / hgt)), q).expect("on screen");
    egui::pos2(x0 + uv[0] * w, y0 + uv[1] * hgt)
}

/// The screen rect a world X/Y box at the top face draws as (the Top
/// view may stand at any quarter-turn roll).
fn screen_box(sh: &ShellUi, lo: [f64; 2], hi: [f64; 2]) -> egui::Rect {
    let c = [[lo[0], lo[1]], [hi[0], lo[1]], [hi[0], hi[1]], [lo[0], hi[1]]].map(|q| px_of(sh, [q[0], q[1], TOP]));
    egui::Rect::from_points(&c)
}

fn press(h: &mut Harness<'static, ShellUi>, p: egui::Pos2, pressed: bool, m: Modifiers) {
    h.input_mut().events.push(Event::ModifiersChanged(m));
    h.input_mut().events.push(Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: m });
    h.run();
}

fn move_to(h: &mut Harness<'static, ShellUi>, p: egui::Pos2, m: Modifiers) {
    h.input_mut().events.push(Event::ModifiersChanged(m));
    h.input_mut().events.push(Event::PointerMoved(p));
    h.run();
}

/// The plate beside the block, Contour1 open in Edge mode, the Top view
/// in orthographic framing the plate.
fn plate_harness() -> Harness<'static, ShellUi> {
    let mut harness = shell_harness();
    {
        let st = &mut harness.state_mut().state;
        st.history.execute(&st.world, ImportModel::from_mesh("plate", plate()));
        crate::actions::target(st, UiTarget::Operation("Contour1".into()));
    }
    harness.run();
    let st = &mut harness.state_mut().state;
    st.geo_pick = GeoPick::Edge;
    let cam = crate::viewport::Camera::frame([X0 + 50.0, 30.0, 5.0], 60.0).toward([0.0, 0.0, 1.0]);
    st.cam = crate::viewport::Camera { projection: crate::viewport::ProjectionMode::Orthographic, ..cam };
    harness.run();
    harness
}

fn contour1(h: &Harness<'static, ShellUi>) -> Vec<GeoSel> {
    let st = &h.state().state;
    let op = model::find_by_name(&st.world, "Contour1").expect("seed has Contour1");
    match model::operation_of(&st.world, &op).map(|o| o.src) {
        Some(SrcRef::Selection(items)) => items,
        _ => Vec::new(),
    }
}

/// A drag from `a` to `b` with `m` held, through a midpoint, then the
/// frames that drain what it queued.
fn drag(h: &mut Harness<'static, ShellUi>, a: egui::Pos2, b: egui::Pos2, m: Modifiers) {
    move_to(h, a, m);
    press(h, a, true, m);
    move_to(h, a + (b - a) * 0.5, m);
    move_to(h, b, m);
    {
        let st = &h.state().state;
        if m.command {
            assert!(st.pick_box.is_some(), "Ctrl + drag draws a pick box");
            assert_eq!(st.drag, None, "a pick box never orbits");
        }
    }
    press(h, b, false, m);
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
}

#[test]
fn a_ctrl_drag_window_around_one_cutout_adds_its_rim_in_one_undo_step() {
    let mut harness = plate_harness();
    let before = contour1(&harness);
    // Left to right: a window around the first cutout, [220, 240] x [20, 40].
    let r = screen_box(harness.state(), [X0 + 15.0, 15.0], [X0 + 45.0, 45.0]);
    let (a, b) = (r.left_top(), r.right_bottom());
    drag(&mut harness, a, b, Modifiers::COMMAND);
    let st = &harness.state().state;
    assert_eq!(st.pick_box, None, "the release ends the box");
    let after = contour1(&harness);
    assert_eq!(&after[..before.len()], &before[..], "the box adds to the picks");
    let added = &after[before.len()..];
    assert_eq!(added.len(), 4, "the cutout's four top-rim chains, nothing hidden: {added:?}");
    assert!(added.iter().all(|g| g.entity == "plate" && matches!(g.feature, FeatureSel::Chain(_))));
    // One undo restores the seed's pick.
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run_steps(2);
    assert_eq!(contour1(&harness), before, "undo takes the whole box back");
}

#[test]
fn a_right_to_left_drag_is_a_crossing_box_and_a_plain_drag_still_orbits() {
    let mut harness = plate_harness();
    let before = contour1(&harness);
    // Right to left over the first cutout's left side (x = 220).
    let r = screen_box(harness.state(), [X0 + 17.0, 25.0], [X0 + 23.0, 35.0]);
    let (a, b) = (r.right_top(), r.left_bottom());
    drag(&mut harness, a, b, Modifiers::COMMAND);
    let after = contour1(&harness);
    assert_eq!(after.len(), before.len() + 1, "the one rim side it crosses: {after:?}");
    // Without Ctrl the same drag orbits and picks nothing.
    let yaw0 = harness.state().state.cam.yaw;
    drag(&mut harness, a, b, Modifiers::NONE);
    assert_ne!(harness.state().state.cam.yaw, yaw0, "a plain drag orbits");
    assert_eq!(contour1(&harness), after, "an orbit picks nothing");
}

/// The same window around the first cutout, Pick through off then on.
fn window_with_pick_through(on: bool) -> Vec<GeoSel> {
    let mut harness = plate_harness();
    harness.state_mut().state.pick_through = on;
    let before = contour1(&harness);
    let r = screen_box(harness.state(), [X0 + 15.0, 15.0], [X0 + 45.0, 45.0]);
    drag(&mut harness, r.left_top(), r.right_bottom(), Modifiers::COMMAND);
    let after = contour1(&harness);
    assert_eq!(&after[..before.len()], &before[..], "the box adds to the picks");
    after[before.len()..].to_vec()
}

#[test]
fn the_box_honours_the_pick_through_toggle() {
    let visible = window_with_pick_through(false);
    assert_eq!(visible.len(), 4, "toggle off: the top rim only, nothing hidden: {visible:?}");
    let through = window_with_pick_through(true);
    // Toggle on: the top rim, the four vertical corners and the bottom
    // rim under the top face - every chain of the cutout.
    assert_eq!(through.len(), 12, "toggle on: hidden chains too: {through:?}");
    assert!(visible.iter().all(|g| through.contains(g)), "the visible ones stay in");
    assert!(through.iter().all(|g| g.entity == "plate" && matches!(g.feature, FeatureSel::Chain(_))));
}
