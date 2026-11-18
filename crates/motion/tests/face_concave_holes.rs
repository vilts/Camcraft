//! Concave-hole facing regressions for #260.
//! Provenance: reduced from the public cutouts report; a horizontal gap
//! can be wide while a nearby return edge excludes that row's cutter.
//! Gate map: covers camcraft_motion::machining2d (face, face_holes).

use camcraft_motion::cl::MoveKind;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{FaceHoles, FaceParams, face};

fn polygon(points: &[(f64, f64)]) -> Loop {
    Loop::new(points.iter().map(|&(x, y)| Point2::new(x, y)).collect()).expect("loop")
}

fn params() -> FaceParams {
    FaceParams {
        tool_diameter: 3.0,
        stepover: 1.5,
        clearance_z: 5.0,
        cut_z: -2.0,
        top_z: None,
        stepdown: None,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        blend_turns: false,
        holes: FaceHoles::Lift,
        stock_top: None,
    }
}

fn concave_return(half_gap: f64) {
    // One U-shaped hole, with material between its arms. The return
    // at Y=0.5 is only 1 mm from the distinguishing row at Y=-0.5.
    let hole = polygon(&[
        (-6.0, -3.0),
        (-half_gap, -3.0),
        (-half_gap, 0.5),
        (half_gap, 0.5),
        (half_gap, -3.0),
        (6.0, -3.0),
        (6.0, 5.0),
        (-6.0, 5.0),
    ]);
    let holes = Region::new(vec![hole.clone()]).expect("hole");
    let p = params();
    let row = Point2::new(0.0, -0.5);
    assert!(half_gap > 0.0, "the notch has a positive horizontal gap");
    assert!(!holes.contains(row), "the horizontal gap contains material");
    let grown = holes.offset(p.tool_diameter * 0.5).expect("grown hole");
    assert!(grown.contains(row), "the nearby return excludes the cutter");
    // Offset arcs are chorded within 5 um; allow 10 um at their boundary,
    // but check cutter-radius clearance, not just the raw hole's interior.
    let forbidden_interior = holes
        .offset(p.tool_diameter * 0.5 - 0.01)
        .expect("forbidden interior");
    let region = Region::new(vec![
        polygon(&[(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)]),
        hole,
    ])
    .expect("plate");
    assert!(
        region.contains(row),
        "the fixture really has a material tongue"
    );

    for climb in [true, false] {
        let path = face(&region, &FaceParams { climb, ..p }).expect("not a narrow-gap refusal");
        assert!(path.all_finite());
        assert!(
            path.notes.iter().any(|n| n.contains("unfaced")),
            "{:?}",
            path.notes
        );
        assert!(
            path.notes.iter().any(|n| n.contains("lifted over")),
            "{:?}",
            path.notes
        );
        let feeds: Vec<_> = path
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Feed)
            .collect();
        assert!(!feeds.is_empty());
        assert!(
            feeds.iter().any(|m| m.from.y == -0.5 && m.to.y == -0.5),
            "the distinguishing row runs"
        );
        for m in feeds {
            for k in 0..=32 {
                let t = f64::from(k) / 32.0;
                let at = Point2::new(
                    m.from.x + t * (m.to.x - m.from.x),
                    m.from.y + t * (m.to.y - m.from.y),
                );
                assert!(
                    !forbidden_interior.contains(at),
                    "a cutter entered the hole: {m:?}"
                );
            }
        }
    }
}

#[test]
fn a_wide_concave_gap_generates_and_reports_the_unfaced_return() {
    assert!(4.0 > params().tool_diameter);
    concave_return(2.0);
}

#[test]
fn a_narrow_concave_return_is_not_a_gap_between_separate_holes() {
    assert!(2.0 < params().tool_diameter);
    concave_return(1.0);
}

#[test]
fn a_gap_exactly_one_tool_wide_still_refuses_when_no_row_piece_fits() {
    let region = Region::new(vec![
        polygon(&[(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)]),
        polygon(&[(-6.5, -5.0), (-1.5, -5.0), (-1.5, 5.0), (-6.5, 5.0)]),
        polygon(&[(1.5, -5.0), (6.5, -5.0), (6.5, 5.0), (1.5, 5.0)]),
    ])
    .expect("plate");
    let error = face(&region, &params())
        .expect_err("no positive-length row piece")
        .to_string();
    assert!(
        error.contains("3.00 mm apart") && error.contains("no wider than the 3.000 mm tool"),
        "{error}"
    );
}
