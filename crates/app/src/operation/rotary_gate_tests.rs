//! Unit pins of the rotary kinds' letter gate and the C stamp
//! (M13.5 B, issue #33, #2828 the radial unwind): which machine
//! rotary answers the fitted line, the spindle C's stamp and its
//! whole-turn unwind - over the axial plane (Rotary Contour) or on
//! the retract cylinder (Rotary Flow) - and every refusal by name.
//! Our own fixtures; no vendor material consulted.

use super::*;

fn rot(name: &str, axis: [f64; 3]) -> AxisLimits {
    AxisLimits {
        name: name.into(),
        rotary: true,
        axis: Some(axis),
        ..Default::default()
    }
}

/// One and a half turns of a radial helix at r 10: C reads -540.
fn helix() -> crate::cl::Toolpath {
    use crate::cl::{Move, MoveKind};
    use crate::geometry::Point3;
    let at = |k: usize| {
        let th = (k as f64 * 30.0).to_radians();
        Point3::new(10.0 * th.cos(), 10.0 * th.sin(), -(k as f64) / 6.0)
    };
    let moves = (0..18)
        .map(|k| Move::new(MoveKind::Feed, at(k), at(k + 1), Some(400.0)).with_axis([at(k + 1).x / 10.0, at(k + 1).y / 10.0, 0.0]))
        .collect();
    crate::cl::Toolpath::new(moves)
}

fn spindle() -> Option<crate::multiaxis::axis::Axis> {
    use crate::geometry::Point3;
    Some(crate::multiaxis::axis::Axis { origin: Point3::new(0.0, 0.0, 0.0), dir: Point3::new(0.0, 0.0, 1.0) })
}

fn c_travel_machine(lo: f64, hi: f64) -> (String, Vec<AxisLimits>) {
    let mut c = rot("C", [0.0, 0.0, 1.0]);
    c.travel = Some((lo, hi));
    ("MT".to_owned(), vec![c])
}

#[test]
fn parallel_and_offset_are_refused_by_name_where_c_would_need_a_rewind() {
    let m = c_travel_machine(-360.0, 360.0);
    for kind in ["Rotary Parallel", "Rotary Offset"] {
        let e = stamp_spindle_c(helix(), spindle(), (Some(&m), Unwind::NoProof), 0.01, kind).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(e.contains(kind) && e.contains("proven for Rotary Contour over the axial plane and for Rotary Flow on the retract cylinder"), "{e}");
        // Inside the travel as stamped: posts exactly as before slice 2.
        let wide = c_travel_machine(-720.0, 720.0);
        let t = stamp_spindle_c(helix(), spindle(), (Some(&wide), Unwind::NoProof), 0.01, kind);
        assert!(t.is_ok_and(|t| t.moves.iter().all(|m| m.rotary.is_some_and(|c| (-720.0..=0.0).contains(&c)))), "{kind}");
    }
    // The contour unwinds: its 540 deg opening stretch is shifted a
    // whole turn into the travel (the section opens there).
    let t = stamp_spindle_c(helix(), spindle(), (Some(&m), Unwind::Axial), 0.01, "Rotary Contour");
    assert!(t.is_ok_and(|t| t.moves.iter().all(|m| m.rotary.is_some_and(|c| (-360.0..=360.0).contains(&c)))));
}

#[test]
fn a_gate_refusal_hands_back_the_world_path_it_judged() {
    // #1015: the rotary kernel's path is world (the frame the post
    // reads), so the refused path rides as the ghost, move for move.
    let m = c_travel_machine(-360.0, 360.0);
    let e = stamp_spindle_c(helix(), spindle(), (Some(&m), Unwind::NoProof), 0.01, "Rotary Parallel").err();
    let ghost = e.as_ref().and_then(|e| e.ghost()).map(|g| g.moves().to_vec());
    assert_eq!(ghost, Some(helix().moves), "{e:?}");
    // A tilted line refused before any stamp: the same path rides.
    let leaning = crate::multiaxis::axis::Axis { dir: crate::geometry::Point3::new(1.0, 0.0, 0.0), ..spindle().expect("axis") };
    let e = stamp_spindle_c(helix(), Some(leaning), (Some(&m), Unwind::NoProof), 0.01, "Rotary Offset").err();
    assert_eq!(e.as_ref().and_then(|e| e.ghost()).map(|g| g.len()), Some(helix().moves.len()), "{e:?}");
}

