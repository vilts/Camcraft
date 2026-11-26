//! Integration tests for `pocket_avoid` and `pocket3d_avoid` (#2055).
//! Provenance: independent `camcraft-motion` tests checking `camcraft-sim::verify::avoid`.

use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::links::{Link, Links};
use camcraft_motion::load::DEFAULT_OPTIMAL_LOAD;
use camcraft_motion::machining2d::{
    CornerRelief, PreHole, RestParams, RestSource, pocket, pocket_avoid,
    pocket_avoid_entered_with_relief, rest_pocket_avoid_with_relief, PocketParams, PocketStrategy,
};
use camcraft_motion::machining3d::{
    pocket3d, pocket3d_avoid, pocket3d_offset, pocket3d_with_rest, Pocket3DParams,
};
use camcraft_motion::tools::ToolGeom;
use camcraft_motion::verify::{avoid_hits, AVOID_VERIFY_TOL, AvoidItem, AvoidSpec, Cutter};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
    .unwrap()])
    .unwrap()
}

fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let p = |x, y, z| Point3::new(x, y, z);
    let v = |i: usize| p([min[0], max[0]][i & 1], [min[1], max[1]][(i >> 1) & 1], [min[2], max[2]][(i >> 2) & 1]);
    let quad = |a: usize, b: usize, c: usize, d: usize| -> [Triangle; 2] {
        [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))]
    };
    let mut t = Vec::with_capacity(12);
    t.extend(quad(4, 5, 7, 6));
    t.extend(quad(0, 2, 3, 1));
    t.extend(quad(0, 1, 5, 4));
    t.extend(quad(2, 6, 7, 3));
    t.extend(quad(1, 3, 7, 5));
    t.extend(quad(2, 0, 4, 6));
    t
}

fn stepped_cutter() -> Cutter {
    Cutter::flat(&ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 8.0,
        shank_radius: 4.5,
        shank_len: 10.0,
        holder_radius: 12.0,
        holder_len: 20.0,
    })
}

fn base_2d(strategy: PocketStrategy, entry: Entry3D, links: Links) -> PocketParams {
    PocketParams {
        tool_diameter: 6.0,
        stepover: 2.5,
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        top_z: 0.0,
        bottom_z: -5.0,
        stepdown: 2.5,
        feed: 600.0,
        plunge_feed: 150.0,
        climb: true,
        strategy,
        retract_z: Some(2.0),
        entry,
        helix_radius: 1.5,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        blend_radius: None,
        links,
    }
}

fn base_3d(strategy: PocketStrategy, entry: Entry3D, links: Links) -> Pocket3DParams {
    Pocket3DParams {
        pass_start: Default::default(),
        links,
        tool_diameter: 6.0,
        stepover: 2.5,
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        top_z: 0.0,
        bottom_z: -8.0,
        stepdown: 4.0,
        feed: 700.0,
        plunge_feed: 200.0,
        climb: true,
        strategy,
        retract_z: Some(2.0),
        entry,
        helix_radius: 1.5,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
    }
}

#[test]
fn empty_avoid_matches_plain_paths_exactly() {
    let reg = rect(-25.0, -20.0, 25.0, 20.0);
    let empty = AvoidSpec { items: vec![], cutter: stepped_cutter(), clearance: 0.0 };

    let p2d = base_2d(PocketStrategy::Waterline, Entry3D::Helix, Links::ROUGH3D);
    assert_eq!(pocket_avoid(&reg, &p2d, &empty).unwrap(), pocket(&reg, &p2d).unwrap());

    let facets = box_facets([-25.0, -20.0, -15.0], [25.0, 20.0, -8.0]);
    let p3d = base_3d(PocketStrategy::OffsetRings, Entry3D::Helix, Links::ROUGH3D);
    assert_eq!(pocket3d_avoid(&facets, &reg, &p3d, false, false, None, &empty).unwrap(), pocket3d(&facets, &reg, &p3d).unwrap());
    assert_eq!(pocket3d_avoid(&facets, &reg, &p3d, false, true, None, &empty).unwrap(), pocket3d_offset(&facets, &reg, &p3d).unwrap());
    assert_eq!(pocket3d_avoid(&facets, &reg, &p3d, true, false, None, &empty).unwrap(), pocket3d_with_rest(&facets, &reg, &p3d).unwrap().0);
}

