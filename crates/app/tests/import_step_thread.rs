//! #1602: a thread flank exported as one rational B-spline strip - degree
//! 1 across, degree 2 along in 36-degree circular arcs, climbing one pitch
//! a turn - imports at full fidelity in a bounded facet count, with no
//! triangle cutting across the turns.
//!
//! Before #1602 the strip's four-turn fixture below meshed into 523 K facets
//! (8 s in release; the 14-turn faces of the user's parts held 2 M facets
//! each and took a minute), and once that runaway was bounded the mesher's
//! parameter plane - 350 times too short across the strip - joined the
//! strip's edges with triangles a quarter of it long.
//!
//! Fixture: generated here, our own geometry - the rational quadratic
//! circular arc of Piegl & Tiller, "The NURBS Book" (2nd ed., sec. 7.2),
//! with z linear in the angle, written as ISO 10303-21 entities (one
//! `ADVANCED_FACE` in an `OPEN_SHELL`, two helical `B_SPLINE_CURVE` rails
//! and two run-out curves crossing the strip on the bias, as polylines of
//! points on the face). No user file.
//!
//! Gate map (M4.4): covers app::geometry::import::step,
//! app::geometry::import::step_guard, app::geometry::import::step_division,
//! app::geometry::import::step_stretch, app::geometry::import::step_nearest.

use std::f64::consts::TAU;
use std::fmt::Write as _;

use camcraft_app::geometry::import::step;
use camcraft_app::geometry::Point3;

const ARC_DEG: f64 = 36.0;
const PITCH: f64 = 1.95;
/// Rail at u = 0: radius, height at angle 0. Rail at u = 1 likewise.
const OUTER: (f64, f64) = (6.35, 0.0);
const INNER: (f64, f64) = (5.08, -0.75);

/// A real in STEP's exponent form.
fn real(x: f64) -> String {
    format!("{x:.17E}")
}

/// One rail's control points and weights.
fn rail((r, z0): (f64, f64), turns: usize) -> (Vec<[f64; 3]>, Vec<f64>) {
    let arcs = turns * (360.0 / ARC_DEG) as usize;
    let w = (ARC_DEG / 2.0).to_radians().cos();
    let z = |a: f64| z0 + PITCH * a / TAU;
    let at = |rad: f64, a: f64| [rad * a.cos(), rad * a.sin(), z(a)];
    let (mut pts, mut ws) = (vec![at(r, 0.0)], vec![1.0]);
    for k in 0..arcs {
        let mid = (k as f64 * ARC_DEG + ARC_DEG / 2.0).to_radians();
        let end = ((k + 1) as f64 * ARC_DEG).to_radians();
        pts.extend([at(r / w, mid), at(r, end)]);
        ws.extend([w, 1.0]);
    }
    (pts, ws)
}

/// The rail through `pts`/`ws` at `v` in [0, 1]: each doubled-knot span is
/// one rational quadratic Bezier arc.
fn on_rail(pts: &[[f64; 3]], ws: &[f64], v: f64) -> [f64; 3] {
    let arcs = (pts.len() - 1) / 2;
    let s = v.clamp(0.0, 1.0) * arcs as f64;
    let k = (s.floor() as usize).min(arcs - 1);
    let t = s - k as f64;
    let b = [(1.0 - t) * (1.0 - t), 2.0 * t * (1.0 - t), t * t];
    let (mut num, mut den) = ([0.0; 3], 0.0);
    for i in 0..3 {
        let wb = ws[2 * k + i] * b[i];
        for (n, c) in num.iter_mut().zip(pts[2 * k + i]) {
            *n += wb * c;
        }
        den += wb;
    }
    num.map(|n| n / den)
}

/// Where the face is trimmed: from (u, v) = (0, 0) across to (1, CAP), and
/// from (1, 1) back to (0, 1 - CAP) - a thread's run-out, crossing the
/// strip on the bias, so the trimming curves carry points of their own.
/// A whole number of arcs for every turn count, so the trim sits on knots.
const CAP: f64 = 0.1;
/// Points on each trimming curve.
const CAP_POINTS: usize = 64;

