//! #3338: the Boundary group's two offsets. The picked-edge offset moves
//! the line that the user picked; the outer-boundary offset moves only
//! the stock side of an open boundary. A negative outer offset pulls the
//! air passes in and leaves the picked edge where it is; one value in
//! both (an old project's one offset) is the old law, byte for byte.
//!
//! Gate map: covers app::operation (boundary, boundary_ctx, types::area),
//! app::project (area), app::mcp (area).

mod area_support;

use area_support::*;

/// A Pocket of the strip between the block's north rim edge and the
/// stock (side B of the open boundary).
fn side_b(world: &World, offset: f64, outer_offset: f64) -> Operation {
    let north = rim_edge(world, |q| (q[1] - 20.0).abs() < 1e-9);
    let a = AreaControls {
        offset,
        outer_offset,
        open: Some(OpenBoundary { picks: pick("part", north), side: ChainSide::Outside }),
        ..Default::default()
    };
    op(SrcRef::Stock, pocket(10.0, 5.0), a)
}

fn bounds(world: &World, o: &Operation) -> (f64, f64, f64, f64) {
    let Some(r) = operation::region_of(world, o) else { panic!("no region: {:?}", run(world, o).err()) };
    let (lo, hi) = r.bounds().expect("bounds");
    (lo.x, lo.y, hi.x, hi.y)
}

fn feed_len(t: &Toolpath) -> f64 {
    t.moves.iter().filter(|m| m.kind == MoveKind::Feed).map(|m| (m.to.x - m.from.x).hypot(m.to.y - m.from.y).hypot(m.to.z - m.from.z)).sum()
}

#[test]
fn a_negative_outer_offset_pulls_in_the_stock_side_and_keeps_the_picked_edge() {
    let world = block_world();
    // Default: the strip runs from the chain (y 20) to the stock reach
    // (the 70 mm stock plus one tool diameter: +-45).
    let (x0, y0, x1, y1) = bounds(&world, &side_b(&world, 0.0, 0.0));
    assert!((y0 - 20.0).abs() < EPS && (y1 - 45.0).abs() < EPS && (x0 + 45.0).abs() < EPS && (x1 - 45.0).abs() < EPS, "{x0} {y0} {x1} {y1}");
    // Outer -10: the stock side comes back to the stock outline; the
    // picked edge stays on the chain.
    let (x0, y0, x1, y1) = bounds(&world, &side_b(&world, 0.0, -10.0));
    assert!((y0 - 20.0).abs() < EPS, "the picked edge stays on the chain: {y0}");
    assert!((y1 - 35.0).abs() < EPS && (x0 + 35.0).abs() < EPS && (x1 - 35.0).abs() < EPS, "the outer edge moves in: {x0} {y1} {x1}");
    // The picked offset alone moves the chain line and not the outer edge.
    let (x0, y0, x1, y1) = bounds(&world, &side_b(&world, -2.0, 0.0));
    assert!((y0 - 22.0).abs() < EPS, "the picked edge moves 2 mm into the kept side: {y0}");
    assert!((y1 - 45.0).abs() < EPS && (x0 + 45.0).abs() < EPS && (x1 - 45.0).abs() < EPS, "{x0} {y1} {x1}");
    // A positive picked offset over a negative outer one: each line by its own value.
    let (x0, y0, x1, y1) = bounds(&world, &side_b(&world, 1.0, -10.0));
    assert!((y0 - 19.0).abs() < EPS && (y1 - 35.0).abs() < EPS && (x1 - 35.0).abs() < EPS && (x0 + 35.0).abs() < EPS, "{x0} {y0} {x1} {y1}");
    // The cut-air length drops.
    let (wide, w0) = run(&world, &side_b(&world, 0.0, 0.0)).expect("default");
    let (tight, w1) = run(&world, &side_b(&world, 0.0, -10.0)).expect("tight");
    assert_eq!((w0, w1), (0, 0), "{:?} {:?}", wide.notes, tight.notes);
    let (lw, lt) = (feed_len(&wide), feed_len(&tight));
    eprintln!("feed length {lw:.1} -> {lt:.1} mm");
    assert!(lt < 0.7 * lw, "the feed length drops by a third or more: {lw:.1} -> {lt:.1} mm");
}