#[test]
fn pocket_avoid_all_strategies_entries_holder_and_edge() {
    let reg = rect(-35.0, -25.0, 35.0, 25.0);
    // Tall pin on the center + elevated holder-height ledge (14..17 mm) on the east side + bare 3D edge on the west.
    let mut tris = box_facets([-3.0, -3.0, -10.0], [3.0, 3.0, 2.0]);
    tris.extend(box_facets([28.0, -10.0, 14.0], [34.0, 10.0, 17.0]));
    let spec = AvoidSpec {
        items: vec![AvoidItem {
            label: "pin & boss edge".into(),
            tris,
            edges: vec![([-22.0, -6.0, -2.5], [-22.0, 6.0, -2.5])],
        }],
        cutter: stepped_cutter(),
        clearance: 1.0,
    };

    let keep_down = Links {
        pass: Link::KeepDown,
        level: Link::KeepDown,
        ..Links::ROUGH3D
    };

    for (strategy, entry, links) in [
        (PocketStrategy::Waterline, Entry3D::Plunge, Links::PLANAR),
        (PocketStrategy::Zigzag, Entry3D::Ramp, Links::ROUGH3D),
        (PocketStrategy::Auto, Entry3D::Helix, Links::ROUGH3D),
        (PocketStrategy::OffsetRings, Entry3D::Helix, keep_down),
        (PocketStrategy::Adaptive, Entry3D::Helix, Links::ROUGH3D),
    ] {
        let params = base_2d(strategy, entry, links);
        let path = pocket_avoid(&reg, &params, &spec)
            .unwrap_or_else(|e| panic!("strategy {strategy:?} entry {entry:?} failed: {e:?}"));
        let hits = avoid_hits(&path, &spec, AVOID_VERIFY_TOL).expect("verify");
        let details: Vec<_> = hits
            .iter()
            .filter_map(|w| match w {
                camcraft_motion::verify::Warning::AvoidHit { at, part, short_mm, .. } => {
                    path.moves.get(*at).map(|m| format!("#{at} {part:?} short={short_mm:.4} {:?} {:?}->{:?}", m.kind, m.from, m.to))
                }
                _ => None,
            })
            .collect();
        assert!(hits.is_empty(), "strategy {strategy:?} entry {entry:?} violated Do not touch: {details:#?}");
        // Verify holder-height ledge [28..34, -10..10, 14..17] keeps holder_r (12.0) + clearance (1.0) = 13.0 mm standoff at cut depth.
        for m in path.moves.iter().filter(|m| m.to.z < 0.0) {
            let dx = if m.to.x < 28.0 { 28.0 - m.to.x } else if m.to.x > 34.0 { m.to.x - 34.0 } else { 0.0 };
            let dy = (m.to.y.abs() - 10.0).max(0.0);
            let dist = dx.hypot(dy);
            assert!(dist >= 13.0 - 1e-2, "{strategy:?}: move {:?} came {dist:.3} mm from holder ledge (< 13.0 mm)", m.to);
        }
    }

    // Covering the entire pocket refuses cleanly naming the protected item.
    let blocker = AvoidSpec {
        items: vec![AvoidItem {
            label: "full shield".into(),
            tris: box_facets([-40.0, -30.0, -10.0], [40.0, 30.0, 5.0]),
            edges: vec![],
        }],
        cutter: stepped_cutter(),
        clearance: 0.5,
    };
    let err = pocket_avoid(&reg, &base_2d(PocketStrategy::Waterline, Entry3D::Plunge, Links::PLANAR), &blocker)
        .expect_err("fully covered pocket must be refused");
    assert!(format!("{err:?}").contains("full shield"), "expected item name in error: {err:?}");
}

