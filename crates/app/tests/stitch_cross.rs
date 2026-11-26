//! Cross-entity open-run stitching (M13.11, the M1.5 leftover): picked
//! runs of two models that meet end to end compose into one ring on the
//! same weld lattice and orientation law as one model's runs, and the
//! contour over that ring is the contour over the one-block ring of the
//! union; a three-way junction and runs on different planes refuse by
//! name; the open chain of an open boundary joins across models; undoing
//! the pick restores the per-entity state. Provenance: our own tests,
//! specced from the stitcher's contract.
//!
//! Gate map (M13.11): covers app::operation (stitch, stitch_cross,
//! selection, picks, boundary_picks), app::commands (SetOperationGeometry,
//! MoveModel, CreateOperation).

mod area_support;

use area_support::*;
use camcraft_app::commands::{MoveModel, SetOperationGeometry};
use camcraft_app::operation::{PickShape, boundary_pick_shape};

/// A 20 x 40 x 10 half block, `x0..x1` by -20..20 by 0..10.
fn half(x0: f64, x1: f64) -> camcraft_app::geometry::Mesh {
    cube(Point3::new(x0, -20.0, 0.0), Point3::new(x1, 20.0, 10.0))
}

/// Two half blocks butted edge to edge along x = 0 inside the 70 x 70
/// stock: 'a' as modelled on the west, 'b' modelled in a's place and
/// MOVED east by 20 - so b's picks read through the transform door, and
/// `lift` raises b off a's plane.
fn two_boxes(lift: f64) -> World {
    let world = world_with(Some(ImportModel::from_mesh("a", half(-20.0, 0.0))), 0.0, 10.0);
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("b", half(-20.0, 0.0)));
    let at = Location { pos: Point3::new(20.0, 0.0, lift), ..Default::default() };
    h.execute(&world, MoveModel::new(&world, "b", at).expect("b placed"));
    world
}

/// The open run of `name`'s top rim (local Z 10) whose LOCAL points all
/// satisfy `pick`.
fn edge(world: &World, name: &str, pick: impl Fn(&[f64; 3]) -> bool) -> GeoSel {
    let e = model::find_by_name(world, name).expect("model");
    let f = model::features_of(world, &e).expect("features");
    let c = feature::chains(&f)
        .into_iter()
        .find(|c| !c.closed && {
            let p = feature::chain_points(&f, c);
            p.len() >= 2 && p.iter().all(|q| (q[2] - 10.0).abs() < 1e-9 && pick(q))
        })
        .expect("the rim edge is its own open chain");
    GeoSel { entity: name.into(), feature: FeatureSel::Chain(c.id) }
}

/// a's three outer top edges (north, west, south), in local = world XY.
fn a_rim(world: &World) -> Vec<GeoSel> {
    vec![
        edge(world, "a", |q| (q[1] - 20.0).abs() < 1e-9),
        edge(world, "a", |q| (q[0] + 20.0).abs() < 1e-9),
        edge(world, "a", |q| (q[1] + 20.0).abs() < 1e-9),
    ]
}

/// b's three outer top edges (north, east, south); b's local x = 0 is
/// world x = 20.
fn b_rim(world: &World) -> Vec<GeoSel> {
    vec![
        edge(world, "b", |q| (q[1] - 20.0).abs() < 1e-9),
        edge(world, "b", |q| q[0].abs() < 1e-9),
        edge(world, "b", |q| (q[1] + 20.0).abs() < 1e-9),
    ]
}

fn union_rim(world: &World) -> Vec<GeoSel> {
    let mut p = a_rim(world);
    p.extend(b_rim(world));
    p
}

fn bbox(pts: &[Point3]) -> (f64, f64, f64, f64) {
    (max_by(pts, |p| -p.x), max_by(pts, |p| p.x), max_by(pts, |p| -p.y), max_by(pts, |p| p.y))
}

