//! #689: law (b) reads an arc move at a quarter of the gouge tolerance
//! (`ARC_READ_SHARE` in sim::verify::section) instead of at the whole
//! tolerance. The verdict's floor `r - leave - tol` is unchanged; what
//! moved is how close the reading stands to the arc the machine cuts.
//! That flips verdicts both ways, and both are pinned here against the
//! analytic distance:
//!
//! - a nominal arc round a convex wall, which the old inscribed chords
//!   (sagitta up to `tol`, toward the wall) convicted, now passes - and
//!   the true clearance shows the pass is the correct verdict;
//! - an arc that really cuts past the floor is convicted round a convex
//!   wall and inside a concave one, as before; and an arc bowing into a
//!   straight wall whose apex the old chords hid is now convicted, so the
//!   new reading is stricter there, never looser.
//!
//! Gate map: covers sim::verify (sim::verify::section, law (b)).
//!
//! Provenance: chord sagitta `r (1 - cos(step / 2))` and point-to-line
//! distance from elementary plane geometry; the tangency law is our own
//! (sim::verify::section).

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::geometry::Point3;
use camcraft_app::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

/// The gouge verdict tolerance these laws run at (`GOUGE_TOL`).
const TOL: f64 = 0.01;
/// The cutter radius of the concave and straight-wall cases.
const R: f64 = 4.0;
/// The cutter radius of the convex (island) cases.
const RC: f64 = 5.0;