/// Rotary Contour unwinds over the axial plane only: a path whose
/// only clearance is a retract cylinder is refused by name through
/// the gate, never rewound at a radius no shipped kind has proven.
#[test]
fn the_contour_takes_no_radial_link() {
    let path = radial_ring_path();
    let m = c_travel_machine(-360.0, 360.0);
    let e = stamp_spindle_c(path, spindle(), (Some(&m), Unwind::Axial), 0.01, "Rotary Contour").err().map(|e| e.to_string()).unwrap_or_default();
    assert!(e.contains("Rotary Contour") && e.contains("radial links are not taken for this kind"), "{e}");
}

/// #2828: the mirror of the contour's refusal - Rotary Flow links
/// ON the retract cylinder, so the same radial-only path unwinds
/// at its radius, every C inside the travel, the chain held (the
/// bench states the C rate the rewind posts under, #906).
#[test]
fn the_flow_unwinds_on_the_retract_cylinder() {
    let path = radial_ring_path();
    let mut m = c_travel_machine(-360.0, 360.0);
    for a in &mut m.1 {
        a.feed = Some(6000.0);
    }
    let t = stamp_spindle_c(path, spindle(), (Some(&m), Unwind::Radial), 0.01, "Rotary Flow").unwrap_or_else(|e| panic!("{e}"));
    assert!(t.moves.iter().all(|m| m.rotary.is_some_and(|c| (-360.0..=360.0).contains(&c))));
    for w in t.moves.windows(2) {
        assert_eq!(w[0].to, w[1].from, "the chain holds through the rewinds");
    }
    // The rewinds are orbit chords on the retract cylinder (r 20).
    let orbit: Vec<&crate::cl::Move> = t
        .moves
        .iter()
        .filter(|m| m.kind == crate::cl::MoveKind::Rapid && m.from.z == m.to.z && m.from.x.hypot(m.from.y) > 15.0 && m.to.x.hypot(m.to.y) > 15.0)
        .collect();
    assert!(!orbit.is_empty(), "the rewinds ride as orbit rapids on the cylinder");
}

/// The radial-link fixture both cylinder tests read: three rings
/// at r 10 joined by RADIAL links out to r 20, down a ring and
/// back - no rapid rises above the cut, so only the radius can be
/// a clearance.
fn radial_ring_path() -> crate::cl::Toolpath {
    use crate::cl::{Move, MoveKind};
    use crate::geometry::Point3;
    let at = |k: usize, z: f64| {
        let th = (k as f64 * 30.0).to_radians();
        Point3::new(10.0 * th.cos(), 10.0 * th.sin(), z)
    };
    let radial = |p: Point3| {
        let r = p.x.hypot(p.y);
        [p.x / r, p.y / r, 0.0]
    };
    let mut moves: Vec<Move> = Vec::new();
    for ring in 0..3 {
        let z = -(ring as f64);
        if let Some(end) = moves.last().map(|m| m.to) {
            let (a, b) = (Point3::new(20.0, 0.0, end.z), Point3::new(20.0, 0.0, z));
            for (p, q) in [(end, a), (a, b), (b, at(0, z))] {
                moves.push(Move::new(MoveKind::Rapid, p, q, None).with_axis(radial(q)));
            }
        }
        for k in 0..12 {
            let to = if k == 11 { at(0, z) } else { at(k + 1, z) };
            moves.push(Move::new(MoveKind::Feed, at(k, z), to, Some(400.0)).with_axis(radial(to)));
        }
    }
    crate::cl::Toolpath::new(moves)
}

