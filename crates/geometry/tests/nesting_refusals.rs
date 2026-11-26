//! Refusal, spacing, seam, and multi-level hole tests for 2D nesting (issue 2158).
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

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn nominal_gap_x(
    a: &camcraft_geometry::nesting::PlacedPart,
    b: &camcraft_geometry::nesting::PlacedPart,
) -> f64 {
    let ba = Region::new(vec![a.outer.clone().unwrap()])
        .unwrap()
        .bounds()
        .unwrap();
    let bb = Region::new(vec![b.outer.clone().unwrap()])
        .unwrap()
        .bounds()
        .unwrap();
    (bb.0.x - ba.1.x).max(ba.0.x - bb.1.x)
}

#[test]
fn cut_side_inflations_leave_exact_uncut_gap() {
    let sheet = SheetInput {
        width: 100.0,
        height: 40.0,
        edge_margin: 5.0,
        cut_width: 2.0,
        part_gap: 4.0,
        allow_parts_in_holes: false,
        clearance_mode: ClearanceMode::SeparateCuts,
    };

    // Outside + Outside => 2*cut_width + part_gap = 8.0 mm nominal gap
    let p_out = PartInput::closed("out", 2, rect_loop(0.0, 0.0, 20.0, 20.0), vec![])
        .with_cut_side(CutSide::Outside)
        .with_rotation(RotationMode::Locked);
    let r_out = nest(&sheet, &[p_out]).unwrap();
    assert!(close(
        nominal_gap_x(
            &r_out.sheets[0].placements[0],
            &r_out.sheets[0].placements[1]
        ),
        8.0,
        1e-4
    ));

    // OnLine + OnLine => cut_width + part_gap = 6.0 mm nominal gap
    let p_on = PartInput::closed("on", 2, rect_loop(0.0, 0.0, 20.0, 20.0), vec![])
        .with_cut_side(CutSide::OnLine)
        .with_rotation(RotationMode::Locked);
    let r_on = nest(&sheet, &[p_on]).unwrap();
    assert!(close(
        nominal_gap_x(&r_on.sheets[0].placements[0], &r_on.sheets[0].placements[1]),
        6.0,
        1e-4
    ));

    // Inside + Inside => part_gap = 4.0 mm nominal gap
    let p_in = PartInput::closed("in", 2, rect_loop(0.0, 0.0, 20.0, 20.0), vec![])
        .with_cut_side(CutSide::Inside)
        .with_rotation(RotationMode::Locked);
    let r_in = nest(&sheet, &[p_in]).unwrap();
    assert!(close(
        nominal_gap_x(&r_in.sheets[0].placements[0], &r_in.sheets[0].placements[1]),
        4.0,
        1e-4
    ));

    // Outside (larger) + Inside (smaller) => cut_width + part_gap = 6.0 mm nominal gap
    let big_out = PartInput::closed("a_out", 1, rect_loop(0.0, 0.0, 22.0, 20.0), vec![])
        .with_cut_side(CutSide::Outside)
        .with_rotation(RotationMode::Locked);
    let small_in = PartInput::closed("b_in", 1, rect_loop(0.0, 0.0, 18.0, 20.0), vec![])
        .with_cut_side(CutSide::Inside)
        .with_rotation(RotationMode::Locked);
    let r_mix = nest(&sheet, &[big_out, small_in]).unwrap();
    assert!(close(
        nominal_gap_x(
            &r_mix.sheets[0].placements[0],
            &r_mix.sheets[0].placements[1]
        ),
        6.0,
        1e-4
    ));
}

