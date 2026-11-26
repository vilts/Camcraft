//! #3332: the exit pick on a selected DXF wire profile. A polygon chain
//! is left at the pick and passes the wire laws; a chain cut on its own
//! arcs refuses the pick by name, and never drops it.
//!
//! Gate map (#3332): covers app::operation (wire_op, wire_sketch,
//! wire_sketch_arcs, wire_verify).
//! Provenance: the suite's own synthetic drawing and fictional machine.

use super::*;

use camcraft_app::geo2d::Point2;
use camcraft_app::path_picks::PathPick;

fn points(world: &World) -> Vec<(f64, f64)> {
    let e = model::find_by_name(world, "Cut").expect("operation");
    let path = model::toolpath_of(world, &e).expect("cache").path;
    let mut v = vec![(path.moves[0].from.x, path.moves[0].from.y)];
    v.extend(path.moves.iter().map(|m| (m.to.x, m.to.y)));
    v
}

fn with_exit(x: f64, y: f64, side: WireSide) -> OpParams {
    let mut w = WireContourOp { side, ..wire_op() };
    w.picks.exit = vec![PathPick::xy(Point2::new(x, y))];
    OpParams::WireContour(w)
}

#[test]
fn a_polygon_chain_is_left_at_the_exit_pick_and_passes_the_wire_laws() {
    let (world, mut h) = document(DXF);
    h.execute(&world, GenerateOperation::new(&world, "Cut").expect("generate selected chain"));
    let auto = points(&world);
    let (start, foot) = (auto[0], auto[1]);
    assert_eq!(auto[auto.len() - 1], start, "without a pick the wire returns to the threading point");
    // The lead-in runs straight to the side that faces the start hole; an
    // exit 2 mm along that side leaves from the path 2 mm along it.
    let along = (foot.1 - start.1, start.0 - foot.0);
    let len = along.0.hypot(along.1);
    let (ux, uy) = (2.0 * along.0 / len, 2.0 * along.1 / len);
    let exit = (start.0 + ux, start.1 + uy);
    h.execute(&world, EditOperationParams::new(&world, "Cut", with_exit(exit.0, exit.1, WireSide::Punch)).expect("the exit pick is taken"));
    h.execute(&world, GenerateOperation::new(&world, "Cut").expect("generates with the exit pick"));
    let v = points(&world);
    let n = v.len();
    assert_eq!(v[n - 1], exit, "the cut ends at the exit pick");
    assert!((v[n - 2].0 - (foot.0 + ux)).abs() < 1e-9 && (v[n - 2].1 - (foot.1 + uy)).abs() < 1e-9, "left from the path's nearest point: {:?}", v[n - 2]);
    assert_eq!(&v[..auto.len() - 1], &auto[..auto.len() - 1], "the lap round the profile does not move");
    let e = model::find_by_name(&world, "Cut").expect("operation");
    let verdict = operation::wire_verify::wire_verdict(&world, &e).expect("wire").expect("verifiable");
    assert!(verdict.findings.is_empty() && verdict.captured && verdict.slugs.len() == 1, "{verdict:?}");
}

#[test]
fn a_chain_cut_on_its_arcs_refuses_the_exit_pick_by_name() {
    let (world, mut h) = document(DXF);
    h.execute(&world, SetOperationGeometry::new(&world, "Cut", vec![pick(&world, "other")]).expect("select circle"));
    h.execute(&world, EditOperationParams::new(&world, "Cut", with_exit(0.0, 0.0, WireSide::Die)).expect("the exit pick is stored"));
    let why = refusal(&world);
    assert!(why.contains("exit point") && why.contains("drawn profile with arcs") && why.contains("clear the exit point"), "{why}");
}
