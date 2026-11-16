//! A Contour on an open edge of a part (#1777): the tool stands one
//! radius off the edge on the side without material, whatever the Side
//! setting says, so the pass never cuts the part the edge bounds - and
//! the program posts clean. Before the fix the tool centre rode the edge,
//! the verifier flagged the gouge and the post was blocked.
//!
//! Gate map (#1777): covers app::operation (generate), motion
//! machining2d (contour_run_side), app::posting.

mod area_support;

use area_support::*;
use camcraft_app::commands::GenerateOperation;
use camcraft_app::posting;

/// Generate "C1" on the block from `picks` and return its cut feeds
/// (below the block top), its warning count and its notes.
fn cut(picks: Vec<GeoSel>, side: Side) -> (Vec<Point3>, usize, Vec<String>) {
    let world = block_world();
    let mut h = History::default();
    let create = CreateOperation::new(&world, "C1", TOOL, SrcRef::Selection(picks), contour(10.0, 0.0, side));
    h.execute(&world, create.expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "C1").expect("generate"));
    let e = model::find_by_name(&world, "C1").expect("op");
    let c = model::toolpath_of(&world, &e).expect("a toolpath");
    let below: Vec<Point3> = feeds(&c.path).into_iter().filter(|p| p.z < 10.0 - EPS).collect();
    assert!(
        posting::post_setup_unwritten(&world).is_ok(),
        "the program posts: {:?}",
        posting::post_setup_unwritten(&world).err()
    );
    (below, c.warnings.len(), c.path.notes.clone())
}

fn north(world: &World) -> FeatureSel {
    rim_edge(world, |q| (q[1] - 20.0).abs() < 1e-9)
}

fn east(world: &World) -> FeatureSel {
    rim_edge(world, |q| (q[0] - 20.0).abs() < 1e-9)
}

/// How far `p` stands outside the 40 x 40 block.
fn off_block(p: &Point3) -> f64 {
    (p.x.abs() - 20.0).max(0.0).hypot((p.y.abs() - 20.0).max(0.0))
}

#[test]
fn an_open_edge_of_a_part_is_cut_on_its_air_side_for_every_side() {
    let w = block_world();
    for side in [Side::Auto, Side::Inside, Side::Outside] {
        let (f, warnings, notes) = cut(pick("part", north(&w)), side);
        assert!(!f.is_empty(), "{side:?}: the edge is cut");
        assert!(f.iter().all(|p| (p.y - (20.0 + R)).abs() < EPS), "{side:?}: one radius north of the edge: {f:?}");
        assert!(f.iter().all(|p| p.x > -20.0 - EPS && p.x < 20.0 + EPS), "{side:?}: no further than the edge's ends");
        assert_eq!(warnings, 0, "{side:?}: nothing gouges");
        assert!(notes.iter().any(|n| n.contains("air side")), "{side:?}: {notes:?}");
    }
}

#[test]
fn two_open_edges_turn_the_corner_on_their_air_side() {
    let w = block_world();
    let mut picks = pick("part", north(&w));
    picks.extend(pick("part", east(&w)));
    let (f, warnings, _) = cut(picks, Side::Inside);
    assert!(!f.is_empty());
    assert!(f.iter().all(|p| off_block(p) > R - EPS), "the tool stays a radius off the block: {f:?}");
    assert_eq!(warnings, 0);
}