#[test]
fn chiral_cavity_on_larger_sheet_finds_zero_slack_seam() {
    // 100x35 sheet: host sits at x=[0,50], leaving both an exact cavity at [20,50]x[10,35]
    // AND free space at x=[50,100]. Note that placing at (tx=50, ty=10) inside the cavity
    // keeps max_y=35 (if sheet is 35 high), whereas placing at (50, 0) also fits at y=0.
    // On a 50x45 sheet (extra height at top!), (50, 10) is strictly interior to IFP ([30..50]x[0..20])
    // and ONLY found via internal NFP seam tracking!
    let sheet = SheetInput {
        width: 50.0,
        height: 45.0,
        edge_margin: 0.0,
        cut_width: 0.0,
        part_gap: 0.0,
        allow_parts_in_holes: false,
        clearance_mode: ClearanceMode::SeparateCuts,
    };
    let host = Loop::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(50.0, 0.0),
        Point2::new(50.0, 10.0),
        Point2::new(40.0, 10.0),
        Point2::new(40.0, 25.0),
        Point2::new(30.0, 25.0),
        Point2::new(30.0, 15.0),
        Point2::new(20.0, 15.0),
        Point2::new(20.0, 35.0),
        Point2::new(0.0, 35.0),
    ])
    .unwrap();
    let chiral = Loop::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(10.0, 0.0),
        Point2::new(10.0, 15.0),
        Point2::new(20.0, 15.0),
        Point2::new(20.0, 5.0),
        Point2::new(30.0, 5.0),
        Point2::new(30.0, 25.0),
        Point2::new(0.0, 25.0),
    ])
    .unwrap();
    let p_host = PartInput::closed("host", 1, host, vec![]).with_rotation(RotationMode::Locked);
    let p_chiral = PartInput::closed("chiral", 1, chiral, vec![])
        .with_rotation(RotationMode::Locked)
        .with_mirror(true);
    let r = nest(&sheet, &[p_host, p_chiral]).unwrap();
    assert_eq!(r.sheet_count, 1);
    let placed_c = &r.sheets[0].placements[1];
    // Inside the cavity max_y == 35.0; sitting above host would have max_y >= 40.0.
    assert!(
        close(placed_c.ty, 10.0, 1e-4) && placed_c.mirrored,
        "expected chiral inside cavity at ty=10 (max_y=35), got tx={} ty={} mir={}",
        placed_c.tx,
        placed_c.ty,
        placed_c.mirrored
    );
}

#[test]
fn multi_level_parts_in_holes_and_open_slit_clearance() {
    let sheet = SheetInput {
        width: 140.0,
        height: 140.0,
        edge_margin: 5.0,
        cut_width: 1.0,
        part_gap: 2.0,
        allow_parts_in_holes: true,
        clearance_mode: ClearanceMode::SeparateCuts,
    };
    let big = PartInput::closed(
        "big",
        1,
        rect_loop(0.0, 0.0, 120.0, 120.0),
        vec![rect_loop(10.0, 10.0, 110.0, 110.0)],
    )
    .with_rotation(RotationMode::Locked);
    let mid = PartInput::closed(
        "mid",
        1,
        rect_loop(0.0, 0.0, 80.0, 80.0),
        vec![rect_loop(10.0, 10.0, 70.0, 70.0)],
    )
    .with_rotation(RotationMode::Locked);
    let tiny = PartInput::closed("tiny", 1, rect_loop(0.0, 0.0, 30.0, 30.0), vec![])
        .with_rotation(RotationMode::Locked);

    let report = nest(&sheet, &[big, mid, tiny]).unwrap();
    assert_eq!(report.sheet_count, 1);
    let p_mid = report.sheets[0]
        .placements
        .iter()
        .find(|p| p.part_id == "mid")
        .unwrap();
    let p_tiny = report.sheets[0]
        .placements
        .iter()
        .find(|p| p.part_id == "tiny")
        .unwrap();
    assert_eq!(p_mid.container_part, Some(("big".into(), 0)));
    assert_eq!(p_tiny.container_part, Some(("mid".into(), 0)));

    // An open chain bisecting a hole blocks placing a large square inside that hole.
    let bisected = PartInput::closed(
        "slit_host",
        1,
        rect_loop(0.0, 0.0, 120.0, 120.0),
        vec![rect_loop(20.0, 20.0, 100.0, 100.0)],
    )
    .with_open_chains(vec![vec![
        Point2::new(60.0, 20.0),
        Point2::new(60.0, 100.0),
    ]])
    .with_rotation(RotationMode::Locked);
    let blocker = PartInput::closed("blocker", 1, rect_loop(0.0, 0.0, 50.0, 50.0), vec![])
        .with_rotation(RotationMode::Locked);
    let r_bisected = nest(&sheet, &[bisected, blocker]).unwrap();
    assert_eq!(
        r_bisected.sheet_count, 2,
        "open slit across hole must block 50x50 inner part"
    );
}

