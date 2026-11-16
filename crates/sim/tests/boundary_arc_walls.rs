//! #905: outside `Mode::Area` law (b) read an arc move only at its chord
//! joints, so a sharp wall corner between two joints was never measured
//! against the cutter. In Boundary mode an XY arc now answers on its true
//! circle to the walls (`sim::verify::arc_wall`), so the verdict no longer
//! depends on where the joints land. Area mode reads its chords whole and
//! is unchanged.
//!
//! Gate map: covers sim::verify (sim::verify::section, sim::verify::arc_wall, law (b)).
//!
//! Provenance: point-to-circle distance along the radius, from elementary
//! plane geometry; the tangency law is our own (sim::verify::section).

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::geometry::Point3;
use camcraft_sim::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

/// The gouge verdict tolerance (`GOUGE_TOL`).
const TOL: f64 = 0.01;
/// The cutter radius.
const R: f64 = 4.0;
/// The arc of the issue's evidence: r 200 about the origin, 0.048 rad.
const RAD: f64 = 200.0;
const SWEEP: f64 = 0.048;
/// Midway between the joints near it: the fine set (5 chords) stands at
/// 0.0192 and 0.0288 rad, the whole-tolerance set (3 chords) at 0.016
/// and 0.032 - each about 1 mm along the arc from here.
const MID: f64 = 0.024;

