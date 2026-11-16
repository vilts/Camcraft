//! #1762: a blind slot picked as three edges forms one U-shaped run.
//! Provenance: our own gridded block fixture and general pocket machining practice.
//! Gate map: covers app::operation::open_band, app::operation::selection,
//! app::operation::boundary, app::operation::keep_law, app::posting.

mod area_support;

use area_support::*;
use camcraft_app::commands::GenerateOperation;
use camcraft_app::geometry::{Mesh, Triangle};
use camcraft_app::posting;
use camcraft_app::verify::Warning;

/// A 40 x 40 x 20 block with a 16 mm blind slot, floor Z 10,
/// end wall Y 8, and mouth Y -20. Emit only exterior cell faces.
fn blind_block() -> Mesh {
    let xs = [-20.0, -8.0, 8.0, 20.0];
    let ys = [-20.0, 8.0, 20.0];
    let zs = [0.0, 10.0, 20.0];
    let filled = |i: i32, j: i32, k: i32| {
        (0..3).contains(&i)
            && (0..2).contains(&j)
            && (0..2).contains(&k)
            && !(i == 1 && j == 0 && k == 1)
    };
    let mut facets = Vec::new();
    for i in 0..3 {
        for j in 0..2 {
            for k in 0..2 {
                if !filled(i, j, k) {
                    continue;
                }
                let p = |a: usize, b: usize, c: usize| {
                    Point3::new(xs[i as usize + a], ys[j as usize + b], zs[k as usize + c])
                };
                for (di, dj, dk, q) in [
                    (-1, 0, 0, [p(0, 0, 0), p(0, 0, 1), p(0, 1, 1), p(0, 1, 0)]),
                    (1, 0, 0, [p(1, 0, 0), p(1, 1, 0), p(1, 1, 1), p(1, 0, 1)]),
                    (0, -1, 0, [p(0, 0, 0), p(1, 0, 0), p(1, 0, 1), p(0, 0, 1)]),
                    (0, 1, 0, [p(0, 1, 0), p(0, 1, 1), p(1, 1, 1), p(1, 1, 0)]),
                    (0, 0, -1, [p(0, 0, 0), p(0, 1, 0), p(1, 1, 0), p(1, 0, 0)]),
                    (0, 0, 1, [p(0, 0, 1), p(1, 0, 1), p(1, 1, 1), p(0, 1, 1)]),
                ] {
                    if !filled(i + di, j + dj, k + dk) {
                        facets.push(Triangle::new(q[0], q[1], q[2]));
                        facets.push(Triangle::new(q[0], q[2], q[3]));
                    }
                }
            }
        }
    }
    Mesh::from_facets(facets)
}

fn slot_world() -> World {
    world_with(
        Some(ImportModel::from_mesh("part", blind_block())),
        0.0,
        20.0,
    )
}

fn slot(world: &World) -> SrcRef {
    let e = model::find_by_name(world, "part").expect("part");
    let f = model::features_of(world, &e).expect("features");
    let picks: Vec<_> = feature::chains(&f)
        .into_iter()
        .filter(|c| {
            let pts = feature::chain_points(&f, c);
            !c.closed
                && pts.len() >= 2
                && pts.iter().all(|q| {
                    (q[2] - 20.0).abs() < 1e-6
                        && q[1] <= 8.0 + EPS
                        && q[0].abs() <= 8.0 + EPS
                        && ((q[0].abs() - 8.0).abs() < EPS || (q[1] - 8.0).abs() < EPS)
                })
        })
        .map(|c| GeoSel {
            entity: "part".into(),
            feature: FeatureSel::Chain(c.id),
        })
        .collect();
    assert_eq!(
        picks.len(),
        3,
        "the slot has two side edges and one end edge"
    );
    SrcRef::Selection(picks)
}

fn params(kind: OpType) -> OpParams {
    let mut p = operation::retype_params(&pocket(20.0, 10.0), kind as usize);
    match &mut p {
        OpParams::Offset2D {
            top_z,
            bottom_z,
            stepdown,
            stepover,
            ..
        }
        | OpParams::Adaptive2D {
            top_z,
            bottom_z,
            stepdown,
            stepover,
            ..
        } => {
            *top_z = Height::fixed(20.0);
            *bottom_z = Height::fixed(10.0);
            *stepdown = 5.0;
            *stepover = 4.0;
        }
        _ => {}
    }
    if let OpParams::Adaptive2D {
        stepdown,
        optimal_load,
        ..
    } = &mut p
    {
        *stepdown = 10.0;
        *optimal_load = 0.4;
    }
    p
}