#[test]
fn runs_across_two_boxes_spell_one_ring() {
    let world = two_boxes(0.0);
    let picks = union_rim(&world);
    let params = contour(10.0, 0.0, Side::Outside);
    // One ring, the 40 x 40 union, every pick consumed by ring 0.
    let region = operation::region_of(&world, &op(SrcRef::Selection(picks.clone()), params.clone(), Default::default()))
        .expect("the runs of both boxes bound one area");
    assert_eq!(region.loops().len(), 1, "one ring across the two models");
    assert!((region.loops()[0].area().abs() - 1600.0).abs() < 1e-6, "the union's 40 x 40");
    assert_eq!(operation::stitched_rings(&world, &picks), vec![Some(0); 6]);
    let notes = operation::selection_notes(&world, None, &picks, &params);
    assert!(notes.iter().any(|n| n.contains("6 open runs stitched into 1 ring")), "{notes:?}");

    // The contour over it is the contour over the one-block ring, at
    // EVERY level: the depth law reads the two bodies as their union
    // (each sliced on its own, the sections united - `contour_depth`
    // over `slice::material_above_of`), so the coincident seam wall at
    // x = 0 is interior, not a non-manifold refusal that stops the
    // ladder at the top level.
    let (across, wa) = run(&world, &op(SrcRef::Selection(picks), params.clone(), Default::default())).expect("cut");
    let (single, ws) = run(&block_world(), &one_block_op(&params)).expect("cut");
    assert_same_cut(&across, &single);
    assert_eq!(wa, ws, "the two-body cut verifies as the one-block cut does: {:?}", across.notes);
    assert!(
        !across.notes.iter().any(|n| n.contains("levels below skipped")),
        "no level is skipped on the union: {:?}",
        across.notes
    );
}

/// The one 40 x 40 block's outer rim contour, the reference cut.
fn one_block_op(params: &OpParams) -> Operation {
    let one = block_world();
    let rim: Vec<GeoSel> = [
        rim_edge(&one, |q| (q[1] - 20.0).abs() < 1e-9),
        rim_edge(&one, |q| (q[0] - 20.0).abs() < 1e-9),
        rim_edge(&one, |q| (q[1] + 20.0).abs() < 1e-9),
        rim_edge(&one, |q| (q[0] + 20.0).abs() < 1e-9),
    ]
    .into_iter()
    .map(|f| GeoSel { entity: "part".into(), feature: f })
    .collect();
    op(SrcRef::Selection(rim), params.clone(), Default::default())
}

/// `across` is `single`'s cut: the same levels, and at each level the
/// same feed moves in XY, move for move; the same extent and length.
fn assert_same_cut(across: &Toolpath, single: &Toolpath) {
    let (za, zs) = (feed_levels(across), feed_levels(single));
    assert_eq!(za.len(), zs.len(), "the same levels: {za:?} vs {zs:?}");
    // #2004: Top Z 10 is the part top and the stepdown is the full
    // height, so the ladder is the one level Z 0 (the air level at Z 10
    // is gone). The one-block ring must cut below its top.
    assert!(!za.is_empty() && za.iter().all(|z| *z < 10.0 - EPS), "the one-block ring cuts below its top: {zs:?}");
    for (a, s) in za.iter().zip(&zs) {
        assert!((a - s).abs() < EPS, "level for level: {za:?} vs {zs:?}");
        let (fa, fs) = (level_feeds(across, *a), level_feeds(single, *s));
        assert!(fa.len() >= 8, "the level at Z {a} is walked: {fa:?}");
        assert_eq!(fa.len(), fs.len(), "the same moves as the one-block cut at Z {a}");
        for (a, s) in fa.iter().zip(&fs) {
            assert!(
                (a.0.x - s.0.x).abs() < EPS && (a.0.y - s.0.y).abs() < EPS && (a.1.x - s.1.x).abs() < EPS && (a.1.y - s.1.y).abs() < EPS,
                "move for move the one-block cut: {a:?} vs {s:?}"
            );
        }
        let (la, ls) = (feed_length_at(across, *a), feed_length_at(single, *s));
        assert!((la - ls).abs() < EPS, "the level's length matches the one-block cut: {la} vs {ls}");
    }
    let (ba, bs) = (bbox(&feeds(across)), bbox(&feeds(single)));
    for (x, y) in [(ba.0, bs.0), (ba.1, bs.1), (ba.2, bs.2), (ba.3, bs.3)] {
        assert!((x - y).abs() < EPS, "the cut's extent matches the one-block cut: {ba:?} vs {bs:?}");
    }
}

