//! Nesting kernel tests: 2D NFP/IFP and deterministic bottom-left-fill
//! placement on the micron grid (issue 2158).
//!
//! Gate map (M4.4): covers camcraft_geometry::nesting.

use camcraft_geometry::geo2d::{Loop, Point2, Region};
use camcraft_geometry::nesting::{
    ClearanceMode, CutSide, NestError, PartInput, RotationMode, SheetInput, nest,
};

fn rect_loop(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
    .expect("valid rectangle loop")
}

fn l_shape_loop(w: f64, h: f64, arm: f64) -> Loop {
    Loop::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(w, 0.0),
        Point2::new(w, arm),
        Point2::new(arm, arm),
        Point2::new(arm, h),
        Point2::new(0.0, h),
    ])
    .expect("valid L-shape loop")
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn assert_no_inflated_overlap(placements: &[camcraft_geometry::nesting::PlacedPart]) {
    for i in 0..placements.len() {
        for j in (i + 1)..placements.len() {
            let inter = placements[i]
                .inflated_footprint
                .intersect(&placements[j].inflated_footprint)
                .expect("intersect inflated footprints");
            assert!(
                inter.is_empty(),
                "inflated footprints overlap between {}#{} and {}#{}: area={}",
                placements[i].part_id,
                placements[i].instance_index,
                placements[j].part_id,
                placements[j].instance_index,
                inter.area()
            );
        }
    }
}

#[test]
fn deterministic_irregular_l_shapes_nest_without_overlap() {
    let sheet = SheetInput {
        width: 120.0,
        height: 120.0,
        edge_margin: 5.0,
        cut_width: 1.0,
        part_gap: 2.0,
        allow_parts_in_holes: true,
        clearance_mode: ClearanceMode::SeparateCuts,
    };
    let l_part = PartInput::closed("l_bracket", 4, l_shape_loop(40.0, 40.0, 15.0), vec![])
        .with_rotation(RotationMode::Orthogonal);
    let spacer = PartInput::closed("spacer", 2, rect_loop(0.0, 0.0, 18.0, 18.0), vec![])
        .with_rotation(RotationMode::Orthogonal);

    let r1 = nest(&sheet, &[l_part.clone(), spacer.clone()]).expect("first nest run");
    let r2 = nest(&sheet, &[l_part, spacer]).expect("second nest run");
    assert_eq!(r1, r2, "nesting must be deterministic");
    assert!(
        r1.unplaced.is_empty(),
        "unexpected unplaced: {:?}",
        r1.unplaced
    );
    assert_eq!(r1.total_requested, 6);
    assert_eq!(r1.total_placed, 6);
    assert_eq!(r1.sheet_count, 1);

    let s = &r1.sheets[0];
    assert_eq!(s.placed_counts.get("l_bracket"), Some(&4));
    assert_eq!(s.placed_counts.get("spacer"), Some(&2));
    assert_no_inflated_overlap(&s.placements);

    let l_area = 40.0 * 15.0 + 25.0 * 15.0; // 975.0
    let spacer_area = 18.0 * 18.0; // 324.0
    let expected_net = 4.0 * l_area + 2.0 * spacer_area;
    let expected_pct = expected_net / (120.0 * 120.0) * 100.0;
    assert!(
        close(s.utilization_pct, expected_pct, 1e-3),
        "got {} want {}",
        s.utilization_pct,
        expected_pct
    );
    assert!(close(r1.overall_utilization_pct, expected_pct, 1e-3));

    // Verify waste_region excludes every cut footprint and sums to sheet area.
    let mut cut_union = Region::empty();
    for p in &s.placements {
        assert!(
            p.cut_footprint
                .difference(&s.usable_sheet)
                .expect("diff usable")
                .area()
                .abs()
                < 1e-3,
            "cut footprint escaped usable sheet for {}#{}",
            p.part_id,
            p.instance_index
        );
        cut_union = cut_union
            .union(&p.cut_footprint)
            .expect("union cut footprints");
        let waste_hit = s
            .waste_region
            .intersect(&p.cut_footprint)
            .expect("waste vs cut");
        assert!(
            waste_hit.area().abs() < 1e-4,
            "waste region overlaps cut footprint: {}",
            waste_hit.area()
        );
    }
    assert!(
        close(
            s.waste_region.area() + cut_union.area(),
            120.0 * 120.0,
            1e-2
        ),
        "waste {} + cuts {} != sheet {}",
        s.waste_region.area(),
        cut_union.area(),
        120.0 * 120.0
    );
    assert!(s.used_bounds.is_some());
}

