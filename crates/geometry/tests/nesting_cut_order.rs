//! Provenance: sheet hold-down and inside-out cutting rules from issue #2159 and CamCraft plasma nesting.
//! Gate map: covers camcraft_geometry::nesting::cut_order.

use camcraft_geometry::geo2d::{Loop, Point2};
use camcraft_geometry::nesting::cut_order::{
    CutFeature, CutGeometry, CutPhase, CutRole, CutSide, PlannedCut, plan_sheet, validate_cut_order,
};

fn poly(points: &[(f64, f64)]) -> Loop {
    Loop::new(points.iter().map(|&(x, y)| Point2::new(x, y)).collect()).expect("fixture loop")
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Loop {
    poly(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)])
}

fn closed(id: u64, ring: Loop) -> CutFeature {
    CutFeature {
        id,
        geometry: CutGeometry::Closed(ring),
        side: CutSide::Auto,
        passes: vec![CutPhase::Breakthrough],
        release_phase: Some(CutPhase::Breakthrough),
    }
}

fn open(id: u64, points: &[(f64, f64)]) -> CutFeature {
    CutFeature {
        id,
        geometry: CutGeometry::Open(points.iter().map(|&(x, y)| Point2::new(x, y)).collect()),
        side: CutSide::Auto,
        passes: vec![CutPhase::Internal],
        release_phase: None,
    }
}

fn sheet() -> Vec<CutFeature> {
    vec![
        closed(1, rect(0.0, 0.0, 20.0, 20.0)),
        closed(2, rect(2.0, 2.0, 16.0, 16.0)),
        closed(3, rect(4.0, 4.0, 2.0, 2.0)),
        open(4, &[(4.3, 5.0), (5.7, 5.0)]),
        open(5, &[(19.0, 10.0), (19.5, 10.0)]),
        open(6, &[(40.0, 1.0), (41.0, 1.0)]),
        closed(8, rect(30.0, 0.0, 3.0, 3.0)),
        closed(7, rect(25.0, 0.0, 3.0, 3.0)),
    ]
}

#[test]
fn sheet_tree_cuts_parts_in_holes_before_release_and_small_siblings_first() {
    let features = sheet();
    let plan = plan_sheet(&features, 0.2).expect("safe sheet");
    let resolved = |id| plan.features.iter().find(|f| f.id == id).expect("resolved id");
    assert_eq!((resolved(1).parent, resolved(1).depth), (None, 0));
    assert_eq!((resolved(2).parent, resolved(2).depth), (Some(1), 1));
    assert_eq!((resolved(3).parent, resolved(3).depth), (Some(2), 2));
    assert_eq!(resolved(4).parent, Some(3));
    assert_eq!(resolved(5).parent, Some(1));
    assert_eq!(resolved(6).parent, None);
    assert_eq!((resolved(1).role, resolved(1).side), (CutRole::PartOutline, CutSide::Outside));
    assert_eq!((resolved(2).role, resolved(2).side), (CutRole::Hole, CutSide::Inside));
    assert_eq!((resolved(3).role, resolved(3).side), (CutRole::PartOutline, CutSide::Outside));
    assert_eq!((resolved(4).role, resolved(4).side), (CutRole::OpenFeature, CutSide::OnLine));
    let profiles: Vec<_> =
        plan.cuts.iter().filter(|c| c.phase == CutPhase::Breakthrough).map(|c| c.feature_id).collect();
    assert_eq!(profiles, vec![7, 8, 3, 2, 1]);
    validate_cut_order(&features, &plan.cuts).expect("generated order is safe");
    let mut reversed = features.clone();
    reversed.reverse();
    assert_eq!(plan.cuts, plan_sheet(&reversed, 0.2).expect("permuted sheet").cuts);
}

