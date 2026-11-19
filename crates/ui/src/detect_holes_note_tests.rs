//! #1556, #1566: the Detect holes button on a Bore op keeps a wide hole
//! (the concentric ring plan clears it), leaves out a hole that needs more
//! rings than the plan allows, and the status line names that hole - the same
//! sentence the MCP `detect_holes` tool answers in `skipped`.
//!
//! Provenance: our own tests over our own detect-holes rule; no vendor
//! material consulted.

use camcraft_app::commands::{AddTool, CreateOperation, ImportModel};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::tools::catalog;

use crate::state::{ErrorScope, State, UiTarget};

/// A Ø10 bull-nose with a 4.999 mm corner (the catalog's `BB-1000-R1`
/// with its corner opened up). Its flat bottom is R0.001, so at a flat
/// blind floor the ring plan steps 0.0008 mm and a Ø24 hole needs 8750
/// rings - past the 4096 budget (the limit is Ø16.554). At a through
/// hole's floor plane (the tip 1 mm under it) the corner reaches R3.001,
/// so a Ø74.3 through hole takes 14 rings. (A ball nose would show the
/// zero-band refusal, but the Bore tool door refuses a ball first.)
const BASE: &str = "BB-1000-R1";
const TOOL: &str = "BB-1000-R4999";
const CORNER: f64 = 4.999;
const H: f64 = 50.0;
const N: usize = 64;

/// One cell of the plate: the bore radius, and the floor height of a
/// blind bore (`None` bores through).
type Cell = (f64, Option<f64>);

/// Push the triangle `p q r`, wound so its normal points along `want`.
fn tri(facets: &mut Vec<Triangle>, p: Point3, q: Point3, r: Point3, want: [f64; 3]) {
    let (u, v) = (
        [q.x - p.x, q.y - p.y, q.z - p.z],
        [r.x - p.x, r.y - p.y, r.z - p.z],
    );
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let dot = n[0] * want[0] + n[1] * want[1] + n[2] * want[2];
    facets.push(if dot >= 0.0 {
        Triangle::new(p, q, r)
    } else {
        Triangle::new(p, r, q)
    });
}

fn quad(facets: &mut Vec<Triangle>, a: Point3, b: Point3, c: Point3, d: Point3, want: [f64; 3]) {
    tri(facets, a, b, c, want);
    tri(facets, a, c, d, want);
}

/// 100 x 100 x 10 cells along X, each bored at its centre with the given
/// radius, through or down to a flat floor; bore vertices pushed radially
/// onto the cell's square so neighbours share their edge vertices.
fn plate(cells: &[Cell]) -> Mesh {
    let mut facets = Vec::new();
    let n_cells = cells.len();
    for (k, &(rad, floor)) in cells.iter().enumerate() {
        let cx = (2.0 * k as f64 + 1.0) * H - n_cells as f64 * H;
        let ang = |i: usize| std::f64::consts::TAU * (i % N) as f64 / N as f64;
        let bp = |i: usize, z: f64| Point3::new(cx + rad * ang(i).cos(), rad * ang(i).sin(), z);
        let sq = |i: usize, z: f64| {
            let (x, y) = (ang(i).cos(), ang(i).sin());
            let s = x.abs().max(y.abs());
            Point3::new(cx + x * H / s, y * H / s, z)
        };
        let wall_bottom = floor.unwrap_or(0.0);
        for i in 0..N {
            let j = i + 1;
            quad(
                &mut facets,
                bp(i, 10.0),
                bp(j, 10.0),
                sq(j, 10.0),
                sq(i, 10.0),
                [0.0, 0.0, 1.0],
            );
            match floor {
                None => quad(
                    &mut facets,
                    bp(i, 0.0),
                    bp(j, 0.0),
                    sq(j, 0.0),
                    sq(i, 0.0),
                    [0.0, 0.0, -1.0],
                ),
                Some(zf) => {
                    // The bore's flat floor faces up; the plate's
                    // bottom under it is solid.
                    tri(
                        &mut facets,
                        Point3::new(cx, 0.0, zf),
                        bp(i, zf),
                        bp(j, zf),
                        [0.0, 0.0, 1.0],
                    );
                    tri(
                        &mut facets,
                        Point3::new(cx, 0.0, 0.0),
                        sq(i, 0.0),
                        sq(j, 0.0),
                        [0.0, 0.0, -1.0],
                    );
                }
            }
            let mid = ang(i) + std::f64::consts::PI / N as f64;
            quad(
                &mut facets,
                bp(i, 10.0),
                bp(j, 10.0),
                bp(j, wall_bottom),
                bp(i, wall_bottom),
                [-mid.cos(), -mid.sin(), 0.0],
            );
            let (a, b) = (sq(i, 10.0), sq(j, 10.0));
            let on = |x: f64| (a.x - x).abs() < 1e-9 && (b.x - x).abs() < 1e-9;
            if !((k > 0 && on(cx - H)) || (k + 1 < n_cells && on(cx + H))) {
                quad(
                    &mut facets,
                    a,
                    b,
                    sq(j, 0.0),
                    sq(i, 0.0),
                    [(a.x + b.x) * 0.5 - cx, (a.y + b.y) * 0.5, 0.0],
                );
            }
        }
    }
    Mesh::from_facets(facets)
}

