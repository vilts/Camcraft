//! #558 in the viewport: with two setups holding the part differently,
//! the OTHER setup's WCS triad is drawn on its spot of the part as the
//! shown setup holds it - never at its raw stored place (the stock's far
//! corner in the report).
//!
//! Provenance: our own test; rigid-body transform algebra.

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Point3, cube, transform_point};
use camcraft_app::stock::StockShape;

use crate::interact::gizmos;
use crate::state::{CommandKind, State};

/// The top corner Setup1 zeroes on (document frame).
const CORNER: [f64; 3] = [40.0, 20.0, 10.0];

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

fn triads(state: &State) -> Vec<[f64; 3]> {
    gizmos(state).into_iter().filter(|g| !g.world_axes).map(|g| g.pos).collect()
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6)
}

#[test]
fn another_setups_wcs_triad_rides_the_part_into_the_shown_setup() {
    let mut state = State::new();
    let part = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(40.0, 20.0, 10.0));
    state.history.execute(&state.world, ImportModel::from_mesh("part", part));
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(&mut state, CommandKind::ActivateSetup(Some("Setup1".into())));
    let shape = StockShape::Box { dx: 60.0, dy: 60.0, dz: 12.0 };
    run(&mut state, CommandKind::SetStock { shape, pos: [0.0; 3], yaw_deg: 0.0 });
    run(&mut state, CommandKind::SetWcs { pos: CORNER, roll_deg: 0.0, pitch_deg: 0.0, yaw_deg: 0.0 });
    run(&mut state, CommandKind::ActivateSetup(Some("Setup2".into())));
    run(&mut state, CommandKind::AlignPart { setup: "Setup2".into(), angles: [0.0, 0.0, 90.0], pivot: Default::default() });
    let a2 = camcraft_app::setup_align::stored(&state.world, "Setup2").expect("Setup2 holds the part turned");
    let c = transform_point(Point3::new(CORNER[0], CORNER[1], CORNER[2]), a2.pos, a2.rot);
    let want = [c.x, c.y, c.z];
    state.toggle_setup_display("Setup1"); // #1494: another setup is drawn only with its eye on
    let shown = triads(&state);
    assert!(shown.iter().any(|p| near(*p, want)), "Setup1's WCS on the turned corner {want:?}; drawn: {shown:?}");
    assert!(!shown.iter().any(|p| near(*p, CORNER)), "never at its raw stored place: {shown:?}");

    // Showing Setup1 again: its WCS is back on the corner as modelled.
    run(&mut state, CommandKind::ActivateSetup(Some("Setup1".into())));
    assert!(triads(&state).iter().any(|p| near(*p, CORNER)), "its own setup: stored pose");
}