#[test]
fn sheet_pass_phases_keep_marking_first_and_breakthrough_last() {
    let mut features = sheet();
    features[0].passes = vec![CutPhase::Breakthrough, CutPhase::Marking, CutPhase::RoughProfile];
    features[2].passes = vec![CutPhase::RoughProfile, CutPhase::Breakthrough, CutPhase::Marking];
    let plan = plan_sheet(&features, 0.2).expect("ordered phases");
    assert!(plan.cuts.windows(2).all(|w| w[0].phase <= w[1].phase));
    assert_eq!(plan.cuts.first().expect("marking").phase, CutPhase::Marking);
    assert_eq!(plan.cuts.last().expect("release").phase, CutPhase::Breakthrough);
    let rough_ids: Vec<_> =
        plan.cuts.iter().filter(|c| c.phase == CutPhase::RoughProfile).map(|c| c.feature_id).collect();
    assert_eq!(rough_ids, vec![3, 1]);
}

#[test]
fn sheet_refuses_a_phase_that_frees_a_hole_before_its_nested_part() {
    let mut features = sheet();
    features[1].passes = vec![CutPhase::Internal];
    features[1].release_phase = Some(CutPhase::Internal);
    let error = plan_sheet(&features, 0.2).expect_err("hole releases too early").to_string();
    assert!(error.contains('2') && error.contains('3'), "{error}");
}

#[test]
fn sheet_order_validator_refuses_released_ancestors_missing_and_extra_passes() {
    let features = sheet();
    let plan = plan_sheet(&features, 0.2).expect("safe sheet");
    let mut swapped = plan.cuts.clone();
    let hole = swapped.iter().position(|c| c.feature_id == 2).expect("hole");
    let island = swapped.iter().position(|c| c.feature_id == 3).expect("island");
    swapped.swap(hole, island);
    let error = validate_cut_order(&features, &swapped).expect_err("part after hole").to_string();
    assert!(error.contains('2') && error.contains('3'), "{error}");
    assert!(validate_cut_order(&features, &plan.cuts[1..]).is_err());
    let mut duplicate = plan.cuts.clone();
    duplicate.push(plan.cuts[0].clone());
    assert!(validate_cut_order(&features, &duplicate).is_err());
    duplicate.pop();
    duplicate.push(PlannedCut { feature_id: 999, phase: CutPhase::Breakthrough });
    assert!(validate_cut_order(&features, &duplicate).is_err());
    let mut wrong_phase = plan.cuts.clone();
    wrong_phase[0].phase = CutPhase::Marking;
    assert!(validate_cut_order(&features, &wrong_phase).is_err());
}

#[test]
fn sheet_order_validator_checks_global_phases_but_accepts_safe_sibling_orders() {
    let mut first = closed(1, rect(0.0, 0.0, 2.0, 2.0));
    first.passes = vec![CutPhase::Marking, CutPhase::Breakthrough];
    let mut second = closed(2, rect(4.0, 0.0, 2.0, 2.0));
    second.passes = vec![CutPhase::Marking, CutPhase::Breakthrough];
    let features = vec![first, second];
    let safe = vec![
        PlannedCut { feature_id: 2, phase: CutPhase::Marking },
        PlannedCut { feature_id: 1, phase: CutPhase::Marking },
        PlannedCut { feature_id: 2, phase: CutPhase::Breakthrough },
        PlannedCut { feature_id: 1, phase: CutPhase::Breakthrough },
    ];
    validate_cut_order(&features, &safe).expect("safe alternative sibling order");
    let mut wrong = safe;
    wrong.swap(1, 2);
    assert!(validate_cut_order(&features, &wrong).is_err());
}

#[test]
fn sheet_order_validator_names_unrequested_and_duplicate_open_passes() {
    let feature = open(70, &[(0.0, 0.0), (2.0, 0.0)]);
    let unrequested = vec![
        PlannedCut { feature_id: 70, phase: CutPhase::Marking },
        PlannedCut { feature_id: 70, phase: CutPhase::Internal },
    ];
    let error = validate_cut_order(std::slice::from_ref(&feature), &unrequested)
        .expect_err("all requested passes are present, but an extra pass is not permitted").to_string();
    assert!(error.contains("unrequested") && error.contains("70"), "{error}");
    let duplicate = vec![
        PlannedCut { feature_id: 70, phase: CutPhase::Internal },
        PlannedCut { feature_id: 70, phase: CutPhase::Internal },
    ];
    let error = validate_cut_order(&[feature], &duplicate).expect_err("duplicate open pass").to_string();
    assert!(error.contains("duplicate") && error.contains("70"), "{error}");
}