#[test]
fn one_value_in_both_offsets_is_the_old_law() {
    let world = block_world();
    // An old project's one offset of -2 moved both lines: the chain to
    // y 22 and the outer edge to y 43.
    let (x0, y0, x1, y1) = bounds(&world, &side_b(&world, -2.0, -2.0));
    assert!((y0 - 22.0).abs() < EPS && (y1 - 43.0).abs() < EPS && (x0 + 43.0).abs() < EPS && (x1 - 43.0).abs() < EPS, "{x0} {y0} {x1} {y1}");
    // A closed area (no open boundary) reads the picked offset only.

    let closed = |outer| op(SrcRef::Stock, pocket(10.0, 5.0), AreaControls { offset: -3.0, outer_offset: outer, ..Default::default() });
    let a = operation::region_of(&world, &closed(3.0)).expect("a");
    let b = operation::region_of(&world, &closed(0.0)).expect("b");
    assert!((a.area() - b.area()).abs() < 1e-9, "the outer offset does not touch a closed area");
}

#[test]
fn an_old_project_offset_loads_into_both_and_saves_the_same_bytes() {
    let world = block_world();
    let mut h = History::default();
    h.execute(&world, CreateOperation::new(&world, "P", TOOL, SrcRef::Stock, pocket(8.0, 4.0)).expect("create"));
    let north = rim_edge(&world, |q| (q[1] - 20.0).abs() < 1e-9);
    let open = Some(OpenBoundary { picks: pick("part", north), side: ChainSide::Outside });
    let legacy = AreaControls { offset: -1.5, outer_offset: -1.5, open: open.clone(), ..Default::default() };
    let e = model::find_by_name(&world, "P").expect("op");
    let params = model::operation_of(&world, &e).expect("op").params;
    h.execute(&world, EditOperationParams::with_area(&world, "P", params.clone(), legacy.clone()).expect("edit"));
    let dir = std::env::temp_dir().join(format!("camcraft-outer-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp");
    let file = dir.join("legacy.camproj");
    project::save(&world, &file).expect("save");
    let text = std::fs::read_to_string(&file).expect("read");
    // The file of an old project: one offset key, no outer key.
    assert!(text.contains("\"offset\": -1.5") && !text.contains("outer_offset"), "{text}");
    let back = new_world();
    project::load(&back, &file).expect("load");
    let e = model::find_by_name(&back, "P").expect("op");
    assert_eq!(model::operation_of(&back, &e).expect("op").area, legacy, "the one offset loads into both");
    let again = dir.join("again.camproj");
    project::save(&back, &again).expect("save");
    assert_eq!(std::fs::read(&file).expect("a"), std::fs::read(&again).expect("b"), "an old project saves the same bytes");
    // Two different values round-trip; an outer offset of 0 is written.
    let split = AreaControls { offset: -1.5, outer_offset: 0.0, open, ..Default::default() };
    h.execute(&world, EditOperationParams::with_area(&world, "P", params, split.clone()).expect("edit"));
    let file = dir.join("split.camproj");
    project::save(&world, &file).expect("save");
    assert!(std::fs::read_to_string(&file).expect("read").contains("\"outer_offset\": 0.0"));
    let back = new_world();
    project::load(&back, &file).expect("load");
    let e = model::find_by_name(&back, "P").expect("op");
    assert_eq!(model::operation_of(&back, &e).expect("op").area, split);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_outer_offset_rides_the_door_and_the_undo() {
    let world = block_world();
    let mut h = History::default();
    h.execute(&world, CreateOperation::new(&world, "P", TOOL, SrcRef::Stock, pocket(8.0, 4.0)).expect("create"));
    let e = model::find_by_name(&world, "P").expect("op");
    let o0 = model::operation_of(&world, &e).expect("op");
    let a = AreaControls { outer_offset: -4.0, ..Default::default() };
    assert!(a.has_boundary_edits() && !a.is_default());
    h.execute(&world, EditOperationParams::with_area(&world, "P", o0.params.clone(), a.clone()).expect("edit"));
    assert_eq!(model::operation_of(&world, &e).expect("op").area, a);
    assert!(h.undo(&world));
    assert_eq!(model::operation_of(&world, &e).expect("op").area, AreaControls::default());
    let bad = AreaControls { outer_offset: f64::INFINITY, ..Default::default() };
    assert!(EditOperationParams::with_area(&world, "P", o0.params, bad).is_err(), "a non-finite outer offset is refused");
}
