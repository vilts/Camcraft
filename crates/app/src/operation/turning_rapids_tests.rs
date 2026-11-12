//! Unit pins of `turning_rapids` (#103): the meridian law on synthetic paths - split out to keep the law's file under the size budget.

use super::*;
use crate::operation::turning_credit::Pilot;

#[test]
fn the_meridian_distance_is_to_the_segment_not_the_line() {
    assert!((seg_dist((0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)) - 1.0).abs() < 1e-12);
    assert!((seg_dist((3.0, 0.0), (-1.0, 0.0), (1.0, 0.0)) - 2.0).abs() < 1e-12);
    assert!((seg_dist((2.0, 2.0), (0.0, 0.0), (0.0, 0.0)) - 8f64.sqrt()).abs() < 1e-12);
}

fn cylinder(r: f64, z0: f64, z1: f64) -> Vec<Triangle> {
    let v = |j: usize, r: f64, z: f64| {
        let th = std::f64::consts::TAU * j as f64 / 32.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut f = Vec::new();
    for j in 0..32 {
        let (a, b, c, d) = (v(j, r, z0), v(j + 1, r, z0), v(j, r, z1), v(j + 1, r, z1));
        f.push(Triangle::new(a, b, d));
        f.push(Triangle::new(a, d, c));
        f.push(Triangle::new(Point3::new(0.0, 0.0, z0), b, a));
        f.push(Triangle::new(Point3::new(0.0, 0.0, z1), c, d));
    }
    f
}

fn mv(kind: MoveKind, from: Point3, to: Point3) -> crate::cl::Move {
    crate::cl::Move { rotary: None, kind, from, to, feed: (kind != MoveKind::Rapid).then_some(100.0), arc: None, axis: None }
}

/// The axial rapids of `moves` the meridian law convicts, a 12 mm
/// part in a 20 mm bar from Z -16 to 16, a 0.8 nose.
fn convicted(moves: Vec<crate::cl::Move>) -> Vec<Warning> {
    convicted_in(moves, 16.0)
}

fn convicted_in(moves: Vec<crate::cl::Move>, bar_top: f64) -> Vec<Warning> {
    let params = OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: crate::operation::Height::fixed(25.0),
        top_z: crate::operation::Height::fixed(16.0),
        bottom_z: crate::operation::Height::fixed(-16.0),
        stepdown: 0.8,
        stock_to_leave: 0.25,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    let path = Toolpath { moves, cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), threads: Vec::new(), probes: Vec::new(), breaks: Vec::new(), plasma: None };
    let (part, stock) = (cylinder(6.0, -16.0, 16.0), cylinder(10.0, -16.0, bar_top));
    let cx = Meridian { part: &part, stock: Some(&stock), nose: 0.8, margin: 0.01, z_tol: 1e-6, drills: &[], band: 0.0, body: None, ahead: &[], outward: None };
    let mut warns = Vec::new();
    judge_axial_rapids(&params, &path, &cx, &mut PriorCell::new(None), &mut warns);
    warns
}

#[test]
fn an_axial_rapid_is_judged_in_the_meridian_against_the_revolved_bar() {
    let p = Point3::new;
    // Into the bar at r 8, before any cut: convicted by index.
    assert_eq!(convicted(vec![mv(MoveKind::Rapid, p(8.0, 0.0, 25.0), p(8.0, 0.0, 5.0))]), vec![Warning::RapidInStock { at: 0 }]);
    // Beside the bar (r 10.9, the nose clear of r 10): clean.
    assert!(convicted(vec![mv(MoveKind::Rapid, p(10.9, 0.0, 25.0), p(10.9, 0.0, 5.0))]).is_empty());
    // A pass turned r 8 down to Z 0 from beside the bar's end; the
    // retract back up along that turned diameter is in its own air.
    let turned = vec![
        mv(MoveKind::Rapid, p(8.0, 0.0, 25.0), p(8.0, 0.0, 17.0)),
        mv(MoveKind::Feed, p(8.0, 0.0, 17.0), p(8.0, 0.0, 0.0)),
        mv(MoveKind::Rapid, p(8.0, 0.0, 0.0), p(8.0, 0.0, 25.0)),
        // ...but a rapid down past that pass's floor is not.
        mv(MoveKind::Rapid, p(8.0, 0.0, 25.0), p(8.0, 0.0, -4.0)),
    ];
    assert_eq!(convicted(turned), vec![Warning::RapidInStock { at: 3 }]);
}