#[test]
fn sheet_refuses_a_later_pass_after_the_same_contour_releases() {
    let mut feature = closed(19, rect(0.0, 0.0, 2.0, 2.0));
    feature.passes = vec![CutPhase::Internal, CutPhase::Breakthrough];
    feature.release_phase = Some(CutPhase::Internal);
    let error = plan_sheet(&[feature], 0.2).expect_err("contour already released").to_string();
    assert!(error.contains("19"), "{error}");
}

#[test]
fn sheet_blind_pocket_pass_keeps_material_until_final_breakthrough() {
    let mut feature = closed(19, rect(0.0, 0.0, 2.0, 2.0));
    feature.passes = vec![CutPhase::Internal, CutPhase::RoughProfile, CutPhase::Breakthrough];
    feature.release_phase = Some(CutPhase::Breakthrough);
    let plan = plan_sheet(&[feature], 0.2).expect("blind pocket retains material");
    assert_eq!(
        plan.cuts.iter().map(|c| c.phase).collect::<Vec<_>>(),
        vec![CutPhase::Internal, CutPhase::RoughProfile, CutPhase::Breakthrough]
    );
    let mut marking = closed(20, rect(0.0, 0.0, 2.0, 2.0));
    marking.passes = vec![CutPhase::Marking];
    marking.release_phase = None;
    assert_eq!(plan_sheet(&[marking], 0.2).expect("marking does not release").cuts.len(), 1);
}

#[test]
fn sheet_refuses_invalid_release_phase_requests() {
    let feature = closed(19, rect(0.0, 0.0, 2.0, 2.0));
    let mut unrequested = feature.clone();
    unrequested.release_phase = Some(CutPhase::Internal);
    assert!(plan_sheet(&[unrequested], 0.2).is_err());
    for phase in [CutPhase::Marking, CutPhase::RoughProfile] {
        let mut invalid = feature.clone();
        invalid.passes = vec![phase];
        invalid.release_phase = Some(phase);
        assert!(plan_sheet(&[invalid], 0.2).is_err());
    }
    let mut chain = open(21, &[(0.0, 0.0), (2.0, 0.0)]);
    chain.release_phase = Some(CutPhase::Internal);
    assert!(plan_sheet(&[chain], 0.2).is_err());
    let mut no_release = feature;
    no_release.release_phase = None;
    assert!(plan_sheet(&[no_release], 0.2).is_err());
}

#[test]
fn sheet_open_chain_sides_resolve_and_invalid_sides_name_the_chain() {
    for side in [CutSide::Auto, CutSide::OnLine, CutSide::Left, CutSide::Right] {
        let mut chain = open(17, &[(0.0, 0.0), (1.0, 0.0)]);
        chain.side = side;
        let plan = plan_sheet(&[chain], 0.2).expect("legal open side");
        assert_eq!(plan.features[0].side, if side == CutSide::Auto { CutSide::OnLine } else { side });
    }
    for side in [CutSide::Inside, CutSide::Outside] {
        let mut chain = open(17, &[(0.0, 0.0), (1.0, 0.0)]);
        chain.side = side;
        let error = plan_sheet(&[chain], 0.2).expect_err("invalid open side").to_string();
        assert!(error.contains("17") && error.contains(&format!("{side:?}")), "{error}");
    }
    for side in [CutSide::Inside, CutSide::Outside, CutSide::OnLine] {
        let mut contour = closed(1, rect(0.0, 0.0, 2.0, 2.0));
        contour.side = side;
        assert_eq!(plan_sheet(&[contour], 0.2).expect("legal closed side").features[0].side, side);
    }
    for side in [CutSide::Left, CutSide::Right] {
        let mut contour = closed(1, rect(0.0, 0.0, 2.0, 2.0));
        contour.side = side;
        assert!(plan_sheet(&[contour], 0.2).is_err());
    }
}