fn p2(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn tool() -> ToolGeom {
    tool_r(R)
}

/// A flat cutter of radius `r`.
fn tool_r(r: f64) -> ToolGeom {
    ToolGeom {
        radius: r,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: r,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    }
}

fn ctx<'a>(region: &'a Region, tool: &'a ToolGeom) -> VerifyContext<'a> {
    VerifyContext {
        region,
        stock: None,
        model: None,
        tool,
        cutter: None,
        mode: Mode::Area,
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

/// A regular `n`-gon on radius `rad` about the origin, ccw.
fn ngon(rad: f64, n: usize) -> Vec<Point2> {
    (0..n)
        .map(|k| {
            let t = std::f64::consts::TAU * k as f64 / n as f64;
            p2(rad * t.cos(), rad * t.sin())
        })
        .collect()
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![p2(x0, y0), p2(x1, y0), p2(x1, y1), p2(x0, y1)]).expect("rect")
}

/// One planar arc feed at z -1 from angle `a0` to `a1` (radians, ccw)
/// on radius `rad` about `c`.
fn arc(c: Point2, rad: f64, a0: f64, a1: f64) -> Toolpath {
    let at = |a: f64| Point3::new(c.x + rad * a.cos(), c.y + rad * a.sin(), -1.0);
    Toolpath {
        moves: vec![Move::new(MoveKind::Feed, at(a0), at(a1), Some(500.0)).with_arc(c, true)],
        ..Toolpath::default()
    }
}

fn gouges(w: &[Warning]) -> usize {
    w.iter().filter(|w| matches!(w, Warning::Gouge { .. })).count()
}

/// The verdict floor: the cutter centre must stand at least this far off
/// the wall (no leave).
fn floor() -> f64 {
    R - TOL
}

/// Island of radius 30 (a 4096-gon: its edges stand up to 8.8 um inside
/// the circle) inside a 100 mm square pocket - a convex wall.
fn island_region() -> Region {
    let island = Loop::new(ngon(30.0, 4096)).expect("island");
    Region::new(vec![rect(-50.0, -50.0, 50.0, 50.0), island]).expect("region")
}

/// The least distance from the origin to any point of `m`'s chords at
/// `tol` - the reading the law takes of the arc at that tolerance.
fn least_radius(m: &Move, tol: f64) -> f64 {
    m.chords(tol)
        .iter()
        .map(|&(a, b)| {
            // Distance from the origin to segment a-b.
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let t = (-(a.x * dx + a.y * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
            (a.x + t * dx).hypot(a.y + t * dy)
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn a_nominal_arc_round_a_convex_wall_passes_where_the_old_chords_convicted_it() {
    // The r = 5 cutter's centre rides r = 34.998 round the r = 30
    // island: 2 um inside the nominal 35, well inside the 10 um
    // allowance. Its true clearance is 4.998 >= the 4.99 floor (the
    // island's edges stand inside its circle, so the clearance to them is
    // no less).
    let rad = 34.998;
    let t = arc(p2(0.0, 0.0), rad, 10f64.to_radians(), 100f64.to_radians());
    let wall_floor = 30.0 + RC - TOL; // the least centre radius the floor allows
    assert!(rad >= wall_floor, "the arc honours the verdict: clearance {}", rad - 30.0);
    // The old reading (inscribed chords at the whole tolerance) stood up
    // to 10 um inside the arc, toward the island: past the floor.
    let old = least_radius(&t.moves[0], TOL);
    assert!(old < wall_floor, "the old chords read the arc inside the floor: {old} vs {wall_floor}");
    // The quarter reading stays within 2.5 um of the arc: above it.
    let new = least_radius(&t.moves[0], TOL * 0.25);
    assert!(new >= wall_floor && rad - new <= TOL * 0.25 + 1e-9, "the quarter reading hugs the arc: {new}");
    // And the law's verdict follows the correct reading: silent.
    assert_eq!(verify(&t, &ctx(&island_region(), &tool_r(RC))), Vec::new(), "a nominal arc is no gouge");
}

#[test]
fn an_arc_cut_past_the_floor_round_a_convex_wall_is_convicted() {
    // 15 um into the r = 30 island past the r = 5 cutter's floor: the
    // centre on r = 34.975 (clearance 4.975 < 4.99). Convicted under the
    // quarter reading, as it was under the old one.
    let rad = 34.975;
    let t = arc(p2(0.0, 0.0), rad, 10f64.to_radians(), 100f64.to_radians());
    assert!(rad - 30.0 < RC - TOL);
    assert!(least_radius(&t.moves[0], TOL) - 30.0 < RC - TOL, "the old reading convicted it too");
    let w = verify(&t, &ctx(&island_region(), &tool_r(RC)));
    assert_eq!(w, vec![Warning::Gouge { at: 0 }]);
}

#[test]
fn an_arc_cut_past_the_floor_inside_a_concave_wall_is_convicted() {
    // A round pocket of r = 50 (a 4096-gon) walked on r = 46.015 by the
    // r = 4 cutter: 15 um past the 3.99 floor (true clearance to the
    // polygon's edges <= 50 - 46.015 = 3.985). The arc's joints lie on
    // the arc, so the old reading convicted it as well - the quarter
    // reading is at least as strict here.
    let region = Region::new(vec![Loop::new(ngon(50.0, 4096)).expect("pocket")]).expect("region");
    let rad = 46.015;
    let t = arc(p2(0.0, 0.0), rad, 10f64.to_radians(), 100f64.to_radians());
    let g = tool();
    assert!(50.0 - rad < floor());
    let w = verify(&t, &ctx(&region, &g));
    assert_eq!(w, vec![Warning::Gouge { at: 0 }]);
    // The nominal walk on r = 46 stays silent under both readings.
    let nominal = arc(p2(0.0, 0.0), 46.0, 10f64.to_radians(), 100f64.to_radians());
    assert_eq!(verify(&nominal, &ctx(&region, &g)), Vec::new());
}

#[test]
fn an_arc_bowing_past_the_floor_into_a_straight_wall_is_convicted_where_the_old_chords_hid_it() {
    // The wall y = 20 of a 40 x 20 pocket; an r = 20 arc whose apex
    // stands at y = 16.015: 5 um past the r = 4 cutter's 3.99 floor (true
    // clearance 3.985). At the whole tolerance its 5 chords put the apex
    // mid-chord, 9.6 um below the arc: the old reading saw 3.995 and
    // passed a real gouge. The quarter reading convicts it.
    let region = Region::new(vec![rect(0.0, 0.0, 40.0, 20.0)]).expect("pocket");
    let (rad, apex) = (20.0, 16.015);
    let c = p2(20.0, apex - rad);
    let half = 0.155;
    let t = arc(c, rad, std::f64::consts::FRAC_PI_2 - half, std::f64::consts::FRAC_PI_2 + half);
    let true_clearance = 20.0 - apex;
    assert!(true_clearance < floor(), "the arc really cuts past the floor: {true_clearance}");
    let highest = |tol: f64| {
        t.moves[0]
            .chords(tol)
            .iter()
            .flat_map(|&(a, b)| [a.y, b.y])
            .fold(f64::NEG_INFINITY, f64::max)
    };
    assert!(20.0 - highest(TOL) >= floor(), "the old chords hid it: read {}", 20.0 - highest(TOL));
    assert!(20.0 - highest(TOL * 0.25) < floor(), "the quarter reading sees it: {}", 20.0 - highest(TOL * 0.25));
    let w = verify(&t, &ctx(&region, &tool()));
    assert_eq!(gouges(&w), 1, "{w:?}");
    assert_eq!(w, vec![Warning::Gouge { at: 0 }]);
}

#[test]
fn the_quarter_reading_is_never_coarser_than_the_old_one_at_any_radius() {
    // `MAX_ARC_CHORDS` (1024) clamps an arc's chord count. At the quarter
    // reading (2.5 um) a near-full circle hits the clamp above r ~ 531 mm,
    // at the old reading (10 um) above r ~ 2125 mm: past those radii the
    // chords stand off the arc by more than their tolerance. The quarter
    // reading then degrades toward the old one - never past it: it takes
    // at least as many chords, and its worst sagitta is never larger.
    let sweep = 359f64.to_radians();
    for rad in [5.0, 50.0, 500.0, 531.0, 600.0, 2000.0, 2125.0, 3000.0] {
        let m = &arc(p2(0.0, 0.0), rad, 0.0, sweep).moves[0];
        let sag = |tol: f64| {
            m.chords(tol)
                .iter()
                .map(|&(a, b)| rad - ((a.x + b.x) * 0.5).hypot((a.y + b.y) * 0.5))
                .fold(0.0f64, f64::max)
        };
        let (old, new) = (m.chords(TOL).len(), m.chords(TOL * 0.25).len());
        assert!(new >= old, "r = {rad}: {new} chords at the quarter reading, {old} at the old one");
        assert!(sag(TOL * 0.25) <= sag(TOL) + 1e-12, "r = {rad}: the quarter reading is coarser");
        if rad <= 500.0 {
            assert!(sag(TOL * 0.25) <= TOL * 0.25 + 1e-9, "r = {rad}: within its 2.5 um below the clamp");
        }
    }
}

/// #689 review B1: outside `Mode::Area` law (b) reads an arc only at its
/// chord joints, and the joints at the whole tolerance and at the quarter
/// are not nested (3 chords become 5 here). A sharp island vertex on an
/// old joint falls between two new joints, so the quarter set alone passed
/// a gouge the old reading convicted. The law reads the union of both, so
/// every old conviction stands - and a vertex just outside the floor stays
/// clean, since points on the arc never read closer than the truth.
#[test]
fn boundary_mode_keeps_the_old_joints_so_a_sharp_vertex_on_one_stays_convicted() {
    // r 200 about the origin sweeping 0.048 rad, 4 mm cutter (floor 3.99).
    let (rad, sweep) = (200.0, 0.048);
    let t = arc(p2(0.0, 0.0), rad, 0.0, sweep);
    let m = &t.moves[0];
    let (old, new) = (m.chords(TOL), m.chords(TOL * 0.25));
    assert_eq!((old.len(), new.len()), (3, 5), "the case needs 3 old chords and 5 new ones");
    // The old joint at 0.016 rad, and no new joint near it.
    let a = sweep / 3.0;
    let joint = p2(rad * a.cos(), rad * a.sin());
    let near = |p: Point2| (p.x - joint.x).hypot(p.y - joint.y) < 1e-6;
    assert!(old.iter().any(|&(_, b)| near(b)), "an old joint stands at {a} rad");
    assert!(!new.iter().any(|&(x, y)| near(x) || near(y)), "no new joint stands there");
    let (ux, uy) = (a.cos(), a.sin()); // outward from the arc at the joint
    let (tx, ty) = (-uy, ux); // along the arc
    // A sharp island vertex `d` outside the old joint, its sides opening
    // away from the arc (half-angle ~14 deg), in a pocket far from the arc.
    let region_at = |d: f64| {
        let v = p2(joint.x + ux * d, joint.y + uy * d);
        let side = |s: f64| p2(v.x + ux * 20.0 + tx * 5.0 * s, v.y + uy * 20.0 + ty * 5.0 * s);
        let island = Loop::new(vec![v, side(1.0), side(-1.0)]).expect("island");
        Region::new(vec![rect(150.0, -50.0, 260.0, 60.0), island]).expect("region")
    };
    let g = tool();
    let boundary = |region: &Region| {
        let mut c = ctx(region, &g);
        c.mode = Mode::Boundary;
        verify(&t, &c)
    };
    // 50, 40 and 10 um past the floor: convicted, as the old reading did.
    for d in [3.94, 3.95, 3.98] {
        assert!(d < floor());
        assert_eq!(boundary(&region_at(d)), vec![Warning::Gouge { at: 0 }], "clearance {d}");
    }
    // Just outside the floor: clean.
    for d in [3.995, 4.0] {
        assert!(d >= floor());
        assert_eq!(boundary(&region_at(d)), Vec::new(), "clearance {d}");
    }
}

/// #864 with #689 review B1: a model coarser than the importer promises
/// tightens the FLOOR (`model_excess`), never the reading - an arc is
/// still read at the union of the old and the quarter joints, at the
/// plain `gouge_tol`. So a sharp vertex on an OLD joint (which no quarter
/// joint stands near) is judged against the tightened floor: inside it is
/// convicted, though the untightened floor passes it, and outside it stays
/// clean.
#[test]
fn boundary_mode_judges_the_union_joints_against_the_tightened_floor() {
    // The B1 case: r 200 sweeping 0.048 rad, the old joint at 0.016 rad.
    let (rad, sweep) = (200.0, 0.048);
    let t = arc(p2(0.0, 0.0), rad, 0.0, sweep);
    let a = sweep / 3.0;
    let joint = p2(rad * a.cos(), rad * a.sin());
    let (ux, uy) = (a.cos(), a.sin());
    let (tx, ty) = (-uy, ux);
    let region_at = |d: f64| {
        let v = p2(joint.x + ux * d, joint.y + uy * d);
        let side = |s: f64| p2(v.x + ux * 20.0 + tx * 5.0 * s, v.y + uy * 20.0 + ty * 5.0 * s);
        let island = Loop::new(vec![v, side(1.0), side(-1.0)]).expect("island");
        Region::new(vec![rect(150.0, -50.0, 260.0, 60.0), island]).expect("region")
    };
    let g = tool();
    let boundary = |region: &Region, excess: f64| {
        let mut c = ctx(region, &g);
        c.mode = Mode::Boundary;
        c.model_excess = excess;
        verify(&t, &c)
    };
    // 6 um of excess leaves 4 um of tolerance: the floor rises from 3.990
    // to 3.996 mm.
    let excess = 0.006;
    let tight = R - (TOL - excess);
    assert!(floor() < tight && tight < R);
    // Between the two floors: clean at full fidelity, convicted with the
    // excess - at the old joint, so the union reading is what found it.
    for d in [3.991, 3.993, 3.995] {
        assert!(d >= floor() && d < tight);
        assert_eq!(boundary(&region_at(d), 0.0), Vec::new(), "full fidelity, clearance {d}");
        assert_eq!(boundary(&region_at(d), excess), vec![Warning::Gouge { at: 0 }], "coarse model, clearance {d}");
    }
    // Inside the plain floor: convicted either way (the #689 verdict).
    assert_eq!(boundary(&region_at(3.98), 0.0), vec![Warning::Gouge { at: 0 }]);
    assert_eq!(boundary(&region_at(3.98), excess), vec![Warning::Gouge { at: 0 }]);
    // Outside the tightened floor: clean either way - the tightening is
    // not a blanket conviction, and the reading added no false gouge.
    for d in [3.997, 4.0] {
        assert!(d >= tight);
        assert_eq!(boundary(&region_at(d), 0.0), Vec::new(), "clearance {d}");
        assert_eq!(boundary(&region_at(d), excess), Vec::new(), "coarse model, clearance {d}");
    }
    // An excess that uses the whole tolerance reads untightened (the
    // program gate refuses instead): the full-fidelity verdicts exactly.
    for d in [3.98, 3.993, 4.0] {
        assert_eq!(boundary(&region_at(d), 0.02), boundary(&region_at(d), 0.0), "clearance {d}");
    }
}