#[test]
fn a_single_groove_at_the_bars_end_is_judged_across_its_band() {
    // Review B2: part r 11 (Z -60..10, long enough to fit an axis) to Z 10, the bar r 12.5 runs 2 mm past it
    // (to Z 12), unfaced. One 5 mm groove at the part's end: its
    // centre stands w/2 past the end, at Z 12.5 - over the bar's
    // end face - but the blade's lower half spans Z 10..12.5.
    let params = OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: crate::operation::Height::fixed(25.0),
        top_z: crate::operation::Height::fixed(10.0),
        bottom_z: crate::operation::Height::fixed(-20.0),
        stepdown: 0.8,
        stock_to_leave: 0.25,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    let p = Point3::new;
    let path = Toolpath {
        moves: vec![
            mv(MoveKind::Rapid, p(11.01, 0.0, 25.0), p(11.01, 0.0, 12.5)),
            mv(MoveKind::Feed, p(11.01, 0.0, 12.5), p(9.0, 0.0, 12.5)),
        ],
        cycles: Vec::new(),
        notes: Vec::new(),
        comps: Vec::new(),
        threads: Vec::new(),
        probes: Vec::new(),
        breaks: Vec::new(),
        plasma: None,
    };
    let (part, stock) = (cylinder(11.0, -60.0, 10.0), cylinder(12.5, -60.0, 12.0));
    let judge = |band: f64| {
        let cx = Meridian { part: &part, stock: Some(&stock), nose: 0.0, margin: 0.01, z_tol: 1e-6, drills: &[], band, body: None, ahead: &[], outward: None };
        let mut warns = Vec::new();
        judge_axial_rapids(&params, &path, &cx, &mut PriorCell::new(None), &mut warns);
        warns
    };
    assert_eq!(judge(2.5), vec![Warning::RapidInStock { at: 0 }], "the blade's band meets the bar's end");
    assert!(judge(0.0).is_empty(), "the band's centre alone stands over the end face - why the band is read");
}