#[test]
fn sheet_refuses_invalid_width_ids_and_pass_requests() {
    let feature = closed(1, rect(0.0, 0.0, 2.0, 2.0));
    for width in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let error = plan_sheet(std::slice::from_ref(&feature), width).expect_err("invalid width").to_string();
        assert!(error.contains("cut_width"), "{error}");
    }
    let same_id = closed(1, rect(4.0, 0.0, 2.0, 2.0));
    let error = plan_sheet(&[feature.clone(), same_id], 0.2).expect_err("duplicate id").to_string();
    assert!(error.contains("duplicate sheet feature id 1"), "{error}");
    let mut invalid = feature.clone();
    invalid.passes.clear();
    assert!(plan_sheet(&[invalid], 0.2).is_err());
    let mut duplicate = feature;
    duplicate.passes.push(CutPhase::Breakthrough);
    assert!(plan_sheet(&[duplicate], 0.2).is_err());
    assert!(plan_sheet(&[], 0.2).expect("empty sheet").cuts.is_empty());
}

#[test]
fn sheet_refuses_degenerate_self_crossing_and_retraced_geometry() {
    let bad_loops = [
        Loop::default(),
        poly(&[(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)]),
        poly(&[(0.0, 0.0), (2.0, 2.0), (0.0, 2.0), (2.0, 0.0)]),
        poly(&[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (1.0, 2.0), (1.0, 3.0), (1.0, 2.0), (0.0, 2.0)]),
    ];
    for ring in bad_loops {
        let error = plan_sheet(&[closed(27, ring)], 0.2).expect_err("invalid contour").to_string();
        assert!(error.contains("27"), "{error}");
    }
    for points in [
        vec![],
        vec![(0.0, 0.0)],
        vec![(0.0, 0.0), (0.0, 0.0)],
        vec![(0.0, 0.0), (1e-8, 0.0)],
        vec![(0.0, 0.0), (2.0, 2.0), (0.0, 2.0), (2.0, 0.0)],
        vec![(0.0, 0.0), (2.0, 0.0), (1.0, 0.0)],
    ] {
        let error = plan_sheet(&[open(28, &points)], 0.2).expect_err("invalid chain").to_string();
        assert!(error.contains("28"), "{error}");
    }
    for (points, reason) in [
        (vec![(0.0, 0.0), (f64::NAN, 1.0)], "non-finite"),
        (vec![(0.0, 0.0), (1e13, 1.0)], "range"),
        (vec![(0.0, 0.0), (1e-8, 0.0)], "zero-length"),
        (vec![(0.0, 0.0), (2.0, 0.0), (0.0, 2.0), (0.0, 0.0)], "marked open"),
        (vec![(0.0, 0.0), (2.0, 0.0), (1.0, 0.0)], "retraces"),
    ] {
        let error = plan_sheet(&[open(28, &points)], 0.2).expect_err("invalid coordinates").to_string();
        assert!(error.contains("28") && error.contains(reason), "{error}");
    }
}

#[test]
fn sheet_refuses_area_accumulation_overflow_by_name() {
    let axes = [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)];
    for mirror in [-1.0, 1.0] {
        let point = |i: usize, inset: f64| {
            let radius = 9e11 - i as f64 * 1e8 - inset;
            let (x, y) = axes[i % 4];
            Point2::new(mirror * x * radius, y * radius)
        };
        // A thin simple spiral: fan area overflows before the return path cancels it.
        let mut points: Vec<_> = (0..=320).map(|i| point(i, 0.0)).collect();
        points.extend((0..=320).rev().map(|i| point(i, 1e6)));
        let ring = Loop::new(points).expect("simple spiral within coordinate range");
        let error = plan_sheet(&[closed(88, ring)], 0.2).expect_err("area sum exceeds i128").to_string();
        assert!(error.contains("88") && error.contains("area") && error.contains("range"), "{error}");
    }
}

