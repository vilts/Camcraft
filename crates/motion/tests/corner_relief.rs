//! Unit and strategy tests for 2D pocket/slot corner relief (#2400).
//!
//! Gate map: covers camcraft_motion::machining2d (corner_relief, corner_relief_spur, rest2d).

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::Links;
use camcraft_motion::load::DEFAULT_OPTIMAL_LOAD;
use camcraft_motion::machining2d::{
    CornerRelief, CornerReliefChoice, CornerReliefKind, CornerReliefOverride, PocketParams,
    PocketStrategy, TBoneDirection, inside_corners, pocket_entered_with_relief, pocket_relief_plan,
};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
    .expect("rect loop")
}

fn square_pocket_params(strategy: PocketStrategy, blend_turns: bool) -> PocketParams {
    PocketParams {
        tool_diameter: 6.0,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: 25.0,
        top_z: 20.0,
        bottom_z: 15.0,
        stepdown: 5.0,
        feed: 1000.0,
        plunge_feed: 250.0,
        climb: true,
        strategy,
        retract_z: None,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
        blend_turns,
        blend_radius: None,
        links: Links::PLANAR,
    }
}

fn path_visits_xy(path: &Toolpath, target: Point2, tol: f64) -> bool {
    path.moves.iter().any(|mv| {
        (mv.to.x - target.x).hypot(mv.to.y - target.y) <= tol && mv.to.z <= 15.0 + 1e-6
    })
}

fn swept_reaches_corner(path: &Toolpath, corner: Point2, r: f64) -> bool {
    let min_dist = path
        .moves
        .iter()
        .filter(|mv| mv.kind == MoveKind::Feed && mv.to.z <= 15.0 + 1e-6)
        .map(|mv| (mv.to.x - corner.x).hypot(mv.to.y - corner.y))
        .fold(f64::INFINITY, f64::min);
    min_dist <= r + 0.02
}

#[test]
fn inside_corners_finds_rectangle_and_micro_fillet_corners() {
    let r = Region::new(vec![rect(0.0, 0.0, 30.0, 20.0)]).expect("rect region");
    let corners = inside_corners(&r);
    assert_eq!(corners.len(), 4, "rectangle has 4 inside corners");

    // L-shaped pocket: 5 convex inside corners need relief; the 1 reflex (>180 deg) corner does not.
    let l_shape = Loop::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(30.0, 0.0),
        Point2::new(30.0, 10.0),
        Point2::new(10.0, 10.0),
        Point2::new(10.0, 30.0),
        Point2::new(0.0, 30.0),
    ])
    .expect("L shape");
    let l_corners = inside_corners(&Region::new(vec![l_shape]).expect("L region"));
    assert_eq!(l_corners.len(), 5, "L-pocket has 5 convex inside corners and 1 reflex corner");

    // Tessellated 0.2 mm corner micro-fillets (30 deg chords) plan cleanly with a 6 mm cutter.
    let r_f = 0.2;
    let mut filleted = Vec::new();
    let centers = [
        (r_f, r_f, std::f64::consts::PI, 1.5 * std::f64::consts::PI),
        (30.0 - r_f, r_f, 1.5 * std::f64::consts::PI, std::f64::consts::TAU),
        (30.0 - r_f, 20.0 - r_f, 0.0, 0.5 * std::f64::consts::PI),
        (r_f, 20.0 - r_f, 0.5 * std::f64::consts::PI, std::f64::consts::PI),
    ];
    for (cx, cy, a0, a1) in centers {
        for k in 0..=3 {
            let a = a0 + (a1 - a0) * (k as f64 / 3.0);
            filleted.push(Point2::new(cx + r_f * a.cos(), cy + r_f * a.sin()));
        }
    }
    let micro_reg = Region::new(vec![Loop::new(filleted).expect("micro fillet loop")]).expect("micro region");
    assert!(
        pocket_relief_plan(&micro_reg, 6.0, 0.0, &CornerRelief { kind: CornerReliefKind::Dogbone, ..Default::default() })
            .is_ok(),
        "tessellated micro-fillet pocket must plan cleanly without false wall-crossing refusal"
    );

    // A 48-gon circle of r=10 mm needs 0 reliefs for a D=6 mm cutter (already reached within slack),
    // and a fine 256-gon circle (< 2 deg per chord) produces 0 sharp inside corners.
    let circle_reg = |n: usize| {
        let pts: Vec<Point2> = (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                Point2::new(15.0 + 10.0 * a.cos(), 10.0 + 10.0 * a.sin())
            })
            .collect();
        Region::new(vec![Loop::new(pts).expect("circle")]).expect("circle region")
    };
    let circ_48 = circle_reg(48);
    let circ_plan = pocket_relief_plan(
        &circ_48,
        6.0,
        0.0,
        &CornerRelief { kind: CornerReliefKind::Dogbone, ..Default::default() },
    )
    .expect("circle plan");
    assert!(circ_plan.reliefs.is_empty(), "circle needs 0 reliefs: {:?}", circ_plan.reliefs);
    assert!(inside_corners(&circle_reg(256)).is_empty(), "fine circle has no sharp corners");
}