/// A tube r_in..r_out from z0 to z1, 32 ways: a finished bore.
fn tube(ri: f64, ro: f64, z0: f64, z1: f64) -> Vec<Triangle> {
    let v = |j: usize, r: f64, z: f64| {
        let th = std::f64::consts::TAU * j as f64 / 32.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut f = Vec::new();
    for j in 0..32 {
        for (ra, za, rb, zb) in [(ro, z0, ro, z1), (ro, z1, ri, z1), (ri, z1, ri, z0), (ri, z0, ro, z0)] {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            f.push(Triangle::new(a1, a2, b2));
            f.push(Triangle::new(a1, b2, b1));
        }
    }
    f
}

#[test]
fn a_ten_mm_bar_with_its_nose_on_the_axis_in_an_eleven_mm_pilot_is_convicted() {
    // Review B3: the insert sits at the bar's edge, the body toward
    // the axis. Nose on the axis: the Ø10 body reaches r 10, through
    // the Ø11 pilot's wall. Nose at r 4.8 (the kernel's approach:
    // bar and nose centred): it spans r -5.2..5.2, clear.
    let params = OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: crate::operation::Height::fixed(60.0),
        top_z: crate::operation::Height::fixed(40.0),
        bottom_z: crate::operation::Height::fixed(-10.0),
        stepdown: 0.8,
        stock_to_leave: 0.25,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    let (part, stock) = (tube(6.0, 10.0, -60.0, 50.0), cylinder(10.0, -60.0, 50.0));
    let p = Point3::new;
    let drills = [DrillAhead {
        radius: 5.5,
        cone: Some(0.6),
        moves: vec![mv(MoveKind::Feed, p(0.0, 0.0, 50.0), p(0.0, 0.0, -16.0))],
    }];
    let judge = |r: f64| {
        let path = Toolpath {
            moves: vec![mv(MoveKind::Rapid, p(r, 0.0, 60.0), p(r, 0.0, -10.0))],
            cycles: Vec::new(),
            notes: Vec::new(),
            comps: Vec::new(),
            threads: Vec::new(),
            probes: Vec::new(),
            breaks: Vec::new(),
            plasma: None,
        };
        let cx = Meridian {
            part: &part,
            stock: Some(&stock),
            nose: 0.4,
            margin: 0.01,
            z_tol: 1e-6,
            drills: &drills,
            band: 0.0,
            body: Some(BarBody { head_d: 10.0, shank_d: 10.0, flute_len: 20.0, shank_len: 40.0 }),
            ahead: &[],
            outward: None,
        };
        let mut warns = Vec::new();
        judge_axial_rapids(&params, &path, &cx, &mut PriorCell::new(None), &mut warns);
        warns
    };
    assert_eq!(judge(0.0), vec![Warning::RapidInStock { at: 0 }], "the body on the far side of the axis went unnamed");
    assert!(judge(4.8).is_empty(), "a centred bar (nose r 5 - 0.4/2) in the Ø11 pilot is clear");
}

/// The B4 bar's one axial rapid at nose radius `r`, judged in a part
/// with a finished bore `wall` and a pilot of radius `pilot` ahead.
fn stepped_bar(r: f64, wall: f64, pilot: f64) -> Vec<Warning> {
    let params = OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: crate::operation::Height::fixed(60.0),
        top_z: crate::operation::Height::fixed(40.0),
        bottom_z: crate::operation::Height::fixed(-10.0),
        stepdown: 0.8,
        stock_to_leave: 0.25,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    let (part, stock) = (tube(wall, 12.0, -60.0, 50.0), cylinder(12.0, -60.0, 50.0));
    let p = Point3::new;
    let drills = [DrillAhead {
        radius: pilot,
        cone: Some(0.6),
        moves: vec![mv(MoveKind::Feed, p(0.0, 0.0, 50.0), p(0.0, 0.0, -16.0))],
    }];
    let path = Toolpath {
        moves: vec![mv(MoveKind::Rapid, p(r, 0.0, 60.0), p(r, 0.0, -10.0))],
        cycles: Vec::new(),
        notes: Vec::new(),
        comps: Vec::new(),
        threads: Vec::new(),
        probes: Vec::new(),
        breaks: Vec::new(),
        plasma: None,
    };
    let cx = Meridian {
        part: &part,
        stock: Some(&stock),
        nose: 0.4,
        margin: 0.01,
        z_tol: 1e-6,
        drills: &drills,
        band: 0.0,
        body: Some(BarBody { head_d: 8.0, shank_d: 12.0, flute_len: 20.0, shank_len: 40.0 }),
            ahead: &[],
            outward: None,
    };
    let mut warns = Vec::new();
    judge_axial_rapids(&params, &path, &cx, &mut PriorCell::new(None), &mut warns);
    warns
}

#[test]
fn a_bar_whose_shank_outgrows_its_head_is_judged_by_its_shank() {
    // Review B4: head Ø8, shank Ø12, nose 0.4; a Ø14 finished bore, a
    // Ø13 pilot ahead. The insert sits at the head's edge; the shank,
    // read both ways a record can mean it (edge on the insert's, or
    // axis on the head's), spans 12 toward the axis and 2 past the
    // nose. Centred (the kernel's line, nose r 5) the bar spans r -7..7:
    // through the r 6.5 pilot - convicted, it cannot fit. The old line
    // (max(d)/2 - nose/2 = 5.8) is convicted too.
    let centred = crate::turning::centred_nose(8.0, 12.0, 0.4);
    assert!((centred - 5.0).abs() < 1e-12, "the kernel centres the whole bar: {centred}");
    let at0 = vec![Warning::RapidInStock { at: 0 }];
    assert_eq!(stepped_bar(centred, 7.0, 6.5), at0, "a 14 mm bar section does not pass a 13 mm pilot");
    assert_eq!(stepped_bar(5.8, 7.0, 6.5), at0, "the old max-diameter line drives the shank into the pilot wall");
    // Centred in a pilot that fits the bar (Ø14.8 under a Ø15 bore) it is clear.
    assert!(stepped_bar(centred, 7.5, 7.4).is_empty(), "the centred bar in a pilot it fits");
}

#[test]
fn a_banded_rapid_just_under_an_earlier_face_cut_is_convicted() {
    // Review B2: a face op ahead takes the r 10 bar's end from Z 16 down
    // to 15.0 (one pass at 15.8, nose 0.8). A 3 mm blade (band 1.5)
    // then rapids down at r 8 to a centre at Z 16: its band reaches Z
    // 14.5, half a millimetre into the bar under the faced end. The
    // ahead kerf is the face's own reach - widening it by THIS blade's
    // band would read that half millimetre as air.
    let params = OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: crate::operation::Height::fixed(25.0),
        top_z: crate::operation::Height::fixed(10.0),
        bottom_z: crate::operation::Height::fixed(-20.0),
        stepdown: 0.8,
        stock_to_leave: 0.25,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    let p = Point3::new;
    let (part, stock) = (cylinder(6.0, -60.0, 10.0), cylinder(10.0, -60.0, 16.0));
    let ahead = [crate::rest::TurnedAhead { nose: 0.8, moves: vec![mv(MoveKind::Feed, p(11.0, 0.0, 15.8), p(0.0, 0.0, 15.8))] }];
    let judge = |c: f64| {
        let path = Toolpath {
            moves: vec![mv(MoveKind::Rapid, p(8.0, 0.0, 25.0), p(8.0, 0.0, c))],
            cycles: Vec::new(),
            notes: Vec::new(),
            comps: Vec::new(),
            threads: Vec::new(),
            probes: Vec::new(),
            breaks: Vec::new(),
            plasma: None,
        };
        let cx = Meridian {
            part: &part,
            stock: Some(&stock),
            nose: 0.0,
            margin: 0.01,
            z_tol: 1e-6,
            drills: &[],
            band: 1.5,
            body: None,
            ahead: &ahead,
            outward: None,
        };
        let mut warns = Vec::new();
        judge_axial_rapids(&params, &path, &cx, &mut PriorCell::new(None), &mut warns);
        warns
    };
    assert_eq!(judge(16.0), vec![Warning::RapidInStock { at: 0 }], "the band under the faced end went unnamed");
    assert!(judge(16.6).is_empty(), "the band clear of the faced end is air");
}

#[test]
fn a_pilot_removes_its_cylinder_and_its_point_cone_only() {
    let p = Pilot { lo: -10.0, hi: 20.0, r: 2.0, cot: Some(0.6) };
    assert!(p.removed(0.0, 1.9));
    assert!(!p.removed(0.0, 2.1));
    assert!(!p.removed(-9.0, 1.9), "the point's cone is not a flat floor");
    assert!(!p.removed(-10.5, 0.0));
}

/// #323: preceding axial feeds clear their nose sweep, not the uncut bar beyond the pass end.
#[test]
fn a_radial_exit_inside_a_longer_bar_is_refused_unless_it_is_a_feed() {
    let p = Point3::new;
    let path = |exit| vec![
        mv(MoveKind::Feed, p(11.8, 0.0, -16.0), p(8.0, 0.0, -16.0)),
        mv(MoveKind::Feed, p(8.0, 0.0, -16.0), p(8.0, 0.0, 16.0)),
        mv(exit, p(8.0, 0.0, 16.0), p(11.8, 0.0, 16.0)),
    ];
    assert_eq!(convicted_in(path(MoveKind::Rapid), 24.0), vec![Warning::RapidInStock { at: 2 }]);
    assert!(convicted_in(path(MoveKind::Feed), 24.0).is_empty());
}
#[test]
fn the_margin_generation_hands_the_law_shrinks_with_a_coarse_models_excess() {
    // #904: `generate` hands the meridian laws `tightened(GOUGE_TOL,
    // excess)`. The margin excuses a graze twice over - the nose disc
    // shrinks by it and so does the bar's skin - so an axial rapid
    // beside the r 10 bar is convicted from further out on a coarse
    // model, by twice the excess.
    use crate::operation::fidelity_law::tightened;
    use crate::operation::material::GOUGE_TOL;
    let p = Point3::new;
    let params = OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: crate::operation::Height::fixed(25.0),
        top_z: crate::operation::Height::fixed(16.0),
        bottom_z: crate::operation::Height::fixed(-16.0),
        stepdown: 0.8,
        stock_to_leave: 0.25,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    let (part, stock) = (cylinder(6.0, -16.0, 16.0), cylinder(10.0, -16.0, 16.0));
    let hit = |r: f64, excess: f64| {
        let path = Toolpath { moves: vec![mv(MoveKind::Rapid, p(r, 0.0, 25.0), p(r, 0.0, 5.0))], cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), threads: Vec::new(), probes: Vec::new(), breaks: Vec::new(), plasma: None };
        let margin = tightened(GOUGE_TOL, excess);
        let cx = Meridian { part: &part, stock: Some(&stock), nose: 0.8, margin, z_tol: 1e-6, drills: &[], band: 0.0, body: None, ahead: &[], outward: None };
        let mut warns = Vec::new();
        judge_axial_rapids(&params, &path, &cx, &mut PriorCell::new(None), &mut warns);
        warns == vec![Warning::RapidInStock { at: 0 }]
    };
    // The smallest radius the law leaves a rapid alone at.
    let clear_from = |excess: f64| {
        let (mut lo, mut hi) = (10.5f64, 11.0f64);
        assert!(hit(lo, excess) && !hit(hi, excess));
        for _ in 0..30 {
            let mid = 0.5 * (lo + hi);
            if hit(mid, excess) { lo = mid } else { hi = mid }
        }
        hi
    };
    assert_eq!(tightened(GOUGE_TOL, 0.0), GOUGE_TOL);
    assert!((tightened(GOUGE_TOL, 0.007) - 0.003).abs() < 1e-12);
    assert_eq!(tightened(GOUGE_TOL, 0.02), GOUGE_TOL, "past the bound: untightened, the gate refuses");
    let (full, coarse) = (clear_from(0.0), clear_from(0.007));
    assert!((coarse - full - 2.0 * 0.007).abs() < 1e-6, "stricter by twice the excess: {full} -> {coarse}");
    // Between the two: silent today, a rapid in stock on the coarse model.
    let between = 0.5 * (full + coarse);
    assert!(!hit(between, 0.0) && hit(between, 0.007));
    // Past the bound the law reads exactly as at full fidelity.
    assert!((clear_from(0.02) - full).abs() < 1e-9);
}