#[test]
fn sheet_refuses_crossing_or_touching_contours_and_boundary_crossing_chains() {
    let outer = closed(1, rect(0.0, 0.0, 10.0, 10.0));
    for ring in [rect(9.0, 2.0, 2.0, 2.0), rect(10.0, 2.0, 2.0, 2.0), rect(0.0, 0.0, 10.0, 10.0)] {
        let error =
            plan_sheet(&[outer.clone(), closed(2, ring)], 0.2).expect_err("ambiguous contour relation").to_string();
        assert!(error.contains('1') && error.contains('2'), "{error}");
    }
    for chain in [open(3, &[(5.0, 5.0), (15.0, 5.0)]), open(3, &[(0.0, 5.0), (5.0, 5.0)])] {
        assert!(plan_sheet(&[outer.clone(), chain], 0.2).is_err());
    }
    let concave = closed(
        10,
        poly(&[(0.0, 0.0), (6.0, 0.0), (6.0, 6.0), (4.0, 6.0), (4.0, 2.0), (2.0, 2.0), (2.0, 6.0), (0.0, 6.0)]),
    );
    // Both endpoints are inside, but the segment crosses the notch twice.
    let chain = open(11, &[(1.0, 5.0), (5.0, 5.0)]);
    assert!(plan_sheet(&[concave, chain], 0.2).is_err());
}

#[test]
fn sheet_crossing_open_features_keep_the_same_containing_parent() {
    let features = vec![
        closed(1, rect(0.0, 0.0, 10.0, 10.0)),
        open(2, &[(2.0, 5.0), (8.0, 5.0)]),
        open(3, &[(5.0, 2.0), (5.0, 8.0)]),
    ];
    let plan = plan_sheet(&features, 0.2).expect("independent open cuts may cross");
    for id in [2, 3] {
        let feature = plan.features.iter().find(|f| f.id == id).expect("open feature");
        assert_eq!((feature.parent, feature.role), (Some(1), CutRole::OpenFeature));
    }
    validate_cut_order(&features, &plan.cuts).expect("open features precede outline release");
}

#[test]
fn sheet_corner_limits_use_tool_radius_and_ignore_input_winding() {
    let points = [(0.0, 0.0), (6.0, 0.0), (6.0, 2.0), (2.0, 2.0), (2.0, 6.0), (0.0, 6.0)];
    for vertices in [points.to_vec(), points.into_iter().rev().collect::<Vec<_>>()] {
        let plan = plan_sheet(&[closed(9, poly(&vertices))], 2.0).expect("concave part");
        assert_eq!(plan.corner_limits.len(), 1);
        let limit = &plan.corner_limits[0];
        assert_eq!(limit.feature_id, 9);
        assert_eq!(vertices[limit.vertex], (2.0, 2.0));
        assert!((limit.tool_radius - 1.0).abs() < 1e-10);
        assert!((limit.angle_radians - std::f64::consts::FRAC_PI_2).abs() < 1e-10);
        assert!((limit.min_unreachable_distance - (2.0_f64.sqrt() - 1.0)).abs() < 1e-10);
    }
    let mut inside = closed(12, rect(0.0, 0.0, 10.0, 10.0));
    inside.side = CutSide::Inside;
    assert_eq!(plan_sheet(&[inside.clone()], 2.0).expect("inside corners").corner_limits.len(), 4);
    inside.side = CutSide::OnLine;
    assert!(plan_sheet(&[inside], 2.0).expect("on-line profile").corner_limits.is_empty());
}

#[test]
fn sheet_open_side_corner_limits_follow_the_directed_cut_side() {
    let points = [(0.0, 0.0), (4.0, 0.0), (4.0, 4.0)];
    let cases: [(Vec<(f64, f64)>, CutSide); 2] =
        [(points.to_vec(), CutSide::Left), (points.into_iter().rev().collect::<Vec<_>>(), CutSide::Right)];
    for (vertices, side) in cases {
        let mut chain = open(22, &vertices);
        chain.side = side;
        let plan = plan_sheet(&[chain], 2.0).expect("side-cut chain");
        assert_eq!(plan.corner_limits.len(), 1);
        let limit = &plan.corner_limits[0];
        assert_eq!((limit.feature_id, limit.vertex), (22, 1));
        assert!((limit.tool_radius - 1.0).abs() < 1e-10);
        assert!((limit.angle_radians - std::f64::consts::FRAC_PI_2).abs() < 1e-10);
        assert!((limit.min_unreachable_distance - (2.0_f64.sqrt() - 1.0)).abs() < 1e-10);
    }
    for side in [CutSide::Right, CutSide::OnLine, CutSide::Auto] {
        let mut chain = open(22, &points);
        chain.side = side;
        assert!(plan_sheet(&[chain], 2.0).expect("no inside corner").corner_limits.is_empty());
    }
}