/// #906: two helices linked over the top need an orbit; with no C
/// rate stated the rewind is refused by name, with one it unwinds.
#[test]
fn a_rewind_without_a_stated_c_rate_is_refused_by_name() {
    use crate::cl::{Move, MoveKind};
    use crate::geometry::Point3;
    let mut path = helix().moves;
    let lower = |mut m: Move| {
        m.from.z -= 4.0;
        m.to.z -= 4.0;
        m
    };
    let more: Vec<Move> = helix().moves.into_iter().map(lower).collect();
    let (Some(end), Some(start)) = (path.last().map(|m| m.to), more.first().map(|m| m.from)) else {
        panic!("the helix has moves")
    };
    // Up, round the spindle line through (0, 15) - never across it - and down.
    let (up, via, over) = (Point3::new(end.x, end.y, 5.0), Point3::new(0.0, 15.0, 5.0), Point3::new(start.x, start.y, 5.0));
    for (p, q) in [(end, up), (up, via), (via, over), (over, start)] {
        path.push(Move::new(MoveKind::Rapid, p, q, None).with_axis([q.x / q.x.hypot(q.y), q.y / q.x.hypot(q.y), 0.0]));
    }
    path.extend(more);
    let path = crate::cl::Toolpath::new(path);
    let m = c_travel_machine(-360.0, 360.0);
    let e = stamp_spindle_c(path.clone(), spindle(), (Some(&m), Unwind::Axial), 0.01, "Rotary Contour").err().map(|e| e.to_string()).unwrap_or_default();
    assert!(e.contains("Rotary Contour") && e.contains("states a C travel but no C rate"), "{e}");
    let mut rated = m.clone();
    for a in &mut rated.1 {
        a.feed = Some(6000.0);
    }
    let t = stamp_spindle_c(path, spindle(), (Some(&rated), Unwind::Axial), 0.01, "Rotary Contour");
    assert!(t.is_ok_and(|t| t.moves.iter().all(|m| m.rotary.is_some_and(|c| (-360.0..=360.0).contains(&c)))));
}

#[test]
fn no_machine_checks_nothing() {
    assert_eq!(rotary_letter(None, [1.0, 0.0, 0.0], "Rotary Contour").ok(), Some(None));
}

#[test]
fn a_table_trunnion_names_its_letter_either_way_round() {
    let m = ("Bench".to_owned(), vec![rot("A", [1.0, 0.0, 0.0])]);
    assert_eq!(rotary_letter(Some(&m), [1.0, 0.0, 0.0], "k").ok(), Some(Some('A')));
    assert_eq!(rotary_letter(Some(&m), [-1.0, 0.0, 0.0], "k").ok(), Some(Some('A')));
}

#[test]
fn minus_z_c_rotary_flips_asymmetric_travel_in_gate() {
    use crate::cl::{Move, MoveKind};
    use crate::geometry::Point3;
    let q0 = Point3::new(10.0, 0.0, 0.0);
    let q1 = Point3::new(0.0, 10.0, 0.0);
    // Canonical azimuth at +Y is +90 -> C = -90 deg.
    let path = crate::cl::Toolpath::new(vec![Move::new(MoveKind::Feed, q0, q1, Some(300.0)).with_axis([0.0, 1.0, 0.0])]);
    let m_plus = ("PlusZ".to_owned(), vec![AxisLimits { name: "C".into(), rotary: true, axis: Some([0.0, 0.0, 1.0]), travel: Some((0.0, 95.0)), ..Default::default() }]);
    let e = stamp_spindle_c(path.clone(), spindle(), (Some(&m_plus), Unwind::NoProof), 0.01, "Rotary Parallel").err().map(|e| e.to_string()).unwrap_or_default();
    assert!(e.contains("past the machine's C travel"), "{e}");
    let m_minus = ("MinusZ".to_owned(), vec![AxisLimits { name: "C".into(), rotary: true, axis: Some([0.0, 0.0, -1.0]), travel: Some((0.0, 95.0)), ..Default::default() }]);
    let ok = stamp_spindle_c(path, spindle(), (Some(&m_minus), Unwind::NoProof), 0.01, "Rotary Parallel").expect("-Z flips 0..95 to -95..0");
    assert!(ok.moves.last().and_then(|m| m.rotary).is_some_and(|c| (c + 90.0).abs() < 1e-9));
}