fn bore_state(cells: &[Cell]) -> State {
    let mut state = State::new();
    state
        .history
        .execute(&state.world, ImportModel::from_mesh("Plate", plate(cells)));
    let mut tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == BASE)
        .map(|(_, t)| t)
        .expect("catalog entry");
    tool.corner_r = CORNER;
    assert!(
        tool.is_sane() && tool.diameter == 10.0,
        "the opened-up Ø10 bull-nose is a valid tool"
    );
    state.history.execute(
        &state.world,
        AddTool::new(&state.world, TOOL, tool).expect("tool"),
    );
    let params = OpParams::Bore {
        break_through: None,
        links: camcraft_app::links::Links::PLANAR,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(10.0),
        bottom_z: None,
        stock_to_leave: 0.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 150.0,
        climb: true,
        tolerance: 0.01,
        holes: Vec::new(),
    };
    let src = SrcRef::Entity("Plate".into());
    state.history.execute(
        &state.world,
        CreateOperation::new(&state.world, "BOR", TOOL, src, params).expect("create"),
    );
    state.ui_target = UiTarget::Operation("BOR".into());
    state.sync_ops_ui();
    state
}

/// The stored holes as (diameter, through); `None` when BOR is gone or
/// is not a Bore op.
fn stored_holes(state: &State) -> Option<Vec<(f64, bool)>> {
    match model::find_by_name(&state.world, "BOR")
        .and_then(|e| model::operation_of(&state.world, &e))
        .map(|o| o.params)
    {
        Some(OpParams::Bore { holes, .. }) => {
            Some(holes.iter().map(|h| (h.radius * 2.0, h.through)).collect())
        }
        _ => None,
    }
}

#[test]
fn detect_holes_on_a_bore_op_stores_the_wide_through_hole_and_names_the_one_past_the_ring_budget() {
    // #1566: the Ø74.3 through hole is about seven tool diameters; the
    // ring plan clears it in 14 rings, so it is stored. The Ø24 blind hole
    // needs 8750 rings at its flat floor: it is left out, by name.
    let mut state = bore_state(&[(37.15, None), (12.0, Some(4.0))]);
    state.detect_holes("BOR");
    state.drain_actions();
    let holes = stored_holes(&state).expect("BOR is a Bore op");
    assert_eq!(holes.len(), 1, "only the through hole is stored: {holes:?}");
    assert!(
        (holes[0].0 - 74.3).abs() < 0.05 && holes[0].1,
        "the wide through hole stays: {holes:?}"
    );
    for want in [
        "holes detected for BOR",
        "24.000 mm hole at (50.000, 0.000) was skipped",
        "wider than the 16.554 mm a 10.000 mm tool clears within 4096 rings",
    ] {
        assert!(
            state.status.contains(want),
            "the status names `{want}`: {}",
            state.status
        );
    }
    assert!(
        !state.status.contains("74.300 mm hole"),
        "the wide through hole is not skipped: {}",
        state.status
    );
}

#[test]
fn detect_holes_on_a_bore_op_with_only_a_hole_past_the_ring_budget_is_refused() {
    let mut state = bore_state(&[(12.0, Some(4.0))]);
    state.detect_holes("BOR");
    state.drain_actions();
    assert_eq!(
        stored_holes(&state).map(|h| h.len()),
        Some(0),
        "a refused detection stores nothing"
    );
    let msg = state.error_for(ErrorScope::Operation).unwrap_or_default();
    assert!(
        msg.contains("no hole this Bore op can make") && msg.contains("24.000 mm hole"),
        "{msg}"
    );
    assert!(
        msg.contains("within 4096 rings"),
        "the refusal says why: {msg}"
    );
}

/// #2496: a Drill op with the 6 mm catalog drill on the plate.
fn drill_state(cells: &[Cell]) -> State {
    let mut state = State::new();
    state.history.execute(&state.world, ImportModel::from_mesh("Plate", plate(cells)));
    let tool = catalog().into_iter().find(|(n, _)| *n == "DR-0600").map(|(_, t)| t).expect("catalog DR-0600");
    state.history.execute(&state.world, AddTool::new(&state.world, "DR-0600", tool).expect("tool"));
    let params = OpParams::Drill {
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(10.0),
        bottom_z: None,
        cycle: camcraft_app::drilling::CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 130.0,
        plunge_feed: 100.0,
        holes: Vec::new(),
        cross: Vec::new(),
        break_through: None,
        break_through_from: camcraft_app::drilling::BreakThroughFrom::FullDiameter,
    };
    let src = SrcRef::Entity("Plate".into());
    state.history.execute(&state.world, CreateOperation::new(&state.world, "DRL", "DR-0600", src, params).expect("create"));
    state.ui_target = UiTarget::Operation("DRL".into());
    state.sync_ops_ui();
    state
}

#[test]
fn detect_holes_on_a_drill_op_keeps_a_wider_hole_and_names_it_undersize() {
    // #2496: the D8 hole stays (a pilot drill is valid), and the status
    // line names it - the sentence MCP `detect_holes` answers in
    // `undersize`. The D6 hole is the drill's own size: not named.
    let mut state = drill_state(&[(4.0, None), (3.0, None)]);
    state.detect_holes("DRL");
    state.drain_actions();
    let stored = match model::find_by_name(&state.world, "DRL").and_then(|e| model::operation_of(&state.world, &e)).map(|o| o.params) {
        Some(OpParams::Drill { holes, .. }) => holes.len(),
        _ => usize::MAX,
    };
    assert_eq!(stored, 2, "both holes are stored");
    for want in ["holes detected for DRL", "the 8.000 mm hole at (-50.000, 0.000) is wider than the 6.000 mm drill"] {
        assert!(state.status.contains(want), "the status names `{want}`: {}", state.status);
    }
    assert!(!state.status.contains("6.000 mm hole"), "the D6 hole is not named: {}", state.status);
}