#[test]
fn places_small_part_inside_large_part_hole_with_full_clearance() {
    let sheet = SheetInput {
        width: 120.0,
        height: 120.0,
        edge_margin: 5.0,
        cut_width: 2.0,
        part_gap: 2.0,
        allow_parts_in_holes: true,
        clearance_mode: ClearanceMode::SeparateCuts,
    };
    let frame = PartInput::closed(
        "frame",
        1,
        rect_loop(0.0, 0.0, 100.0, 100.0),
        vec![rect_loop(20.0, 20.0, 80.0, 80.0)],
    )
    .with_rotation(RotationMode::Locked);
    // Hole is 60x60; hole_usable shrinks by cut_width + part_gap/2 = 3.0 per side -> 54x54.
    // Core is 30x30; inflated by cut_width + part_gap/2 = 3.0 per side -> 36x36 <= 54x54.
    let core = PartInput::closed("core", 1, rect_loop(0.0, 0.0, 30.0, 30.0), vec![])
        .with_rotation(RotationMode::Locked);

    let report = nest(&sheet, &[frame.clone(), core.clone()]).expect("nest with hole");
    assert_eq!(report.sheet_count, 1, "core should fit inside frame hole");
    assert_eq!(report.total_placed, 2);
    assert_no_inflated_overlap(&report.sheets[0].placements);

    let placed_core = report.sheets[0]
        .placements
        .iter()
        .find(|p| p.part_id == "core")
        .expect("core placed");
    assert_eq!(
        placed_core.container_part,
        Some(("frame".to_string(), 0)),
        "core must report frame#0 as container"
    );

    let expected_net = (100.0 * 100.0 - 60.0 * 60.0) + (30.0 * 30.0);
    let expected_pct = expected_net / (120.0 * 120.0) * 100.0;
    assert!(close(report.sheets[0].utilization_pct, expected_pct, 1e-3));

    // When allow_parts_in_holes is false, core cannot fit inside frame and must open sheet 2.
    let mut no_holes = sheet;
    no_holes.allow_parts_in_holes = false;
    let report_no_holes = nest(&no_holes, &[frame, core]).expect("nest without parts in holes");
    assert_eq!(report_no_holes.sheet_count, 2);
}

#[test]
fn swept_open_chains_and_cut_sides_nest_cleanly() {
    let sheet = SheetInput {
        width: 120.0,
        height: 80.0,
        edge_margin: 5.0,
        cut_width: 2.0,
        part_gap: 2.0,
        allow_parts_in_holes: true,
        clearance_mode: ClearanceMode::SeparateCuts,
    };
    let slit_online = PartInput::open(
        "slit_online",
        2,
        vec![vec![Point2::new(0.0, 0.0), Point2::new(30.0, 0.0)]],
    )
    .with_cut_side(CutSide::OnLine);
    let slit_side = PartInput::open(
        "slit_side",
        1,
        vec![vec![Point2::new(0.0, 0.0), Point2::new(30.0, 0.0)]],
    )
    .with_cut_side(CutSide::Outside);
    let box_inside = PartInput::closed("box_in", 1, rect_loop(0.0, 0.0, 20.0, 20.0), vec![])
        .with_cut_side(CutSide::Inside);
    let box_outside = PartInput::closed("box_out", 1, rect_loop(0.0, 0.0, 20.0, 20.0), vec![])
        .with_cut_side(CutSide::Outside);

    let report = nest(&sheet, &[slit_online, slit_side, box_inside, box_outside])
        .expect("nest open chains and cut sides");
    assert!(report.unplaced.is_empty());
    assert_eq!(report.total_placed, 5);
    assert_eq!(report.sheet_count, 1);
    assert_no_inflated_overlap(&report.sheets[0].placements);
    // Open chain net area is 0, closed boxes contribute 20*20 * 2 = 800.
    let expected_pct = 800.0 / (120.0 * 80.0) * 100.0;
    assert!(close(report.sheets[0].utilization_pct, expected_pct, 1e-3));
}