fn p2(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn floor() -> f64 {
    R - TOL
}

fn tool() -> ToolGeom {
    ToolGeom {
        radius: R,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: R,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    }
}

fn ctx<'a>(region: &'a Region, tool: &'a ToolGeom, mode: Mode) -> VerifyContext<'a> {
    VerifyContext {
        region,
        stock: None,
        model: None,
        tool,
        cutter: None,
        mode,
        stock_top_z: 0.0,
        clearance_z: 5.0,
        stock_to_leave: 0.0,
        gouge_tol: TOL,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![p2(x0, y0), p2(x1, y0), p2(x1, y1), p2(x0, y1)]).expect("rect")
}

/// The arc as one feed at z -1, ccw from 0 to `SWEEP`, or the same arc
/// cut clockwise from `SWEEP` back to 0.
fn arc(ccw: bool) -> Toolpath {
    let at = |a: f64| Point3::new(RAD * a.cos(), RAD * a.sin(), -1.0);
    let (a0, a1) = if ccw { (0.0, SWEEP) } else { (SWEEP, 0.0) };
    Toolpath {
        moves: vec![Move::new(MoveKind::Feed, at(a0), at(a1), Some(500.0)).with_arc(p2(0.0, 0.0), ccw)],
        ..Toolpath::default()
    }
}

/// A sharp island vertex `d` off the arc at `MID`: outside the circle
/// (`outward`) or inside it, its sides opening away from the arc
/// (half-angle ~14 deg), in a pocket that stands far from the arc.
fn region_at(d: f64, outward: bool) -> (Region, Point2) {
    let s = if outward { 1.0 } else { -1.0 };
    let (ux, uy) = (s * MID.cos(), s * MID.sin()); // away from the arc
    let (tx, ty) = (-MID.sin(), MID.cos()); // along it
    let v = p2(RAD * MID.cos() + ux * d, RAD * MID.sin() + uy * d);
    let side = |k: f64| p2(v.x + ux * 20.0 + tx * 5.0 * k, v.y + uy * 20.0 + ty * 5.0 * k);
    let island = Loop::new(vec![v, side(1.0), side(-1.0)]).expect("island");
    (Region::new(vec![rect(150.0, -50.0, 260.0, 60.0), island]).expect("region"), v)
}

/// The nearest any joint of the arc comes to `v`, at the whole tolerance
/// and at the quarter: what the joint reading alone measured.
fn joint_clearance(m: &Move, v: Point2) -> f64 {
    [TOL, TOL * 0.25]
        .into_iter()
        .flat_map(|t| m.chords(t))
        .flat_map(|(a, b)| [a, b])
        .map(|p| (p.x - v.x).hypot(p.y - v.y))
        .fold(f64::INFINITY, f64::min)
}

fn boundary(t: &Toolpath, region: &Region) -> Vec<Warning> {
    let g = tool();
    verify(t, &ctx(region, &g, Mode::Boundary))
}

#[test]
fn a_corner_between_an_arcs_joints_past_the_floor_is_a_gouge() {
    for outward in [true, false] {
        for ccw in [true, false] {
            let t = arc(ccw);
            // 90 um past the floor: the joints stand ~1 mm along the arc
            // and read the vertex clear, so the joint reading passed it.
            let (region, v) = region_at(3.90, outward);
            let seen = joint_clearance(&t.moves[0], v);
            assert!(seen >= floor(), "the joints read it clear: {seen} (outward {outward})");
            assert_eq!(boundary(&t, &region), vec![Warning::Gouge { at: 0 }], "outward {outward}, ccw {ccw}");
            // Deeper and shallower past the floor: convicted too.
            for d in [3.5, 3.8, 3.98] {
                let (region, _) = region_at(d, outward);
                assert_eq!(boundary(&t, &region), vec![Warning::Gouge { at: 0 }], "d {d}, outward {outward}, ccw {ccw}");
            }
        }
    }
}

#[test]
fn the_same_corner_just_inside_the_floor_stays_clean() {
    for outward in [true, false] {
        for ccw in [true, false] {
            let t = arc(ccw);
            for d in [3.995, 4.0, 4.5] {
                assert!(d >= floor());
                let (region, _) = region_at(d, outward);
                assert_eq!(boundary(&t, &region), Vec::new(), "d {d}, outward {outward}, ccw {ccw}");
            }
        }
    }
}

#[test]
fn area_mode_verdicts_on_the_corner_are_unchanged() {
    // Area mode reads the arc's chords along their whole length: it
    // convicted this corner before #905 and still does, and passes it
    // clear of the floor.
    let g = tool();
    let t = arc(true);
    let (past, _) = region_at(3.90, true);
    assert_eq!(verify(&t, &ctx(&past, &g, Mode::Area)), vec![Warning::Gouge { at: 0 }]);
    let (clear, _) = region_at(4.0, true);
    assert_eq!(verify(&t, &ctx(&clear, &g, Mode::Area)), Vec::new());
}

/// #864 with #905: a model coarser than the importer promises tightens
/// every part-facing floor of law (b) - the whole-arc wall reading too,
/// not only the joints. A corner BETWEEN the joints, where only the
/// whole-arc reading measures, is convicted between the plain and the
/// tightened floor on such a model; at full fidelity, and outside the
/// tightened floor, the verdicts above stand unchanged.
#[test]
fn the_whole_arc_reading_answers_to_a_coarse_models_tightened_floor() {
    let g = tool();
    let judge = |t: &Toolpath, region: &Region, excess: f64| {
        let mut c = ctx(region, &g, Mode::Boundary);
        c.model_excess = excess;
        verify(t, &c)
    };
    // 7 um of excess leaves 3 um of tolerance: the floor rises from
    // 3.990 to 3.997 mm. The whole-arc reading pads the arc's radius by
    // the centre's 1 um lattice (it never reads farther than the truth),
    // so the clearances below stand 3 um or more off whichever floor they
    // must read clear of.
    let excess = 0.007;
    let tight = R - (TOL - excess);
    assert!(floor() < tight && tight < R);
    for outward in [true, false] {
        for ccw in [true, false] {
            let t = arc(ccw);
            // Between the two floors.
            for d in [3.994, 3.995, 3.996] {
                assert!(d >= floor() + 0.003 && d < tight);
                let (region, v) = region_at(d, outward);
                // No joint comes near: only the whole-arc reading sees it.
                assert!(joint_clearance(&t.moves[0], v) >= tight, "the joints read it clear of the tightened floor too");
                assert_eq!(judge(&t, &region, 0.0), Vec::new(), "full fidelity, d {d}, outward {outward}, ccw {ccw}");
                assert_eq!(judge(&t, &region, excess), vec![Warning::Gouge { at: 0 }], "coarse model, d {d}, outward {outward}, ccw {ccw}");
                // An excess that uses the whole tolerance reads untightened
                // (the program gate refuses instead).
                assert_eq!(judge(&t, &region, 0.02), Vec::new(), "past the tolerance, d {d}");
            }
            // Outside the tightened floor: clean either way.
            for d in [4.001, 4.01, 4.5] {
                assert!(d >= tight + 0.003);
                let (region, _) = region_at(d, outward);
                assert_eq!(judge(&t, &region, excess), Vec::new(), "coarse model, d {d}, outward {outward}, ccw {ccw}");
            }
            // Inside the plain floor: a gouge either way.
            let (region, _) = region_at(3.98, outward);
            assert_eq!(judge(&t, &region, excess), judge(&t, &region, 0.0));
            assert_eq!(judge(&t, &region, excess), vec![Warning::Gouge { at: 0 }]);
        }
    }
}