#[test]
fn blind_slot_pocket_family_cuts_from_end_wall_to_stock_edge() {
    let world = slot_world();
    for kind in [OpType::Offset2D, OpType::Pocket, OpType::Adaptive2D] {
        for extension in [0.0, 7.0] {
            let SrcRef::Selection(picks) = slot(&world) else {
                unreachable!()
            };
            let area = AreaControls {
                extension,
                ..Default::default()
            };
            let (path, warnings) = run(&world, &op(SrcRef::Selection(picks), params(kind), area))
                .unwrap_or_else(|e| panic!("{kind:?}, extension {extension}: {e}"));
            assert_eq!(warnings, 0, "{kind:?}");
            let cuts: Vec<_> = feeds(&path)
                .into_iter()
                .filter(|p| p.z < 20.0 - EPS)
                .collect();
            assert!(!cuts.is_empty(), "{kind:?}");
            assert!(
                cuts.iter().all(|p| p.x.abs() <= 8.0 - R + EPS
                    && p.y <= 8.0 - R + EPS
                    && p.z >= 10.0 - EPS),
                "the tool stays off all three walls: {kind:?}"
            );
            let low = -max_by(&cuts, |p| -p.y);
            assert!(
                (low - (-35.0 + R - extension)).abs() < 0.05,
                "{kind:?}: mouth reaches Y {low}, extension {extension}"
            );
            assert!(
                max_by(&cuts, |p| p.y) > 2.0,
                "{kind:?}: clears to the end wall"
            );
        }
    }
}

#[test]
fn blind_slot_reversed_picks_keep_the_same_region() {
    let world = slot_world();
    let SrcRef::Selection(mut picks) = slot(&world) else {
        unreachable!()
    };
    let before = operation::selection_account(&world, None, &picks, &params(OpType::Offset2D));
    picks.reverse();
    let after = operation::selection_account(&world, None, &picks, &params(OpType::Offset2D));
    assert_eq!(before.1, None);
    assert!(
        before
            .0
            .iter()
            .any(|note| note.contains("into a blind slot")),
        "{:?}",
        before.0
    );
    assert_eq!(before, after);
}

#[test]
fn blind_slot_missing_side_wall_is_refused() {
    let world = slot_world();
    let SrcRef::Selection(mut picks) = slot(&world) else {
        unreachable!()
    };
    let e = model::find_by_name(&world, "part").expect("part");
    let f = model::features_of(&world, &e).expect("features");
    picks.retain(|pick| {
        let FeatureSel::Chain(id) = pick.feature else {
            unreachable!()
        };
        let c = feature::resolve_chain(&f, id).expect("chain");
        !feature::chain_points(&f, &c)
            .iter()
            .all(|q| (q[0] - 8.0).abs() < EPS)
    });
    assert_eq!(picks.len(), 2);
    let err = run(
        &world,
        &op(
            SrcRef::Selection(picks),
            params(OpType::Offset2D),
            AreaControls::default(),
        ),
    )
    .expect_err("two adjacent walls do not define a blind slot");
    assert!(
        err.contains("open") && (err.contains("run") || err.contains("slot")),
        "{err}"
    );
}

#[test]
fn blind_slot_pass_through_kept_part_blocks_post() {
    let world = slot_world();
    let mut h = History::default();
    h.execute(
        &world,
        ImportModel::from_mesh(
            "clamp",
            cube(Point3::new(-4.0, -32.0, 0.0), Point3::new(4.0, -24.0, 20.0)),
        ),
    );
    h.execute(
        &world,
        CreateOperation::new(&world, "P1", TOOL, slot(&world), params(OpType::Offset2D))
            .expect("create"),
    );
    h.execute(
        &world,
        GenerateOperation::new(&world, "P1").expect("generate"),
    );
    let e = model::find_by_name(&world, "P1").expect("operation");
    let tp = model::toolpath_of(&world, &e).expect("cached path");
    assert!(!tp.path.moves.is_empty());
    assert!(
        tp.warnings
            .iter()
            .any(|w| matches!(w, Warning::Gouge { .. }) && w.blocks_post()),
        "{:?}",
        tp.warnings
    );
    let out = std::env::temp_dir().join(format!("camcraft-blind-slot-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let err = posting::post_setup(&world, &out).expect_err("a gouge blocks posting");
    assert!(
        err.contains("post blocked") && err.contains("gouge"),
        "{err}"
    );
    assert!(!out.exists());
}
