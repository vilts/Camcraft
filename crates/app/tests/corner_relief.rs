//! Provenance: independent corner-relief cutting, simulation and golden CL tests.
//! Gate map (#2400): covers app::operation::corner_relief, app::operation::corner_relief_sim,
//! app::commands (CreateOperation, GenerateOperation with corner relief).

mod common;
mod corner_relief_support;
mod pocket_support;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{CreateOperation, GenerateOperation};
use camcraft_app::geo2d::Point2;
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::model;
use camcraft_app::operation::{Height, HeightReference};
use camcraft_app::recognize;
use camcraft_app::stock_sim::simulate_document;
use camcraft_app::verify::Warning;
use common::{gen_to_cltext, write_golden};
use corner_relief_support::{TOOL, base_pocket, patch_pocket, setup_world};
use pocket_support::{block_with_pocket, rounded_rect};
use serde_json::json;

const TOOL_R: f64 = 3.0;

fn min_dist_at_floor(tp: &Toolpath, corner: Point2, floor_z: f64) -> f64 {
    let mut best = f64::INFINITY;
    for m in &tp.moves {
        if m.kind != MoveKind::Feed {
            continue;
        }
        if (m.from.z - floor_z).abs() > 1e-3 || (m.to.z - floor_z).abs() > 1e-3 {
            continue;
        }
        let ax = m.from.x;
        let ay = m.from.y;
        let bx = m.to.x;
        let by = m.to.y;
        let dx = bx - ax;
        let dy = by - ay;
        let len2 = dx * dx + dy * dy;
        let t = if len2 <= 1e-18 {
            0.0
        } else {
            (((corner.x - ax) * dx + (corner.y - ay) * dy) / len2).clamp(0.0, 1.0)
        };
        let px = ax + t * dx;
        let py = ay + t * dy;
        best = best.min((corner.x - px).hypot(corner.y - py));
    }
    best
}

fn any_feed_outside_x(tp: &Toolpath) -> (f64, f64) {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for m in &tp.moves {
        if m.kind == MoveKind::Feed {
            lo = lo.min(m.from.x.min(m.to.x));
            hi = hi.max(m.from.x.max(m.to.x));
        }
    }
    (lo, hi)
}

fn any_feed_outside_y(tp: &Toolpath) -> (f64, f64) {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for m in &tp.moves {
        if m.kind == MoveKind::Feed {
            lo = lo.min(m.from.y.min(m.to.y));
            hi = hi.max(m.from.y.max(m.to.y));
        }
    }
    (lo, hi)
}