fn trimmed(u: f64, v: f64) -> bool {
    v >= CAP * u && v <= 1.0 - CAP + CAP * u
}

/// The flank as a STEP file in millimetres.
fn flank_step(turns: usize) -> String {
    let arcs = turns * (360.0 / ARC_DEG) as usize;
    let knots: Vec<String> = (0..=arcs).map(|i| real(i as f64 / arcs as f64)).collect();
    let mults: Vec<String> =
        (0..=arcs).map(|i| if i == 0 || i == arcs { "3" } else { "2" }.to_string()).collect();
    let (knots, mults) = (knots.join(", "), mults.join(", "));
    let (outer, ws) = rail(OUTER, turns);
    let (inner, _) = rail(INNER, turns);
    let weights: Vec<String> = ws.iter().map(|w| real(*w)).collect();
    let weights = weights.join(", ");
    // The rows share their weights, so the surface is linear across u.
    let at = |u: f64, v: f64| {
        let (a, b) = (on_rail(&outer, &ws, v), on_rail(&inner, &ws, v));
        [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * u)
    };
    let cap = |from: (f64, f64), to: (f64, f64)| -> Vec<[f64; 3]> {
        (0..=CAP_POINTS)
            .map(|i| {
                let t = i as f64 / CAP_POINTS as f64;
                at(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t)
            })
            .collect()
    };
    let cap_start = cap((0.0, 0.0), (1.0, CAP));
    let cap_end = cap((1.0, 1.0), (0.0, 1.0 - CAP));

    let mut out = String::new();
    let mut id = 100;
    let mut ent = |out: &mut String, body: String| -> String {
        id += 1;
        let _ = writeln!(out, "#{id} = {body};");
        format!("#{id}")
    };
    let mut point = |out: &mut String, p: [f64; 3]| -> String {
        let body = format!("CARTESIAN_POINT('', ({}, {}, {}))", real(p[0]), real(p[1]), real(p[2]));
        ent(out, body)
    };
    // The points first, so `ent` is free for the rest.
    let c_out: Vec<String> = outer.iter().map(|p| point(&mut out, *p)).collect();
    let c_in: Vec<String> = inner.iter().map(|p| point(&mut out, *p)).collect();
    let p_start: Vec<String> = cap_start.iter().map(|p| point(&mut out, *p)).collect();
    let p_end: Vec<String> = cap_end.iter().map(|p| point(&mut out, *p)).collect();
    // Corners: (0, 0), (1, CAP), (1, 1), (0, 1 - CAP).
    let corners = [cap_start[0], cap_start[CAP_POINTS], cap_end[0], cap_end[CAP_POINTS]];
    let v: Vec<String> = corners.iter().map(|p| point(&mut out, *p)).collect();
    drop(point);
    let mut e = |body: String| ent(&mut out, body);
    let surface = e(format!(
        "( BOUNDED_SURFACE() B_SPLINE_SURFACE(1, 2, (({}), ({})), .UNSPECIFIED., .F., .F., .F.) \
         B_SPLINE_SURFACE_WITH_KNOTS((2, 2), ({mults}), (0.0, 1.0), ({knots}), .UNSPECIFIED.) \
         GEOMETRIC_REPRESENTATION_ITEM() RATIONAL_B_SPLINE_SURFACE((({weights}), ({weights}))) \
         REPRESENTATION_ITEM('') SURFACE() )",
        c_out.join(", "),
        c_in.join(", ")
    ));
    // A rail runs only between its trim corners: the arcs [a0, a1). The
    // trim sits on knots (CAP times the arc count is whole), as an edge's
    // curve in an exported file ends at its vertices.
    let mut curve = |cps: &[String], (a0, a1): (usize, usize)| {
        let knots: Vec<String> = (a0..=a1).map(|i| real(i as f64 / arcs as f64)).collect();
        let mults: Vec<&str> = (a0..=a1).map(|i| if i == a0 || i == a1 { "3" } else { "2" }).collect();
        let weights: Vec<String> = ws[2 * a0..=2 * a1].iter().map(|w| real(*w)).collect();
        e(format!(
            "( BOUNDED_CURVE() B_SPLINE_CURVE(2, ({}), .UNSPECIFIED., .F., .F.) \
             B_SPLINE_CURVE_WITH_KNOTS(({}), ({}), .UNSPECIFIED.) CURVE() \
             GEOMETRIC_REPRESENTATION_ITEM() RATIONAL_B_SPLINE_CURVE(({})) REPRESENTATION_ITEM('') )",
            cps[2 * a0..=2 * a1].join(", "),
            mults.join(", "),
            knots.join(", "),
            weights.join(", ")
        ))
    };
    let cap_arcs = (CAP * arcs as f64).round() as usize;
    let h_out = curve(&c_out, (0, arcs - cap_arcs));
    let h_in = curve(&c_in, (cap_arcs, arcs));
    let mut e = |body: String| ent(&mut out, body);
    let mut polyline = |pts: &[String]| {
        let n = pts.len() - 1;
        let mults: Vec<&str> = (0..=n).map(|i| if i == 0 || i == n { "2" } else { "1" }).collect();
        let knots: Vec<String> = (0..=n).map(|i| real(i as f64 / n as f64)).collect();
        e(format!(
            "B_SPLINE_CURVE_WITH_KNOTS('', 1, ({}), .POLYLINE_FORM., .F., .F., ({}), ({}), .UNSPECIFIED.)",
            pts.join(", "),
            mults.join(", "),
            knots.join(", ")
        ))
    };
    let l_start = polyline(&p_start);
    let l_end = polyline(&p_end);
    let mut e = |body: String| ent(&mut out, body);
    let vp: Vec<String> = v.iter().map(|p| e(format!("VERTEX_POINT('', {p})"))).collect();
    let start = e(format!("EDGE_CURVE('', {}, {}, {l_start}, .T.)", vp[0], vp[1]));
    let rail_in = e(format!("EDGE_CURVE('', {}, {}, {h_in}, .T.)", vp[1], vp[2]));
    let end = e(format!("EDGE_CURVE('', {}, {}, {l_end}, .T.)", vp[2], vp[3]));
    let rail_out = e(format!("EDGE_CURVE('', {}, {}, {h_out}, .T.)", vp[0], vp[3]));
    // Counter-clockwise in (u, v): across the start, up the inner rail,
    // back across the end, down the outer rail.
    let oriented: Vec<String> = [(start, ".T."), (rail_in, ".T."), (end, ".T."), (rail_out, ".F.")]
        .into_iter()
        .map(|(edge, sense)| e(format!("ORIENTED_EDGE('', *, *, {edge}, {sense})")))
        .collect();
    let lp = e(format!("EDGE_LOOP('', ({}))", oriented.join(", ")));
    let bound = e(format!("FACE_OUTER_BOUND('', {lp}, .T.)"));
    let face = e(format!("ADVANCED_FACE('', ({bound}), {surface}, .T.)"));
    e(format!("OPEN_SHELL('', ({face}))"));
    format!(
        "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('CamCraft #1602 fixture: a thread flank'), '2;1');\n\
         FILE_NAME('', '', (''), (''), '', '', '');\nFILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\nENDSEC;\nDATA;\n\
         #11 = ( GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#15)) \
         GLOBAL_UNIT_ASSIGNED_CONTEXT((#12, #13, #14)) REPRESENTATION_CONTEXT('', '') );\n\
         #12 = ( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) );\n\
         #13 = ( NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.) );\n\
         #14 = ( NAMED_UNIT(*) SI_UNIT($,.STERADIAN.) SOLID_ANGLE_UNIT() );\n\
         #15 = UNCERTAINTY_MEASURE_WITH_UNIT(1.0E-6, #12, 'distance_accuracy_value', '');\n\
         {out}ENDSEC;\nEND-ISO-10303-21;\n"
    )
}