#[test]
fn refuses_malformed_parts_and_edge_cases_by_name() {
    let sheet = SheetInput {
        width: 100.0,
        height: 100.0,
        edge_margin: 5.0,
        cut_width: 1.0,
        part_gap: 2.0,
        allow_parts_in_holes: true,
        clearance_mode: ClearanceMode::SeparateCuts,
    };

    // Empty part id
    let empty_id = PartInput::closed("  ", 1, rect_loop(0.0, 0.0, 10.0, 10.0), vec![]);
    assert!(
        matches!(nest(&sheet, &[empty_id]), Err(NestError::InvalidInput(m)) if m.contains("empty"))
    );

    // Duplicate part id
    let p1 = PartInput::closed("dup", 1, rect_loop(0.0, 0.0, 10.0, 10.0), vec![]);
    let p2 = PartInput::closed("dup", 1, rect_loop(0.0, 0.0, 15.0, 15.0), vec![]);
    assert!(
        matches!(nest(&sheet, &[p1, p2]), Err(NestError::InvalidInput(m)) if m.contains("dup"))
    );

    // Part with neither outer nor open chains
    let empty_part = PartInput::open("nothing", 1, vec![]);
    assert!(matches!(
        nest(&sheet, &[empty_part]),
        Err(NestError::InvalidPart { part_id, .. }) if part_id == "nothing"
    ));

    // Collinear zero-area outer loop
    let col = Loop::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(5.0, 0.0),
        Point2::new(10.0, 0.0),
    ])
    .unwrap();
    let zero_area = PartInput::closed("flat", 1, col, vec![]);
    assert!(matches!(
        nest(&sheet, &[zero_area]),
        Err(NestError::InvalidPart { part_id, .. }) if part_id == "flat"
    ));

    // Overlapping holes
    let overlap_holes = PartInput::closed(
        "olap",
        1,
        rect_loop(0.0, 0.0, 40.0, 40.0),
        vec![
            rect_loop(5.0, 5.0, 20.0, 20.0),
            rect_loop(15.0, 15.0, 30.0, 30.0),
        ],
    );
    assert!(matches!(
        nest(&sheet, &[overlap_holes]),
        Err(NestError::InvalidPart { part_id, .. }) if part_id == "olap"
    ));

    // Empty usable sheet from edge_margin
    let mut bad_margin = sheet.clone();
    bad_margin.edge_margin = 50.0;
    let ok = PartInput::closed("ok", 1, rect_loop(0.0, 0.0, 10.0, 10.0), vec![]);
    assert!(matches!(
        nest(&bad_margin, std::slice::from_ref(&ok)),
        Err(NestError::InvalidInput(m)) if m.contains("edge_margin")
    ));

    // Open chain with zero cut_width + zero part_gap
    let mut zero_clear = sheet.clone();
    zero_clear.cut_width = 0.0;
    zero_clear.part_gap = 0.0;
    let open = PartInput::open(
        "slit",
        1,
        vec![vec![Point2::new(0.0, 0.0), Point2::new(10.0, 0.0)]],
    );
    assert!(matches!(
        nest(&zero_clear, &[open]),
        Err(NestError::InvalidPart { part_id, .. }) if part_id == "slit"
    ));

    // Open chain with coordinate out of range
    let oob = PartInput::open(
        "far",
        1,
        vec![vec![Point2::new(0.0, 0.0), Point2::new(1e13, 0.0)]],
    );
    assert!(matches!(nest(&sheet, &[oob]), Err(NestError::OutOfRange(m)) if m.contains("far")));

    // Too-fine rotation step
    let tiny_step = ok.clone().with_rotation(RotationMode::FixedStep(0.01));
    assert!(matches!(
        nest(&sheet, &[tiny_step]),
        Err(NestError::InvalidPart { part_id, .. }) if part_id == "ok"
    ));

    // Empty parts & zero quantity
    let empty_report = nest(&sheet, &[]).unwrap();
    assert_eq!(empty_report.sheet_count, 0);
    assert_eq!(empty_report.total_placed, 0);
    let zero_qty = nest(
        &sheet,
        &[PartInput::closed(
            "q0",
            0,
            rect_loop(0.0, 0.0, 10.0, 10.0),
            vec![],
        )],
    )
    .unwrap();
    assert_eq!(zero_qty.sheet_count, 0);
}