#[test]
fn square_cad_pocket_dogbone_clears_corners_and_verifies_without_gouge() {
    // 60 x 40 x 20 block with a 20 x 20 square pocket (floor z 10, top z 20).
    let mesh = block_with_pocket(30.0, 20.0, &rounded_rect(10.0, 10.0, 0.0, 1), 10.0, 20.0);
    let (world, mut h) = setup_world(Some(mesh), 60.0, 40.0, 20.0);
    let rec = recognize::pockets_of(&world, "part").expect("recognize");
    assert_eq!(rec.len(), 1);
    let base = base_pocket(
        PocketStrategy::Waterline,
        Height::of(HeightReference::BottomOfSelection, 10.0),
        Height::of(HeightReference::BottomOfSelection, 0.0),
        5.0,
    );
    let (src, base_op) = recognize::operation_for(&rec[0], &base).expect("op for pocket");
    let params = patch_pocket(&base_op, json!({ "corner_relief": { "kind": "dogbone" } }))
        .expect("corner_relief param supported");

    h.execute(&world, CreateOperation::new(&world, "Pocket1", TOOL, src, params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Pocket1").expect("generate"));

    let e = model::find_by_name(&world, "Pocket1").expect("op entity");
    let cache = model::toolpath_of(&world, &e).expect("toolpath");
    assert!(
        !cache.warnings.iter().any(|w| matches!(w, Warning::Gouge { .. } | Warning::Crossing { .. })),
        "unexpected verify warnings: {:?}",
        cache.warnings
    );

    // Every square corner of [-10, 10]^2 must be reached at floor z = 10.0 by a center
    // within TOOL_R (<= 3.001 mm) so a 20x20 square mating part fits.
    for (cx, cy) in [(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)] {
        let d = min_dist_at_floor(&cache.path, Point2::new(cx, cy), 10.0);
        assert!(d <= TOOL_R + 1e-3, "corner ({cx}, {cy}) left uncut: center dist {d}");
    }

    // Program stock simulation must pass with no unexcused gouge.
    let sim = simulate_document(&world).expect("simulate_document");
    assert!(
        sim.dev.max_gouge <= sim.dev.tol,
        "corner relief exceeded bounds: max_gouge={} tol={}",
        sim.dev.max_gouge,
        sim.dev.tol
    );
}

#[test]
fn square_slot_tbone_rule_and_per_corner_override() {
    // 60 x 40 x 20 block with a 30 x 12 square-ended slot (x in [-15, 15], y in [-6, 6], floor z 10).
    let mesh = block_with_pocket(30.0, 20.0, &rounded_rect(15.0, 6.0, 0.0, 1), 10.0, 20.0);
    let (world, mut h) = setup_world(Some(mesh), 60.0, 40.0, 20.0);
    let rec = recognize::pockets_of(&world, "part").expect("recognize");
    assert_eq!(rec.len(), 1);
    let base = base_pocket(
        PocketStrategy::Waterline,
        Height::of(HeightReference::BottomOfSelection, 10.0),
        Height::of(HeightReference::BottomOfSelection, 0.0),
        10.0,
    );
    let (src, base_op) = recognize::operation_for(&rec[0], &base).expect("op for slot");

    // 1. Long-edge rule: travels along the 30 mm X walls so Y walls stay flush at |y - tool_r| <= 3.0
    //    while X reaches +/-15.0 (cutting the short end walls).
    let long_edge = patch_pocket(
        &base_op,
        json!({ "corner_relief": { "kind": "t_bone", "tbone_direction": "long_edge" } }),
    )
    .expect("long_edge corner_relief supported");
    h.execute(&world, CreateOperation::new(&world, "SlotTBone", TOOL, src.clone(), long_edge).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "SlotTBone").expect("generate"));
    let e = model::find_by_name(&world, "SlotTBone").expect("op");
    let cache = model::toolpath_of(&world, &e).expect("toolpath");
    assert!(cache.warnings.is_empty(), "warnings: {:?}", cache.warnings);
    for (cx, cy) in [(-15.0, -6.0), (15.0, -6.0), (15.0, 6.0), (-15.0, 6.0)] {
        let d = min_dist_at_floor(&cache.path, Point2::new(cx, cy), 10.0);
        assert!(d <= TOOL_R + 1e-3, "slot corner ({cx}, {cy}) not cleared: {d}");
    }
    let (min_x, max_x) = any_feed_outside_x(&cache.path);
    let (min_y, max_y) = any_feed_outside_y(&cache.path);
    assert!(min_x <= -14.99 && max_x >= 14.99, "T-bone must extend into X end walls: {min_x}..{max_x}");
    assert!(min_y >= -3.001 && max_y <= 3.001, "long Y walls must stay un-notched: {min_y}..{max_y}");

    // 2. Vertical rule with per-corner overrides: (15, 6) = dogbone, (-15, -6) = none.
    let overrides = patch_pocket(
        &base_op,
        json!({
            "corner_relief": {
                "kind": "t_bone",
                "tbone_direction": "vertical",
                "overrides": [
                    { "at": [15.0, 6.0], "choice": "dogbone" },
                    { "at": [-15.0, -6.0], "choice": "none" }
                ]
            }
        }),
    )
    .expect("per-corner overrides supported");
    h.execute(&world, CreateOperation::new(&world, "SlotOverride", TOOL, src, overrides).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "SlotOverride").expect("generate"));
    let e2 = model::find_by_name(&world, "SlotOverride").expect("op2");
    let cache2 = model::toolpath_of(&world, &e2).expect("toolpath2");
    assert!(cache2.warnings.is_empty(), "warnings: {:?}", cache2.warnings);
    assert!(min_dist_at_floor(&cache2.path, Point2::new(15.0, 6.0), 10.0) <= TOOL_R + 1e-3);
    assert!(min_dist_at_floor(&cache2.path, Point2::new(-15.0, 6.0), 10.0) <= TOOL_R + 1e-3);
    // (-15, -6) left unrelieved by override -> center stops sqrt(2)*3 = 4.2426 away.
    let skipped = min_dist_at_floor(&cache2.path, Point2::new(-15.0, -6.0), 10.0);
    assert!(skipped > 4.0, "override none must leave (-15, -6) unrelieved: {skipped}");
}

#[test]
fn pocket_waterline_square_dogbone_matches_golden() {
    let mesh = block_with_pocket(30.0, 20.0, &rounded_rect(10.0, 10.0, 0.0, 1), 10.0, 20.0);
    let (world, mut h) = setup_world(Some(mesh), 60.0, 40.0, 20.0);
    let rec = recognize::pockets_of(&world, "part").expect("recognize");
    let base = base_pocket(
        PocketStrategy::Waterline,
        Height::of(HeightReference::BottomOfSelection, 10.0),
        Height::of(HeightReference::BottomOfSelection, 0.0),
        10.0,
    );
    let (src, base_op) = recognize::operation_for(&rec[0], &base).expect("op");
    let params = patch_pocket(&base_op, json!({ "corner_relief": { "kind": "dogbone" } }))
        .expect("corner_relief param supported");
    h.execute(&world, CreateOperation::new(&world, "GoldenDogbone", TOOL, src, params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "GoldenDogbone").expect("generate"));
    let e = model::find_by_name(&world, "GoldenDogbone").expect("op");
    let cache = model::toolpath_of(&world, &e).expect("toolpath");
    assert!(cache.warnings.is_empty(), "golden warnings: {:?}", cache.warnings);
    write_golden("pocket_waterline_square_dogbone.cl", &gen_to_cltext(&cache.path));
}

#[test]
fn corner_relief_test_modules_stay_under_the_line_law() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    for file in [
        "corner_relief.rs",
        "corner_relief_safety.rs",
        "corner_relief_serialization.rs",
        "corner_relief_support/mod.rs",
    ] {
        let text = std::fs::read_to_string(root.join(file)).expect("corner relief test module");
        let lines = text.lines().count();
        assert!(lines <= 400, "{file} has {lines} lines; split the module before it exceeds 400");
    }
}
