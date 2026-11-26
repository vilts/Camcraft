//! Rotation modes, mirroring, router single-slot mode, and multi-part batch tests (issue 2158).
//!
//! Gate map (M4.4): covers camcraft_geometry::nesting.

use camcraft_geometry::geo2d::{Loop, Point2};
use camcraft_geometry::nesting::{ClearanceMode, PartInput, RotationMode, SheetInput, nest};

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
fn router_single_slot_mode_fits_shared_channel_and_refuses_under_sized_gap() {
    let mut sheet = SheetInput {
        width: 128.0,
        height: 72.0,
        edge_margin: 5.0,
        cut_width: 6.0,
        part_gap: 6.0,
        allow_parts_in_holes: false,
        clearance_mode: ClearanceMode::RouterSingleSlot,
    };
    let sq = PartInput::closed("panel", 2, rect_loop(0.0, 0.0, 50.0, 50.0), vec![])
        .with_rotation(RotationMode::Locked);

    let single = nest(&sheet, std::slice::from_ref(&sq)).expect("single slot nest");
    assert_eq!(single.sheet_count, 1);
    assert_no_inflated_overlap(&single.sheets[0].placements);

    sheet.clearance_mode = ClearanceMode::SeparateCuts;
    let separate = nest(&sheet, std::slice::from_ref(&sq)).expect("separate cuts nest");
    assert_eq!(separate.sheet_count, 2);

    sheet.clearance_mode = ClearanceMode::RouterSingleSlot;
    sheet.part_gap = 4.0;
    let err = nest(&sheet, &[sq]).expect_err("must refuse part_gap < cut_width");
    let msg = err.to_string();
    assert!(
        msg.contains("part_gap") && msg.contains("cut_width"),
        "unexpected error: {msg}"
    );
}

#[test]
fn fixed_step_rotation_and_mirror_unlock_tight_placements() {
    let sheet = SheetInput {
        width: 70.0,
        height: 70.0,
        edge_margin: 5.0,
        cut_width: 0.0,
        part_gap: 0.0,
        allow_parts_in_holes: false,
        clearance_mode: ClearanceMode::SeparateCuts,
    };
    let bar = Loop::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(72.0, 0.0),
        Point2::new(72.0, 6.0),
        Point2::new(0.0, 6.0),
    ])
    .unwrap();
    let locked =
        PartInput::closed("bar", 1, bar.clone(), vec![]).with_rotation(RotationMode::Locked);
    let r_locked = nest(&sheet, &[locked]).expect("oversized locked returns unplaced");
    assert_eq!(r_locked.total_placed, 0);
    assert_eq!(r_locked.unplaced.len(), 1);
    assert!(r_locked.unplaced[0].reason.contains("bar"));
    assert!(r_locked.strict().is_err());

    let angled =
        PartInput::closed("bar", 1, bar, vec![]).with_rotation(RotationMode::FixedStep(45.0));
    let r_angled = nest(&sheet, &[angled]).expect("45 deg rotation fits");
    assert_eq!(r_angled.total_placed, 1);
}

#[test]
fn allow_mirror_places_chiral_part_in_cavity_and_circular_hole_accepts_inner_part() {
    let sheet = SheetInput {
        width: 50.0,
        height: 35.0,
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
    let host_part = PartInput::closed("host", 1, host, vec![]).with_rotation(RotationMode::Locked);
    let chiral_no_mirror = PartInput::closed("chiral", 1, chiral.clone(), vec![])
        .with_rotation(RotationMode::Orthogonal)
        .with_mirror(false);
    let r_no_mirror = nest(&sheet, &[host_part.clone(), chiral_no_mirror]).unwrap();
    assert_eq!(
        r_no_mirror.sheet_count, 2,
        "unmirrored chiral part must spill to sheet 2"
    );

    let chiral_mirror = PartInput::closed("chiral", 1, chiral, vec![])
        .with_rotation(RotationMode::Orthogonal)
        .with_mirror(true);
    let r_mirror = nest(&sheet, &[host_part, chiral_mirror]).unwrap();
    assert_eq!(
        r_mirror.sheet_count, 1,
        "mirrored chiral part must fit on sheet 1"
    );
    assert_no_inflated_overlap(&r_mirror.sheets[0].placements);
    let placed_chiral = r_mirror.sheets[0]
        .placements
        .iter()
        .find(|p| p.part_id == "chiral")
        .unwrap();
    assert!(placed_chiral.mirrored);

    let circle_sheet = SheetInput {
        width: 140.0,
        height: 140.0,
        edge_margin: 5.0,
        cut_width: 1.0,
        part_gap: 2.0,
        allow_parts_in_holes: true,
        clearance_mode: ClearanceMode::SeparateCuts,
    };
    let ring_pts = |r: f64, cx: f64, cy: f64| -> Loop {
        let n = 16;
        let pts = (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * (i as f64) / (n as f64);
                Point2::new(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect();
        Loop::new(pts).unwrap()
    };
    let flange = PartInput::closed(
        "flange",
        1,
        rect_loop(0.0, 0.0, 120.0, 120.0),
        vec![ring_pts(40.0, 60.0, 60.0)],
    )
    .with_rotation(RotationMode::Locked);
    let washer = PartInput::closed("washer", 1, ring_pts(15.0, 0.0, 0.0), vec![])
        .with_rotation(RotationMode::Locked);
    let r_circle = nest(&circle_sheet, &[flange, washer]).unwrap();
    assert_eq!(r_circle.sheet_count, 1);
    assert_no_inflated_overlap(&r_circle.sheets[0].placements);
    let placed_washer = r_circle.sheets[0]
        .placements
        .iter()
        .find(|p| p.part_id == "washer")
        .unwrap();
    assert_eq!(
        placed_washer.container_part,
        Some(("flange".to_string(), 0))
    );
}

#[test]
fn batch_30_irregular_parts_is_deterministic_and_zero_overlap() {
    let sheet = SheetInput {
        width: 200.0,
        height: 150.0,
        edge_margin: 5.0,
        cut_width: 1.5,
        part_gap: 2.0,
        allow_parts_in_holes: true,
        clearance_mode: ClearanceMode::SeparateCuts,
    };
    let l_part = PartInput::closed("l_arm", 10, l_shape_loop(45.0, 45.0, 16.0), vec![])
        .with_rotation(RotationMode::Orthogonal);
    let frame = PartInput::closed(
        "frame",
        8,
        rect_loop(0.0, 0.0, 50.0, 40.0),
        vec![rect_loop(10.0, 10.0, 40.0, 30.0)],
    )
    .with_rotation(RotationMode::Orthogonal);
    let plug = PartInput::closed("plug", 12, rect_loop(0.0, 0.0, 12.0, 12.0), vec![])
        .with_rotation(RotationMode::Locked);

    let r1 = nest(&sheet, &[l_part.clone(), frame.clone(), plug.clone()]).unwrap();
    let r2 = nest(&sheet, &[l_part, frame, plug]).unwrap();
    assert_eq!(r1, r2);
    assert_eq!(r1.total_requested, 30);
    assert_eq!(r1.total_placed, 30);
    assert!(r1.unplaced.is_empty());
    for s in &r1.sheets {
        assert_no_inflated_overlap(&s.placements);
    }
}