#[test]
fn refuses_unbounded_and_overflowing_requested_quantities() {
    let sheet = SheetInput {
        width: 100.0,
        height: 100.0,
        ..SheetInput::default()
    };
    // Oversized parts never enter the per-instance placement loop, even without the guard.
    let huge = PartInput::closed(
        "huge",
        usize::MAX,
        rect_loop(0.0, 0.0, 200.0, 200.0),
        vec![],
    );
    let one = PartInput::closed("one", 1, rect_loop(0.0, 0.0, 200.0, 200.0), vec![]);
    for parts in [vec![huge.clone()], vec![huge, one]] {
        let err = nest(&sheet, &parts).expect_err("unbounded quantity must be refused");
        assert!(err.to_string().contains("quantity"), "{err}");
    }
}

#[test]
fn refuses_requested_quantity_sum_overflow() {
    let sheet = SheetInput {
        width: 100.0,
        height: 100.0,
        ..SheetInput::default()
    };
    let first = PartInput::closed("first", 10_000, rect_loop(0.0, 0.0, 200.0, 200.0), vec![]);
    let second = PartInput::closed(
        "second",
        usize::MAX,
        rect_loop(0.0, 0.0, 200.0, 200.0),
        vec![],
    );
    let err = nest(&sheet, &[first, second]).expect_err("quantity sum must not overflow");
    let msg = err.to_string();
    assert!(
        msg.contains("second") && msg.contains("quantity") && msg.contains("overflows"),
        "{msg}"
    );
}

#[test]
fn refuses_micron_hole_escape_by_name() {
    let sheet = SheetInput {
        width: 10.0,
        height: 10.0,
        edge_margin: 0.0,
        cut_width: 0.0,
        part_gap: 0.0,
        allow_parts_in_holes: true,
        clearance_mode: ClearanceMode::SeparateCuts,
    };
    let part = PartInput::closed(
        "micron_escape",
        1,
        rect_loop(0.0, 0.0, 1.0, 1.0),
        vec![rect_loop(0.5, 0.2, 1.000001, 0.8)],
    );
    let err = nest(&sheet, &[part]).expect_err("a non-empty escape must be refused");
    assert!(matches!(err, NestError::InvalidPart { part_id, reason }
            if part_id == "micron_escape" && reason == "hole loop 0 lies outside or intersects outer loop"));
}

#[test]
fn refuses_sub_epsilon_hole_overlap_by_name() {
    let sheet = SheetInput {
        width: 10.0,
        height: 10.0,
        edge_margin: 0.0,
        cut_width: 0.0,
        part_gap: 0.0,
        allow_parts_in_holes: true,
        clearance_mode: ClearanceMode::SeparateCuts,
    };
    let part = PartInput::closed(
        "micron_overlap",
        1,
        rect_loop(0.0, 0.0, 1.0, 1.0),
        vec![
            rect_loop(0.1, 0.2, 0.5, 0.8),
            rect_loop(0.499999, 0.2, 0.9, 0.8),
        ],
    );
    let err = nest(&sheet, &[part]).expect_err("a non-empty hole overlap must be refused");
    assert!(matches!(err, NestError::InvalidPart { part_id, reason }
            if part_id == "micron_overlap" && reason == "hole loop 1 overlaps hole loop 0"));
}