#[test]
fn dogbone_and_tbone_rules_plan_cleanly_on_rectangle() {
    let reg = Region::new(vec![rect(0.0, 0.0, 40.0, 20.0)]).expect("rect");

    let dog = pocket_relief_plan(
        &reg,
        6.0,
        0.0,
        &CornerRelief { kind: CornerReliefKind::Dogbone, ..Default::default() },
    )
    .expect("dogbone plan");
    assert_eq!(dog.reliefs.len(), 4);
    for rel in &dog.reliefs {
        let d_corner = (rel.end.x - rel.corner.x).hypot(rel.end.y - rel.corner.y);
        assert!((d_corner - 3.0).abs() <= 0.02, "dogbone touches corner: {d_corner}");
        assert!((rel.radius - 3.0).abs() <= 1e-9);
    }

    let tb_long = pocket_relief_plan(
        &reg,
        6.0,
        0.0,
        &CornerRelief { kind: CornerReliefKind::TBone, tbone_direction: TBoneDirection::LongEdge, overrides: vec![] },
    )
    .expect("tbone long edge");
    assert_eq!(tb_long.reliefs.len(), 4);
    // On a 40x20 rect, LongEdge keeps the long 40mm X edges intact (notching the short vertical walls via a horizontal X spur),
    // whereas ShortEdge keeps the short 20mm Y edges intact (notching the long horizontal walls via a vertical Y spur).
    for rel in &tb_long.reliefs {
        let sx = (rel.end.x - rel.start.x).abs();
        let sy = (rel.end.y - rel.start.y).abs();
        assert!(sx > 0.5 && sy <= 0.02, "LongEdge spur extends horizontally along X: ({sx}, {sy})");
        assert!(rel.end.y == 3.0 || rel.end.y == 17.0, "LongEdge keeps Y walls unnotched: {:?}", rel.end);
    }

    let tb_short = pocket_relief_plan(
        &reg,
        6.0,
        0.0,
        &CornerRelief { kind: CornerReliefKind::TBone, tbone_direction: TBoneDirection::ShortEdge, overrides: vec![] },
    )
    .expect("tbone short edge");
    for rel in &tb_short.reliefs {
        let sx = (rel.end.x - rel.start.x).abs();
        let sy = (rel.end.y - rel.start.y).abs();
        assert!(sy > 0.5 && sx <= 0.02, "ShortEdge spur extends vertically along Y: ({sx}, {sy})");
        assert!(rel.end.x == 3.0 || rel.end.x == 37.0, "ShortEdge keeps X walls unnotched: {:?}", rel.end);
    }

    let tb_vert = pocket_relief_plan(
        &reg,
        6.0,
        0.0,
        &CornerRelief { kind: CornerReliefKind::TBone, tbone_direction: TBoneDirection::Vertical, overrides: vec![] },
    )
    .expect("tbone vertical");
    assert_eq!(tb_vert.reliefs, tb_short.reliefs, "Vertical matches Y-notching wall rule on axis-aligned box");

    let tb_horiz = pocket_relief_plan(
        &reg,
        6.0,
        0.0,
        &CornerRelief { kind: CornerReliefKind::TBone, tbone_direction: TBoneDirection::Horizontal, overrides: vec![] },
    )
    .expect("tbone horizontal");
    assert_eq!(tb_horiz.reliefs, tb_long.reliefs, "Horizontal matches X-notching wall rule on axis-aligned box");
}

#[test]
fn pocket_strategies_and_blend_turns_emit_relief_spurs() {
    let reg = Region::new(vec![rect(0.0, 0.0, 30.0, 30.0)]).expect("30x30");
    let relief = CornerRelief { kind: CornerReliefKind::Dogbone, ..Default::default() };
    let plan = pocket_relief_plan(&reg, 6.0, 0.0, &relief).expect("plan");

    for (strategy, blend) in [
        (PocketStrategy::Waterline, false),
        (PocketStrategy::Waterline, true),
        (PocketStrategy::Zigzag, false),
        (PocketStrategy::OffsetRings, false),
        (PocketStrategy::Adaptive, false),
    ] {
        let params = square_pocket_params(strategy, blend);
        let path = pocket_entered_with_relief(&reg, &params, &[], &relief)
            .unwrap_or_else(|e| panic!("{strategy:?} blend={blend} failed: {e}"));
        assert!(path.all_finite(), "{strategy:?} non-finite moves");
        for rel in &plan.reliefs {
            assert!(
                path_visits_xy(&path, rel.end, 1e-2),
                "{strategy:?} blend={blend} missed relief end {:?}",
                rel.end
            );
            assert!(
                swept_reaches_corner(&path, rel.corner, 3.0),
                "{strategy:?} blend={blend} did not clear corner {:?}",
                rel.corner
            );
        }
    }
}

