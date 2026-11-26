//! #2811: 3D Finish option to stop the waterline ring pass on steep flanks.
//!
//! Base synthetic fixture (`slab_with_holes`): 24 x 160 mm bar with a 2 deg
//! sloped top face and two through holes, plus an all-steep vertical wall
//! fixture and a flat-only plateau top fixture.
//!
//! Gate map (#2811): covers camcraft_motion::finishing3d (raster, rings, validate).

use camcraft_motion::cl::MoveKind;
use camcraft_motion::finishing3d::{
    blend, morph, parallel, waterline, Finish3DParams, FinishPattern, GapLink, WATERLINES_OFF_NOTE,
};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::GenError;

const TAN2: f64 = 0.034_920_722_784_327_5; // tan(2 deg)

fn top_at(y: f64) -> f64 {
    5.0 + TAN2 * y
}

fn quad(f: &mut Vec<Triangle>, a: Point3, b: Point3, c: Point3, d: Point3) {
    f.push(Triangle::new(a, b, c));
    f.push(Triangle::new(a, c, d));
}

fn slab_with_holes() -> Vec<Triangle> {
    let (x0, x1, y0, y1) = (0.0, 24.0, 0.0, 160.0);
    let (xa0, xa1, ya0, ya1) = (5.5, 18.5, 88.0, 101.0);
    let (xb0, xb1, yb0, yb1) = (9.5, 14.5, 42.0, 48.0);
    type SlabBand = (f64, f64, Vec<(f64, f64)>);
    let mut f: Vec<Triangle> = Vec::new();
    let bands: Vec<SlabBand> = vec![
        (y0, yb0, vec![(x0, x1)]),
        (yb0, yb1, vec![(x0, xb0), (xb1, x1)]),
        (yb1, ya0, vec![(x0, x1)]),
        (ya0, ya1, vec![(x0, xa0), (xa1, x1)]),
        (ya1, y1, vec![(x0, x1)]),
    ];
    for (ba, bb, xs) in bands {
        for (sa, sb) in xs {
            quad(
                &mut f,
                Point3::new(sa, ba, top_at(ba)),
                Point3::new(sb, ba, top_at(ba)),
                Point3::new(sb, bb, top_at(bb)),
                Point3::new(sa, bb, top_at(bb)),
            );
            quad(
                &mut f,
                Point3::new(sa, ba, 0.0),
                Point3::new(sa, bb, 0.0),
                Point3::new(sb, bb, 0.0),
                Point3::new(sb, ba, 0.0),
            );
        }
    }
    quad(&mut f, Point3::new(x0, y0, 0.0), Point3::new(x1, y0, 0.0), Point3::new(x1, y0, top_at(y0)), Point3::new(x0, y0, top_at(y0)));
    quad(&mut f, Point3::new(x1, y1, 0.0), Point3::new(x0, y1, 0.0), Point3::new(x0, y1, top_at(y1)), Point3::new(x1, y1, top_at(y1)));
    quad(&mut f, Point3::new(x0, y1, 0.0), Point3::new(x0, y0, 0.0), Point3::new(x0, y0, top_at(y0)), Point3::new(x0, y1, top_at(y1)));
    quad(&mut f, Point3::new(x1, y0, 0.0), Point3::new(x1, y1, 0.0), Point3::new(x1, y1, top_at(y1)), Point3::new(x1, y0, top_at(y0)));
    for (hx0, hx1, hy0, hy1) in [(xa0, xa1, ya0, ya1), (xb0, xb1, yb0, yb1)] {
        let (ta, tb) = (top_at(hy0), top_at(hy1));
        quad(&mut f, Point3::new(hx0, hy0, 0.0), Point3::new(hx0, hy0, ta), Point3::new(hx1, hy0, ta), Point3::new(hx1, hy0, 0.0));
        quad(&mut f, Point3::new(hx1, hy1, 0.0), Point3::new(hx1, hy1, tb), Point3::new(hx0, hy1, tb), Point3::new(hx0, hy1, 0.0));
        quad(&mut f, Point3::new(hx0, hy0, 0.0), Point3::new(hx0, hy1, 0.0), Point3::new(hx0, hy1, tb), Point3::new(hx0, hy0, ta));
        quad(&mut f, Point3::new(hx1, hy1, 0.0), Point3::new(hx1, hy0, 0.0), Point3::new(hx1, hy0, ta), Point3::new(hx1, hy1, tb));
    }
    f
}

fn probe_params(pattern: FinishPattern, bottom_z: f64, waterlines: bool) -> Finish3DParams {
    Finish3DParams {
        pattern,
        stepover: None,
        tool_diameter: 7.9375,
        tolerance: 0.05,
        clearance_z: 15.0,
        retract_z: None,
        top_z: top_at(160.0),
        bottom_z,
        feed: 500.0,
        plunge_feed: 250.0,
        climb: true,
        ramp: false,
        entry_ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        steep_stepover: None,
        pencil_reference: None,
        gap_link: GapLink::Jump,
        waterlines,
    }
}

fn classify_feeds(moves: &[camcraft_motion::cl::Move]) -> (usize, usize) {
    let mut xfam = 0usize;
    let mut yfam = 0usize;
    for m in moves {
        if m.kind != MoveKind::Feed {
            continue;
        }
        let (dx, dy) = (m.to.x - m.from.x, m.to.y - m.from.y);
        let xy = (dx * dx + dy * dy).sqrt();
        if xy < 0.5 {
            continue;
        }
        let deg = dy.abs().max(f64::EPSILON).atan2(dx.abs()).to_degrees();
        if deg < 45.0 {
            xfam += 1;
        } else if deg >= 67.5 {
            yfam += 1;
        }
    }
    (xfam, yfam)
}