#[test]
fn pocket3d_avoid_shell_rest_and_adaptive_stay_clear() {
    let reg = rect(-35.0, -25.0, 35.0, 25.0);
    let mut facets = box_facets([-35.0, -25.0, -16.0], [35.0, 25.0, -8.0]);
    // Unprotected gently sloped step on the west side so shell lifting activates.
    facets.extend(box_facets([-28.0, -15.0, -8.0], [-14.0, 15.0, -5.0]));

    let spec = AvoidSpec {
        items: vec![AvoidItem {
            label: "protected pad".into(),
            tris: box_facets([-4.0, -4.0, -8.0], [4.0, 4.0, 1.0]),
            edges: vec![([15.0, -8.0, -4.0], [15.0, 8.0, -4.0])],
        }],
        cutter: stepped_cutter(),
        clearance: 0.8,
    };

    for (strategy, entry, rest, shell) in [
        (PocketStrategy::Waterline, Entry3D::Plunge, false, false),
        (PocketStrategy::OffsetRings, Entry3D::Ramp, false, true),
        (PocketStrategy::OffsetRings, Entry3D::Helix, true, false),
        (PocketStrategy::Adaptive, Entry3D::Helix, false, false),
    ] {
        let p = base_3d(strategy, entry, Links::ROUGH3D);
        let path = pocket3d_avoid(&facets, &reg, &p, rest, shell, None, &spec)
            .unwrap_or_else(|e| panic!("3D {strategy:?} rest={rest} shell={shell} failed: {e:?}"));
        let hits = avoid_hits(&path, &spec, AVOID_VERIFY_TOL).expect("verify");
        assert!(hits.is_empty(), "3D {strategy:?} rest={rest} shell={shell} hit Do not touch: {hits:?}");
    }

    // Merged #2004 cut_levels_under: without stock above top_z, top_z (0.0) is skipped;
    // with stock_top above top_z, top_z is included as a cut plane on both plain and rest paths.
    for rest in [false, true] {
        let mut p_no_air = base_3d(PocketStrategy::OffsetRings, Entry3D::Plunge, Links::ROUGH3D);
        p_no_air.links.approach.stock_top = None;
        let path_no_air = pocket3d_avoid(&facets, &reg, &p_no_air, rest, false, None, &spec).expect("no-air");
        assert!(
            path_no_air.moves.iter().all(|m| m.kind != camcraft_motion::cl::MoveKind::Feed || m.to.z < -1e-6),
            "rest={rest}: top_z air plane must be skipped when stock_top is None"
        );
        let mut p_over = p_no_air;
        p_over.links.approach.stock_top = Some(2.0);
        let path_over = pocket3d_avoid(&facets, &reg, &p_over, rest, false, None, &spec).expect("stock-over");
        assert!(
            path_over.moves.iter().any(|m| m.kind == camcraft_motion::cl::MoveKind::Feed && m.to.z.abs() < 1e-6),
            "rest={rest}: top_z must be cut when stock stands above top_z"
        );
        assert!(avoid_hits(&path_over, &spec, AVOID_VERIFY_TOL).expect("verify").is_empty());
    }
}

#[test]
fn pocket_avoid_keeps_final_clearance_lift_clear_of_overhead_obstacle() {
    let reg = rect(-35.0, -25.0, 35.0, 25.0);
    // Overhead protected pads at all 4 outer corners above retract_z + total_tool_height (2 + 38 = 40),
    // but inside clearance_z + total_tool_height (10 + 38 = 48), catching whichever outer corner
    // Waterline finishes and lifts to clearance_z at.
    let mut corner_tris = Vec::new();
    for (x0, x1) in [(-35.0, -28.0), (28.0, 35.0)] {
        for (y0, y1) in [(-25.0, -18.0), (18.0, 25.0)] {
            corner_tris.extend(box_facets([x0, y0, 44.0], [x1, y1, 48.0]));
        }
    }
    let spec = AvoidSpec {
        items: vec![AvoidItem {
            label: "overhead bridge".into(),
            tris: corner_tris,
            edges: vec![],
        }],
        cutter: stepped_cutter(),
        clearance: 1.0,
    };
    let mut p = base_2d(
        PocketStrategy::Waterline,
        Entry3D::Plunge,
        Links { level: Link::Retract, pass: Link::Retract, ..Links::ROUGH3D },
    );
    p.top_z = -40.0;
    p.bottom_z = -45.0;
    p.stepdown = 2.5;
    p.retract_z = Some(2.0);
    p.clearance_z = 10.0;

    let path = pocket_avoid(&reg, &p, &spec).expect("pocket_avoid");
    let hits = avoid_hits(&path, &spec, AVOID_VERIFY_TOL).expect("verify");
    assert!(hits.is_empty(), "final clearance lift must not hit overhead bridge: {hits:?}");
}

