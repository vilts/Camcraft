//! #1347: re-applying the same two-axis part picks (#769) is no change.
//! The picks the held alignment already lands make no turn, so
//! `AlignPartToSetup::align_from_axes` (and its Apply check) refuses
//! "already holds the part that way" - never a no-op undo step built
//! from float noise. A real further turn from the same held alignment
//! still goes through, one undo step. Pinned on a 20 x 20 x 10 block.
//! Provenance: our own fixtures; no vendor material consulted.
//!
//! Gate map (#1347): covers app::pick::axis (align_from_axes),
//! app::commands (AlignPartToSetup::align_from_axes, check_align).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AlignPartToSetup, CreateSetup, History, ImportModel};
use camcraft_app::geometry::{Point3, cube, feature};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::{AxisPick, InPlaneAxes, InPlaneAxis};
use camcraft_app::setup_align;
use camcraft_app::world::new_world;

/// A 20 x 20 x 10 block at the origin, with Setup1.
fn block() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(
        &world,
        ImportModel::from_mesh("block", cube(Point3::new(0.0, 0.0, 0.0), Point3::new(20.0, 20.0, 10.0))),
    );
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    (world, h)
}

/// The block's edge between two LOCAL ends, picked running `from` -> `to`.
fn edge(world: &World, from: [f64; 3], to: [f64; 3]) -> AxisPick {
    let e = model::find_by_name(world, "block").expect("block");
    let f = model::features_of(world, &e).expect("features");
    let (id, flip) = feature::chains(&f)
        .into_iter()
        .find_map(|c| {
            let p = feature::chain_points(&f, &c);
            if p.len() != 2 {
                return None;
            }
            if p[0] == from && p[1] == to {
                Some((c.id, false))
            } else if p[0] == to && p[1] == from {
                Some((c.id, true))
            } else {
                None
            }
        })
        .expect("the block has that edge");
    AxisPick { picks: vec![GeoSel { entity: "block".into(), feature: FeatureSel::Chain(id) }], flip }
}

fn axes(y: AxisPick, x: AxisPick) -> InPlaneAxes {
    InPlaneAxes { y: Some(y), x: Some(x), primary: InPlaneAxis::Y }
}

/// The part laid on its side: +Y the upright picked up, +X the front
/// top edge picked +X (the case that left float noise in the origin).
fn on_its_side(w: &World) -> InPlaneAxes {
    axes(edge(w, [0.0, 0.0, 0.0], [0.0, 0.0, 10.0]), edge(w, [0.0, 0.0, 10.0], [20.0, 0.0, 10.0]))
}

#[test]
fn re_applying_the_same_two_axis_picks_refuses_as_already_held() {
    let (w, mut h) = block();
    let picks = on_its_side(&w);
    let cmd = AlignPartToSetup::align_from_axes(&w, "Setup1", &picks).expect("the first apply turns the part");
    h.execute(&w, cmd);
    let held = setup_align::stored(&w, "Setup1").expect("the part is held turned");

    // The same picks, read through the held alignment, already land.
    let msg = |r: Result<(), String>| r.expect_err("no change to make");
    let err = msg(AlignPartToSetup::align_from_axes(&w, "Setup1", &picks).map(|_| ()).map_err(|e| e.to_string()));
    assert!(err.contains("already holds the part that way"), "{err}");
    let err = msg(AlignPartToSetup::check_align(&w, "Setup1", &picks).map_err(|e| e.to_string()));
    assert!(err.contains("already holds the part that way"), "Apply greys out by name: {err}");
    assert_eq!(setup_align::stored(&w, "Setup1"), Some(held), "nothing was written");

    // One undo step - the first apply - takes it back to as modelled.
    assert!(h.undo(&w), "the apply is undoable");
    assert_eq!(setup_align::stored(&w, "Setup1"), None, "one step back is as modelled");
}

#[test]
fn a_real_further_turn_from_the_held_alignment_still_goes_through() {
    let (w, mut h) = block();
    let cmd = AlignPartToSetup::align_from_axes(&w, "Setup1", &on_its_side(&w)).expect("on its side");
    h.execute(&w, cmd);
    let held = setup_align::stored(&w, "Setup1").expect("held");
    // Different picks: the upright picked DOWN as +Y (the part turns on
    // over), the top edge picked -X as +X.
    let other = axes(edge(&w, [0.0, 0.0, 10.0], [0.0, 0.0, 0.0]), edge(&w, [20.0, 0.0, 10.0], [0.0, 0.0, 10.0]));
    let cmd = AlignPartToSetup::align_from_axes(&w, "Setup1", &other).expect("a real turn is accepted");
    h.execute(&w, cmd);
    let next = setup_align::stored(&w, "Setup1").expect("still held");
    assert_ne!(next, held, "the alignment moved on");
    assert!(h.undo(&w), "the further turn is one undo step");
    assert_eq!(setup_align::stored(&w, "Setup1"), Some(held), "undo restores the held alignment exactly");
}