#[test]
fn parallel_waterlines_on_vs_off_on_sloped_bar_with_holes() {
    let facets = slab_with_holes();
    for (bottom_z, expected_steep) in [(9.75, 5usize), (0.0, 24usize)] {
        let on = parallel(&facets, &probe_params(FinishPattern::Parallel, bottom_z, true))
            .expect("waterlines on");
        let (xfam_on, yfam_on) = classify_feeds(&on.moves);
        assert!(xfam_on > 0 && yfam_on > 0, "bottom {bottom_z}: expected both raster and ring moves, got x={xfam_on} y={yfam_on}");
        assert!(on.notes.is_empty(), "waterlines on writes no note: {:?}", on.notes);

        let off = parallel(&facets, &probe_params(FinishPattern::Parallel, bottom_z, false))
            .expect("waterlines off");
        let (xfam_off, yfam_off) = classify_feeds(&off.moves);
        assert!(xfam_off > 0, "bottom {bottom_z}: expected raster moves");
        assert_eq!(yfam_off, 0, "bottom {bottom_z}: expected zero Y-family ring moves when waterlines off");
        assert!(off.all_finite());
        let last = off.moves.last().expect("final clearance rapid");
        assert_eq!(last.kind, MoveKind::Rapid);
        assert!((last.to.z - 15.0).abs() < 1e-9);
        assert!(
            off.notes.iter().any(|n| {
                n.starts_with(WATERLINES_OFF_NOTE)
                    && n.contains(&format!("{expected_steep} steep flank"))
                    && n.contains("left uncut")
            }),
            "expected steep flank note with count {expected_steep}, got {:?}",
            off.notes
        );
    }
}

#[test]
fn waterlines_off_applies_to_parallel_multi_scallop_and_steep_shallow() {
    let facets = slab_with_holes();
    for pat in [
        FinishPattern::ParallelMulti,
        FinishPattern::Scallop,
        FinishPattern::SteepShallow,
    ] {
        let on = parallel(&facets, &probe_params(pat, 0.0, true)).expect("waterlines on");
        let off = parallel(&facets, &probe_params(pat, 0.0, false)).expect("waterlines off");
        assert!(
            on.moves.iter().any(|m| m.kind == MoveKind::Feed && m.to.z < 4.5),
            "{pat:?} with waterlines on cuts ring levels down the walls"
        );
        assert!(
            off.moves.iter().filter(|m| m.kind == MoveKind::Feed).all(|m| m.from.z >= 4.9 && m.to.z >= 4.9),
            "{pat:?} with waterlines off must cut only the sloped top face"
        );
        assert!(
            off.notes.iter().any(|n| n.starts_with(WATERLINES_OFF_NOTE) && n.contains("24 steep flank")),
            "{pat:?} must note uncut steep flanks: {:?}",
            off.notes
        );
    }
}

#[test]
fn waterlines_off_refuses_all_steep_model_with_empty_error() {
    let wall = vec![
        Triangle::new(Point3::new(-5.0, 0.0, 0.0), Point3::new(5.0, 0.0, 5.0), Point3::new(5.0, 0.0, 0.0)),
        Triangle::new(Point3::new(-5.0, 0.0, 0.0), Point3::new(-5.0, 0.0, 5.0), Point3::new(5.0, 0.0, 5.0)),
    ];
    for pat in [
        FinishPattern::Parallel,
        FinishPattern::ParallelMulti,
        FinishPattern::Scallop,
        FinishPattern::SteepShallow,
    ] {
        let mut on = probe_params(pat, 0.0, true);
        on.tool_diameter = 6.0;
        on.top_z = 5.0;
        assert!(parallel(&wall, &on).is_ok(), "{pat:?} with waterlines on cuts the wall");

        let mut off = probe_params(pat, 0.0, false);
        off.tool_diameter = 6.0;
        off.top_z = 5.0;
        let err = parallel(&wall, &off).expect_err("all-steep model with waterlines off must refuse");
        assert!(
            matches!(err, GenError::Empty(ref m) if m.contains("waterline ring pass is off")),
            "expected GenError::Empty naming waterlines off, got {err:?}"
        );
    }
}

#[test]
fn waterlines_off_on_flat_surface_writes_no_steep_flank_note() {
    let mut flat = Vec::new();
    quad(
        &mut flat,
        Point3::new(0.0, 0.0, 10.0),
        Point3::new(20.0, 0.0, 10.0),
        Point3::new(20.0, 20.0, 10.0),
        Point3::new(0.0, 20.0, 10.0),
    );
    let mut p = probe_params(FinishPattern::Parallel, 0.0, false);
    p.tool_diameter = 6.0;
    p.top_z = 10.0;
    let out = parallel(&flat, &p).expect("flat surface cuts cleanly");
    assert!(out.notes.is_empty(), "no steep flanks means no note: {:?}", out.notes);
}

#[test]
fn ring_dependent_patterns_refuse_waterlines_off_by_name() {
    let facets = slab_with_holes();
    for (pat, res) in [
        (FinishPattern::Waterline, waterline(&facets, &probe_params(FinishPattern::Waterline, 0.0, false))),
        (FinishPattern::Blend, blend(&facets, &probe_params(FinishPattern::Blend, 0.0, false))),
        (FinishPattern::Morph, morph(&facets, &probe_params(FinishPattern::Morph, 0.0, false))),
    ] {
        let err = res.expect_err("ring-dependent pattern must refuse waterlines off");
        assert!(
            matches!(err, GenError::Invalid(ref m) if m.contains("waterline")),
            "{pat:?}: expected GenError::Invalid, got {err:?}"
        );
    }
}