/// The area of the ideal flank (a helicoid between the two rails) inside
/// the trim, summed over a fine grid: the rational arcs follow it to well
/// under a micron.
fn ideal_area(turns: usize) -> f64 {
    let (nu, nt) = (64, 20_000 * turns);
    let s = |u: f64, t: f64| {
        let r = OUTER.0 + (INNER.0 - OUTER.0) * u;
        let z = OUTER.1 + (INNER.1 - OUTER.1) * u + PITCH * t / TAU;
        Point3::new(r * t.cos(), r * t.sin(), z)
    };
    let span = TAU * turns as f64;
    let mut area = 0.0;
    for i in 0..nu {
        for j in 0..nt {
            let (u0, u1) = (i as f64 / nu as f64, (i + 1) as f64 / nu as f64);
            let (t0, t1) = (span * j as f64 / nt as f64, span * (j + 1) as f64 / nt as f64);
            if !trimmed((u0 + u1) / 2.0, (j as f64 + 0.5) / nt as f64) {
                continue;
            }
            let (a, b, c) = (s(u0, t0), s(u1, t0), s(u0, t1));
            area += len((b - a).cross(c - a));
        }
    }
    area
}

/// How far `p` stands off the ideal flank along the axis: its u from its
/// radius, then its height against the flank's at that u and angle,
/// reduced to the nearest turn. The flank is ~30 degrees off the axis, so
/// this over-reads the true distance by under 15 %.
fn axial_gap(p: Point3) -> f64 {
    let r = p.x.hypot(p.y);
    let u = ((r - OUTER.0) / (INNER.0 - OUTER.0)).clamp(0.0, 1.0);
    let ideal = OUTER.1 + (INNER.1 - OUTER.1) * u + PITCH * p.y.atan2(p.x) / TAU;
    let d = (p.z - ideal).rem_euclid(PITCH);
    d.min(PITCH - d)
}