#[test]
fn a_missing_line_refuses_by_name_and_the_spindle_answers_c() {
    let b = ("Bench".to_owned(), vec![rot("B", [0.0, 1.0, 0.0])]);
    let e = rotary_letter(Some(&b), [1.0, 0.0, 0.0], "Rotary Offset").err();
    assert!(e.is_some_and(|e| e.to_string().contains("states no rotary on that line")));
    let c = ("MT".to_owned(), vec![rot("C", [0.0, 0.0, 1.0])]);
    assert_eq!(rotary_letter(Some(&c), [0.0, 0.0, 1.0], "Rotary Contour").ok(), Some(Some('C')));
}

/// A helical rotor along world X (#2772): a 4-lobe helical sweep
/// turning as x advances - the reporter's screw, whose normals
/// share no rotation axis (the fit's own refusal).
fn screw(turns: f64) -> Vec<Triangle> {
    use crate::geometry::Point3;
    let rate = turns * std::f64::consts::TAU / 32.0;
    let (nth, nx) = (96usize, 32usize);
    let rho = |th: f64| 5.0 + 1.5 * (4.0 * th).cos();
    let pt = |th_i: usize, x: f64| {
        let th = std::f64::consts::TAU * th_i as f64 / nth as f64;
        let (s, c) = (rate * x).sin_cos();
        let r = rho(th);
        let (bs, bc) = th.sin_cos();
        Point3::new(x, r * (bc * c - bs * s), r * (bs * c + bc * s))
    };
    let mut f = Vec::new();
    for thi in 0..nth {
        let tk = (thi + 1) % nth;
        for st in 0..nx {
            let (x0, x1) = (-16.0 + 32.0 * st as f64 / nx as f64, -16.0 + 32.0 * (st + 1) as f64 / nx as f64);
            let (a1, a2, b1, b2) = (pt(thi, x0), pt(tk, x0), pt(thi, x1), pt(tk, x1));
            f.push(Triangle::new(a1, a2, b2));
            f.push(Triangle::new(a1, b2, b1));
        }
    }
    let cap = |f: &mut Vec<Triangle>, x: f64, outward: bool| {
        let c = Point3::new(x, 0.0, 0.0);
        for thi in 0..nth {
            let tk = (thi + 1) % nth;
            let (a, b) = (pt(thi, x), pt(tk, x));
            f.push(if outward { Triangle::new(c, a, b) } else { Triangle::new(c, b, a) });
        }
    };
    cap(&mut f, -16.0, false);
    cap(&mut f, 16.0, true);
    f
}

/// #2772 parts 1-2: a stated line answers the letter without a
/// fit. The screw's fit refuses (no axis of revolution), so today
/// the door answers nothing; its STATED line - the A the part is
/// clamped on - holds the machine's inventory, and the spindle's
/// C answers a stated vertical line with the axis itself.
#[test]
fn a_stated_line_answers_the_letter_without_a_fit() {
    use crate::geometry::Point3;
    let mesh = screw(2.0);
    assert!(crate::multiaxis::axis::fit(&mesh).is_err(), "the helical lobes share no rotation axis");
    let at = |d: [f64; 3]| crate::multiaxis::axis_stated::canonical(d, Point3::new(0.0, 0.0, 0.0)).expect("a direction");
    let a_bench = ("Bench".to_owned(), vec![rot("A", [1.0, 0.0, 0.0])]);
    let stated = at([1.0, 0.0, 0.0]);
    // Nothing stated: the fit refuses, the door answers nothing.
    assert_eq!(letter_gate(Some(&a_bench), None, &mesh, "Rotary Parallel").ok(), Some(None));
    // The stated line answers the machine's A, no fit asked.
    assert_eq!(letter_gate(Some(&a_bench), Some(&stated), &mesh, "Rotary Parallel").ok(), Some(None));
    // The spindle's C answers a stated vertical line with the axis.
    let c_bench = ("MT".to_owned(), vec![rot("C", [0.0, 0.0, 1.0])]);
    let z = at([0.0, 0.0, 1.0]);
    assert_eq!(letter_gate(Some(&c_bench), Some(&z), &mesh, "Rotary Contour").ok(), Some(Some(z)));
    // A stated line no rotary holds still refuses by name.
    let e = letter_gate(Some(&c_bench), Some(&stated), &mesh, "Rotary Contour").expect_err("no rotary about the stated line");
    assert!(e.to_string().contains("states no rotary on that line"), "{e}");
}