/// The distinct planes of the level feeds (both ends at one Z), descending.
fn feed_levels(t: &Toolpath) -> Vec<f64> {
    let mut zs: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && (m.from.z - m.to.z).abs() < EPS)
        .map(|m| m.to.z)
        .collect();
    zs.sort_by(|a, b| b.total_cmp(a));
    zs.dedup_by(|a, b| (*a - *b).abs() < EPS);
    zs
}

/// The feed moves of the level at `z`, as (from, to).
fn level_feeds(t: &Toolpath, z: f64) -> Vec<(Point3, Point3)> {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && (m.from.z - z).abs() < EPS && (m.to.z - z).abs() < EPS)
        .map(|m| (m.from, m.to))
        .collect()
}

/// Every open chain of `name`'s top rim (local Z 10) that lies on the
/// 40 x 40 outline - its points AND its segment midpoints on |x| = 20
/// or |y| = 20, which keeps a seam's top edge (ends on the rim, middle
/// across it) out of the picks.
fn outline_chains(world: &World, name: &str) -> Vec<GeoSel> {
    let on_rim = |q: [f64; 3]| (q[0].abs() - 20.0).abs() < 1e-9 || (q[1].abs() - 20.0).abs() < 1e-9;
    let e = model::find_by_name(world, name).expect("model");
    let f = model::features_of(world, &e).expect("features");
    feature::chains(&f)
        .into_iter()
        .filter(|c| !c.closed && {
            let p = feature::chain_points(&f, c);
            p.len() >= 2
                && p.iter().all(|q| (q[2] - 10.0).abs() < 1e-9 && on_rim(*q))
                && p.windows(2).all(|w| on_rim([(w[0][0] + w[1][0]) * 0.5, (w[0][1] + w[1][1]) * 0.5, 10.0]))
        })
        .map(|c| GeoSel { entity: name.into(), feature: FeatureSel::Chain(c.id) })
        .collect()
}

#[test]
fn touching_closed_solids_in_one_model_cut_as_their_union() {
    // #2845: one model can contain two complete closed solids. Their
    // shared face is interior to the union, just as when each solid is
    // imported as its own model. The cut must match the one-block cut.
    let mut facets: Vec<_> = half(-20.0, 0.0).facets.to_vec();
    facets.extend(half(0.0, 20.0).facets.iter().copied());
    let world = world_with(
        Some(ImportModel::from_mesh("ab", camcraft_app::geometry::Mesh::from_facets(facets))),
        0.0,
        10.0,
    );
    let picks = outline_chains(&world, "ab");
    assert!(picks.len() >= 4, "the outline's runs: {picks:?}");
    let params = contour(10.0, 0.0, Side::Outside);
    let o = op(SrcRef::Selection(picks), params.clone(), Default::default());
    assert_eq!(operation::region_of(&world, &o).expect("the outline bounds one area").loops().len(), 1);
    let (across, wa) = run(&world, &o).expect("the closed solids cut as their union");
    let (single, ws) = run(&block_world(), &one_block_op(&params)).expect("reference cut");
    assert_same_cut(&across, &single);
    assert_eq!(wa, ws, "the union verifies as the one-block cut does: {:?}", across.notes);
    assert!(
        !across.notes.iter().any(|n| n.contains("levels below skipped")),
        "no level is skipped on the union: {:?}",
        across.notes
    );
}

fn feed_length_at(t: &Toolpath, z: f64) -> f64 {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && (m.from.z - z).abs() < EPS && (m.to.z - z).abs() < EPS)
        .map(|m| ((m.to.x - m.from.x).powi(2) + (m.to.y - m.from.y).powi(2)).sqrt())
        .sum()
}