#[test]
fn positive_stock_to_leave_offsets_relief_by_tool_plus_leave() {
    let reg = Region::new(vec![rect(0.0, 0.0, 40.0, 40.0)]).expect("40x40");
    let relief = CornerRelief { kind: CornerReliefKind::Dogbone, ..Default::default() };
    let mut params = square_pocket_params(PocketStrategy::Waterline, false);
    params.stock_to_leave = 0.5;
    let plan = pocket_relief_plan(&reg, params.tool_diameter, params.stock_to_leave, &relief).expect("plan");
    assert_eq!(plan.reliefs.len(), 4);
    for rel in &plan.reliefs {
        let d_corner = (rel.end.x - rel.corner.x).hypot(rel.end.y - rel.corner.y);
        assert!((rel.radius - 3.5).abs() <= 1e-9);
        assert!((d_corner - 3.5).abs() <= 0.02, "leave shifts relief endpoint to r + leave: {d_corner}");
    }
    let path = pocket_entered_with_relief(&reg, &params, &[], &relief).expect("pocket with leave");
    assert!(path.all_finite());
    for rel in &plan.reliefs {
        assert!(path_visits_xy(&path, rel.end, 1e-2));
    }
}

#[test]
fn unreachable_corners_and_bad_overrides_are_refused() {
    let narrow = Region::new(vec![rect(0.0, 0.0, 4.0, 20.0)]).expect("4x20");
    let relief = CornerRelief { kind: CornerReliefKind::Dogbone, ..Default::default() };
    let err = pocket_relief_plan(&narrow, 6.0, 0.0, &relief).expect_err("narrow pocket refuses");
    assert!(!err.to_string().is_empty());

    // Island leaving only a 1 mm channel next to the outer pocket corner blocks a relief spur
    // or renders the corner unreachable without violating the island boundary.
    let pocket_outer = rect(0.0, 0.0, 30.0, 30.0);
    let island = Loop::new(vec![
        Point2::new(7.5, 7.5),
        Point2::new(7.5, 22.5),
        Point2::new(22.5, 22.5),
        Point2::new(22.5, 7.5),
    ])
    .expect("island");
    // Gap between outer (0..30) and island (7.5..22.5) is 7.5 mm; diagonal near (0,0)->(7.5,7.5) is ~10.6 mm.
    // Shrink gap by shifting island corner to (4.5, 4.5): orthogonal clearance is 4.5 mm (< tool D=6).
    let tight_island = Loop::new(vec![
        Point2::new(4.5, 4.5),
        Point2::new(4.5, 25.5),
        Point2::new(25.5, 25.5),
        Point2::new(25.5, 4.5),
    ])
    .expect("tight island");
    let pinched = Region::new(vec![pocket_outer, tight_island]).expect("pinched");
    assert!(
        pocket_relief_plan(&pinched, 6.0, 0.0, &relief).is_err(),
        "corner blocked by close island must refuse"
    );

    let reg = Region::new(vec![rect(0.0, 0.0, 30.0, 30.0)]).expect("30x30");
    let unmatched = CornerRelief {
        kind: CornerReliefKind::None,
        tbone_direction: TBoneDirection::LongEdge,
        overrides: vec![CornerReliefOverride {
            at: Point2::new(99.0, 99.0),
            choice: CornerReliefChoice::Dogbone,
        }],
    };
    let err = pocket_relief_plan(&reg, 6.0, 0.0, &unmatched).expect_err("unmatched override refuses");
    assert!(err.to_string().contains("corner relief"), "{err}");

    let dup = CornerRelief {
        kind: CornerReliefKind::None,
        tbone_direction: TBoneDirection::LongEdge,
        overrides: vec![
            CornerReliefOverride { at: Point2::new(0.0, 0.0), choice: CornerReliefChoice::Dogbone },
            CornerReliefOverride { at: Point2::new(0.01, 0.01), choice: CornerReliefChoice::None },
        ],
    };
    assert!(pocket_relief_plan(&reg, 6.0, 0.0, &dup).is_err());

    drop(island);
}