#[test]
fn multi_sheet_overflow_and_exact_per_sheet_counts() {
    let sheet = SheetInput {
        width: 100.0,
        height: 100.0,
        edge_margin: 5.0,
        cut_width: 2.0,
        part_gap: 2.0,
        allow_parts_in_holes: true,
        clearance_mode: ClearanceMode::SeparateCuts,
    };
    // Exact fit: 4 of 40x40 squares per 100x100 sheet:
    // 5 (margin) + 2 (kerf) + 40 + 6 (2*kerf+gap) + 40 + 2 (kerf) + 5 (margin) = 100.0!
    // Request 5 squares -> 4 on sheet 0, 1 on sheet 1.
    let sq = PartInput::closed("tile", 5, rect_loop(0.0, 0.0, 40.0, 40.0), vec![])
        .with_rotation(RotationMode::Locked);
    let small = PartInput::closed("plug", 2, rect_loop(0.0, 0.0, 10.0, 10.0), vec![])
        .with_rotation(RotationMode::Locked);

    let report = nest(&sheet, &[sq, small]).expect("multi sheet nest");
    assert!(report.unplaced.is_empty());
    assert_eq!(report.sheet_count, 2);
    assert_eq!(report.sheets[0].placed_counts.get("tile"), Some(&4));
    assert_eq!(report.sheets[1].placed_counts.get("tile"), Some(&1));
    assert_eq!(report.placed_counts.get("tile"), Some(&5));
    assert_eq!(report.placed_counts.get("plug"), Some(&2));
    for s in &report.sheets {
        assert_no_inflated_overlap(&s.placements);
    }
    let overall_expected = (5.0 * 1600.0 + 2.0 * 100.0) / (2.0 * 10_000.0) * 100.0;
    assert!(close(
        report.overall_utilization_pct,
        overall_expected,
        1e-3
    ));
}

#[test]
fn refuses_non_finite_negative_and_oversized_inputs_by_name() {
    let good_sheet = SheetInput {
        width: 100.0,
        height: 100.0,
        edge_margin: 5.0,
        cut_width: 1.0,
        part_gap: 2.0,
        allow_parts_in_holes: true,
        clearance_mode: ClearanceMode::SeparateCuts,
    };
    let good_part = PartInput::closed("ok", 1, rect_loop(0.0, 0.0, 10.0, 10.0), vec![]);

    let mut bad = good_sheet.clone();
    bad.width = -10.0;
    assert!(matches!(
        nest(&bad, std::slice::from_ref(&good_part)),
        Err(NestError::InvalidInput(msg)) if msg.contains("width")
    ));

    let mut bad = good_sheet.clone();
    bad.height = f64::NAN;
    assert!(matches!(
        nest(&bad, std::slice::from_ref(&good_part)),
        Err(NestError::InvalidInput(msg)) if msg.contains("height")
    ));

    let mut bad = good_sheet.clone();
    bad.edge_margin = -1.0;
    assert!(matches!(
        nest(&bad, std::slice::from_ref(&good_part)),
        Err(NestError::InvalidInput(msg)) if msg.contains("edge_margin")
    ));

    let mut bad = good_sheet.clone();
    bad.cut_width = f64::INFINITY;
    assert!(matches!(
        nest(&bad, std::slice::from_ref(&good_part)),
        Err(NestError::InvalidInput(msg)) if msg.contains("cut_width")
    ));

    let mut bad = good_sheet.clone();
    bad.part_gap = -0.1;
    assert!(matches!(
        nest(&bad, std::slice::from_ref(&good_part)),
        Err(NestError::InvalidInput(msg)) if msg.contains("part_gap")
    ));

    let mut bad = good_sheet.clone();
    bad.width = 1e13;
    assert!(matches!(
        nest(&bad, std::slice::from_ref(&good_part)),
        Err(NestError::OutOfRange(msg)) if msg.contains("width")
    ));

    // Bad rotation step
    let bad_rot = good_part
        .clone()
        .with_rotation(RotationMode::FixedStep(0.0));
    assert!(matches!(
        nest(&good_sheet, &[bad_rot]),
        Err(NestError::InvalidPart { part_id, .. }) if part_id == "ok"
    ));

    // Hole outside outer loop
    let bad_hole = PartInput::closed(
        "bad_hole",
        1,
        rect_loop(0.0, 0.0, 20.0, 20.0),
        vec![rect_loop(30.0, 30.0, 40.0, 40.0)],
    );
    assert!(matches!(
        nest(&good_sheet, &[bad_hole]),
        Err(NestError::InvalidPart { part_id, .. }) if part_id == "bad_hole"
    ));

    // Oversized part vs sheet -> explicit unplaced refusal naming the part
    let huge = PartInput::closed("huge_plate", 1, rect_loop(0.0, 0.0, 200.0, 50.0), vec![]);
    let report = nest(&good_sheet, &[huge]).expect("oversized part recorded in unplaced");
    assert_eq!(report.unplaced.len(), 1);
    assert_eq!(report.unplaced[0].part_id, "huge_plate");
    assert!(report.unplaced[0].reason.contains("huge_plate"));
    assert!(report.strict().is_err());
}