#[test]
fn a_three_way_junction_refuses_by_name() {
    let world = two_boxes(0.0);
    let mut h = History::default();
    // A third block north of b, whose west edge leaves the (0, 20)
    // corner the a/b seam already meets at.
    h.execute(
        &world,
        ImportModel::from_mesh("c", cube(Point3::new(0.0, 20.0, 0.0), Point3::new(20.0, 40.0, 10.0))),
    );
    let mut picks = union_rim(&world);
    picks.push(edge(&world, "c", |q| q[0].abs() < 1e-9));
    let params = contour(10.0, 0.0, Side::Outside);
    let err = run(&world, &op(SrcRef::Selection(picks.clone()), params.clone(), Default::default()))
        .expect_err("three runs at one point is no ring");
    assert!(err.contains("meet 3 ways at (0.000, 20.000)"), "{err}");
    assert!(err.contains("'c'"), "the stray model is named: {err}");
    assert_eq!(operation::stitched_rings(&world, &picks), vec![None; 7], "a refusal owns no ring");
    let shape = boundary_pick_shape(&world, None, &picks, &params);
    assert!(matches!(shape, PickShape::Invalid(ref why) if why.contains("meet 3 ways")), "{shape:?}");
}

#[test]
fn runs_on_different_planes_refuse_by_name() {
    // b lifted 5: its rim meets a's in XY and stands 5 above it.
    let world = two_boxes(5.0);
    let picks = union_rim(&world);
    let err = run(&world, &op(SrcRef::Selection(picks.clone()), contour(10.0, 0.0, Side::Outside), Default::default()))
        .expect_err("a ring that steps between planes is refused");
    assert!(err.contains("different planes"), "{err}");
    assert!(err.contains("Z 10.000 to 15.000"), "{err}");
    assert!(err.contains("'a'") && err.contains("'b'"), "{err}");
    assert_eq!(operation::stitched_rings(&world, &picks), vec![None; 6]);
}

#[test]
fn an_open_boundary_chain_joins_across_models() {
    let world = two_boxes(0.0);
    let picks = vec![
        edge(&world, "a", |q| (q[1] - 20.0).abs() < 1e-9),
        edge(&world, "b", |q| (q[1] - 20.0).abs() < 1e-9),
    ];
    match boundary_pick_shape(&world, None, &picks, &pocket(10.0, 0.0)) {
        PickShape::Open { picks: n, chain } => {
            assert_eq!(n, 2);
            assert_eq!(chain.len(), 3, "one chain through the seam vertex: {chain:?}");
            let xs: Vec<f64> = chain.iter().map(|p| p.x).collect();
            assert!(xs.iter().cloned().fold(f64::INFINITY, f64::min) + 20.0 < EPS);
            assert!(xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max) - 20.0 < EPS);
            assert!(chain.iter().all(|p| (p.y - 20.0).abs() < EPS));
        }
        other => panic!("one open chain across the two models, got {other:?}"),
    }
}

#[test]
fn undoing_the_cross_pick_restores_the_per_entity_state() {
    let world = two_boxes(0.0);
    let mut h = History::default();
    let params = contour(10.0, 0.0, Side::Outside);
    let before = a_rim(&world);
    h.execute(
        &world,
        CreateOperation::new(&world, "cut", TOOL, SrcRef::Selection(before.clone()), params.clone()).expect("op"),
    );
    let read = |world: &World| {
        let e = model::find_by_name(world, "cut").expect("op");
        model::operation_of(world, &e).expect("operation")
    };
    assert!(operation::region_of(&world, &read(&world)).is_none(), "three runs of one box bound nothing");
    assert_eq!(operation::stitched_rings(&world, &before), vec![None; 3]);

    let after = union_rim(&world);
    h.execute(&world, SetOperationGeometry::new(&world, "cut", after.clone()).expect("pick"));
    let o = read(&world);
    assert_eq!(o.src, SrcRef::Selection(after.clone()));
    assert_eq!(operation::region_of(&world, &o).expect("ring").loops().len(), 1);
    assert_eq!(operation::stitched_rings(&world, &after), vec![Some(0); 6]);

    assert!(h.undo(&world));
    let o = read(&world);
    assert_eq!(o.src, SrcRef::Selection(before.clone()), "undo restores a's own picks");
    assert!(operation::region_of(&world, &o).is_none(), "and the per-entity state: no ring");
    assert_eq!(operation::stitched_rings(&world, &before), vec![None; 3]);

    assert!(h.redo(&world));
    let o = read(&world);
    assert_eq!(operation::region_of(&world, &o).expect("ring again").loops().len(), 1);
}
