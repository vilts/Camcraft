//! Provenance: original closed D50 drilled-bar regression for #2601.
//! Gate map: covers camcraft_app::operation::material, camcraft_app::operation::cross_reach.

mod cross_support;
mod punched_bar;

use cross_support::*;

#[test]
fn mill_detection_names_opposite_blind_radial_holes_and_stores_none() {
    let expected: Vec<_> = [-1.0, 1.0]
        .into_iter()
        .map(|side| CrossHole {
            origin: Point3::new(side * 25.0, 0.0, 0.0),
            dir: Point3::new(-side, 0.0, 0.0),
            radius: 2.5,
            entry_d: 0.0,
            floor_d: -8.0,
            through: false,
        })
        .collect();
    let world = new_world();
    let mut history = History::default();
    let cmd = LoadMachine::new(&world, "bench.mch".into(), Some(machine("mill")));
    history.execute(&world, cmd.expect("machine"));
    add_tool(&world, &mut history, "DR-0400");
    history.execute(
        &world,
        ImportModel::from_mesh(
            "bar",
            punched_bar::punched_bar(25.0, -40.0, 40.0, &expected),
        ),
    );
    let source = SrcRef::Entity("bar".into());
    // The recognizer keeps each opposite blind hole with its own depth.
    let cross = operation::cross_holes_of(&world, None, &source).expect("radial holes read");
    assert_eq!(cross.len(), 2, "{cross:?}");
    for hole in &expected {
        assert!(cross.contains(hole), "missing {hole:?}: {cross:?}");
    }
    // #3448: a plain mill has no turret, so Detect holes names both
    // holes and stores nothing, instead of a cross list Generate refuses.
    let error = operation::detect_holes(&world, "D", &cross_params(vec![], None), None, &source)
        .expect_err("a plain mill has no turret");
    assert_eq!(error.matches("radial hole at").count(), 2, "{error}");
    assert!(error.contains("no turret"), "{error}");
}