fn len(p: Point3) -> f64 {
    p.dot(p).sqrt()
}

fn edge(a: Point3, b: Point3) -> f64 {
    len(b - a)
}

#[test]
fn a_four_turn_thread_flank_meshes_at_full_fidelity_in_a_bounded_count() {
    let turns = 4;
    let text = flank_step(turns);
    let (mesh, chord) = step::parse_with_chord(text.as_bytes()).expect("the flank imports");
    let facets = &mesh.facets;
    // ~80 steps a turn at 5 um on a 6.35 mm radius, two triangles each;
    // the isotropic division made 523 K.
    assert!(facets.len() < 5_000, "{} facets for a four-turn strip", facets.len());
    assert!(step::coarse(chord).is_none(), "meshed at the promised chord, not reduced: {chord} mm");
    // No triangle crosses the turns: the longest edge a sound mesh has is
    // the strip's own diagonal (1.47 mm across, ~0.5 mm a step).
    let longest = facets.iter().map(|t| edge(t.a, t.b).max(edge(t.b, t.c)).max(edge(t.c, t.a))).fold(0.0, f64::max);
    assert!(longest < 2.0, "a triangle {longest} mm long cuts across the turns");
    let area: f64 = facets.iter().map(|t| len((t.b - t.a).cross(t.c - t.a)) / 2.0).sum();
    let ideal = ideal_area(turns);
    assert!((area - ideal).abs() < 0.01 * ideal, "mesh area {area} mm2 against the flank's {ideal} mm2");
    // Every facet lies on the flank: its centre within the chord promise
    // (and the micron the rational arcs differ from the ideal helicoid).
    let worst = facets
        .iter()
        .map(|t| axial_gap(Point3::new((t.a.x + t.b.x + t.c.x) / 3.0, (t.a.y + t.b.y + t.c.y) / 3.0, (t.a.z + t.b.z + t.c.z) / 3.0)))
        .fold(0.0, f64::max);
    assert!(worst < 0.01, "a facet stands {worst} mm off the flank");
    // No wall-clock assert (PR #1638 review item 7, the issue-1605 class):
    // the facet bound above already pins the runaway.
}