#[test]
fn pocket_avoid_predrilled_and_rest_stay_clear() {
    let reg = rect(-30.0, -20.0, 30.0, 20.0);
    let spec = AvoidSpec {
        items: vec![AvoidItem {
            label: "protected pin".into(),
            tris: box_facets([-3.0, -3.0, -10.0], [3.0, 3.0, 2.0]),
            edges: vec![],
        }],
        cutter: stepped_cutter(),
        clearance: 1.0,
    };
    // Pre-drilled hole inside the protected pin must be ignored; safe hole at (-18, 0) stays usable.
    let mut p = base_2d(PocketStrategy::Waterline, Entry3D::Helix, Links::PLANAR);
    p.links.approach.predrilled = true;
    p.links.approach.center_cutting = true;
    let holes = [
        PreHole { at: Point2::new(0.0, 0.0), floor: -6.0 },
        PreHole { at: Point2::new(-18.0, 0.0), floor: -6.0 },
    ];
    let path = pocket_avoid_entered_with_relief(&reg, &p, &holes, &spec, &CornerRelief::default())
        .expect("predrilled pocket_avoid");
    let hits = avoid_hits(&path, &spec, AVOID_VERIFY_TOL).expect("verify predrilled");
    assert!(hits.is_empty(), "predrilled pocket_avoid hit protected pin: {hits:?}");

    // Rest machining with Do not touch: previous 20 mm cutter left corner material;
    // protect the north-east corner so that corner is skipped while other corners cut clear.
    let rest_spec = AvoidSpec {
        items: vec![AvoidItem {
            label: "ne corner guard".into(),
            tris: box_facets([22.0, 12.0, -10.0], [31.0, 21.0, 2.0]),
            edges: vec![],
        }],
        cutter: stepped_cutter(),
        clearance: 1.0,
    };
    let rp = RestParams {
        source: RestSource::EnteredTool,
        previous_tool_diameter: 20.0,
        previous_corner_radius: 0.0,
        previous_stock_to_leave: 0.0,
        allowance: 0.5,
    };
    let p_rest = base_2d(PocketStrategy::Waterline, Entry3D::Helix, Links::ROUGH3D);
    let rest_path = rest_pocket_avoid_with_relief(&reg, &p_rest, &rp, &rest_spec, &CornerRelief::default())
        .expect("rest_pocket_avoid");
    let rest_hits = avoid_hits(&rest_path, &rest_spec, AVOID_VERIFY_TOL).expect("verify rest");
    assert!(rest_hits.is_empty(), "rest pocket_avoid hit protected corner: {rest_hits:?}");

    // When all rest corners are protected, refusal names the protected item.
    let all_guard = AvoidSpec {
        items: vec![AvoidItem {
            label: "all corners".into(),
            tris: box_facets([-32.0, -22.0, -10.0], [32.0, 22.0, 2.0]),
            edges: vec![],
        }],
        cutter: stepped_cutter(),
        clearance: 0.5,
    };
    let err = rest_pocket_avoid_with_relief(&reg, &p_rest, &rp, &all_guard, &CornerRelief::default())
        .expect_err("fully protected rest pocket must refuse");
    assert!(format!("{err:?}").contains("all corners"), "expected protected label: {err:?}");
}

#[test]
fn pocket_avoid_offset_rings_keep_down_never_crosses() {
    let reg = rect(-25.0, -25.0, 25.0, 25.0);
    // Overhead ledge sits clear of flutes at Z 17, but meets the wider shank/holder at Z 10 so
    // inner rings drop out between levels and Level 1's helix enters near a Level 0 helix ring.
    let spec = AvoidSpec {
        items: vec![
            AvoidItem {
                label: "center pin".into(),
                tris: box_facets([-17.0, -17.0, 0.0], [17.0, 17.0, 22.0]),
                edges: vec![],
            },
            AvoidItem {
                label: "side guard".into(),
                tris: vec![],
                edges: vec![([18.0, -10.0, 12.0], [18.0, 10.0, 12.0])],
            },
        ],
        cutter: stepped_cutter(),
        clearance: 1.0,
    };
    let mut p = base_2d(
        PocketStrategy::OffsetRings,
        Entry3D::Helix,
        Links { level: Link::KeepDown, pass: Link::KeepDown, ..Links::ROUGH3D },
    );
    p.tool_diameter = 4.0;
    p.stepover = 1.0;
    p.optimal_load = 0.22;
    p.helix_radius = 1.5;
    p.top_z = 24.0;
    p.bottom_z = 10.0;
    p.stepdown = 7.0;
    p.clearance_z = 30.0;
    p.retract_z = Some(26.0);

    let path = pocket_avoid(&reg, &p, &spec).expect("pocket_avoid offset_rings+keep_down");
    let crossed = camcraft_motion::verify::crossed_pairs(&path, 0.01);
    assert!(crossed.is_empty(), "OffsetRings + KeepDown crossed: {crossed:?}");
    let hits = avoid_hits(&path, &spec, AVOID_VERIFY_TOL).expect("verify");
    assert!(hits.is_empty(), "OffsetRings + KeepDown hit protected: {hits:?}");
}
