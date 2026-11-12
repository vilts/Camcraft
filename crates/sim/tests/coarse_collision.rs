//! #904: the collision laws on a model coarser than the importer
//! promises. They judge clearance against material read off facets, and
//! a facet of a convex wall stands INSIDE the true surface by its chord
//! error - so a clearance can read that much too generous. With
//! `VerifyContext::model_excess` set, while it leaves the gouge
//! tolerance positive:
//!
//! - law (c), shank and holder reach, grows each band by the excess
//!   (`VerifyContext::band_growth`), and its mid-turn sweep (c3) with it;
//! - law (d), rapids in stock, gives the excess back out of the slack of
//!   the footprint it marches, and its posed second reading out of the
//!   depth it lets a leaning stack stand in the part.
//!
//! Past that every law reads exactly as at full fidelity (the program
//! gate refuses the operation instead), and an excess of 0 changes
//! nothing.
//!
//! Provenance: our own fixtures; the sagitta `R * (1 - cos(pi / n))` of
//! an inscribed n-gon is elementary geometry. No vendor material.
//!
//! Gate map (#904): covers camcraft_sim::verify (excess, reach, sweep,
//! rapids, rapids_posed).

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::geometry::{cube, Point3, Triangle};
use camcraft_sim::verify::{verify, Mode, ToolGeom, VerifyContext, Warning};

/// The importer's promise, the gouge tolerance, and an excess that
/// leaves 0.003 mm of it (a 0.012 mm mesh).
const CAP: f64 = 0.005;
const TOL: f64 = 0.01;
const EXCESS: f64 = 0.007;

fn region(pts: Vec<Point2>) -> Region {
    match Loop::new(pts).and_then(|l| Region::new(vec![l])) {
        Ok(r) => r,
        Err(e) => panic!("region: {e:?}"),
    }
}

fn square(h: f64) -> Region {
    region(vec![Point2::new(-h, -h), Point2::new(h, -h), Point2::new(h, h), Point2::new(-h, h)])
}

fn path(moves: Vec<Move>) -> Toolpath {
    Toolpath { moves, cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), threads: Vec::new(), probes: Vec::new(), breaks: Vec::new(), plasma: None }
}

fn mv(kind: MoveKind, a: (f64, f64, f64), b: (f64, f64, f64), axis: Option<[f64; 3]>) -> Move {
    let mut m = Move::new(kind, Point3::new(a.0, a.1, a.2), Point3::new(b.0, b.1, b.2), (kind != MoveKind::Rapid).then_some(300.0));
    m.axis = axis;
    m
}

// ---- (c) and (c3): the band stack ----------------------------------------

/// A 6 mm cutter under a 20 mm holder whose bottom stands 10 mm above
/// the tip.
fn held() -> ToolGeom {
    ToolGeom { radius: 3.0, nose_radius: 0.0, flute_len: 5.0, shank_radius: 3.0, shank_len: 5.0, holder_radius: 10.0, holder_len: 5.0 }
}

/// The 80 mm block's X = 40 wall is sampled 0.05 mm inside its middle
/// (the reach law's own nudge): the holder's rim stands `gap` short of
/// that sample when the tip is here.
fn tip_x(gap: f64) -> f64 {
    40.0 - 0.05 - 10.0 - gap
}

fn block_verdict(p: &Toolpath, excess: f64) -> Vec<Warning> {
    let (block, tool) = (square(40.0), held());
    let cx = VerifyContext {
        region: &block,
        stock: Some(&block),
        tool: &tool,
        mode: Mode::Area,
        stock_top_z: 25.0,
        clearance_z: 35.0,
        gouge_tol: TOL,
        z_tol: 1e-6,
        model_excess: excess,
        ..Default::default()
    };
    verify(p, &cx)
}

#[test]
fn a_holder_clearing_a_wall_by_less_than_the_excess_is_convicted_on_a_coarse_model() {
    // A plunge to Z 5: the holder's bottom at Z 15, under the Z 25 top,
    // its rim 0.004 mm short of the uncut wall.
    let plunge = |gap: f64| path(vec![mv(MoveKind::Plunge, (tip_x(gap), 0.0, 30.0), (tip_x(gap), 0.0, 5.0), None)]);
    let near = plunge(0.004);
    assert!(block_verdict(&near, 0.0).is_empty(), "full fidelity keeps today's verdict: {:?}", block_verdict(&near, 0.0));
    assert_eq!(block_verdict(&near, EXCESS), vec![Warning::ShankHit { at: 0, part: false }]);
    // Clear by more than the excess: clean on the coarse model too.
    let clear = plunge(0.008);
    assert!(block_verdict(&clear, EXCESS).is_empty(), "the growth is the excess, no more");
    // In the wall: convicted either way.
    let hit = plunge(-0.5);
    assert!(!block_verdict(&hit, 0.0).is_empty() && !block_verdict(&hit, EXCESS).is_empty());
    // Past the tolerance the law reads untightened (the gate refuses).
    for excess in [TOL, 0.02, 0.5] {
        assert_eq!(block_verdict(&near, excess), block_verdict(&near, 0.0), "excess {excess}");
    }
}

#[test]
fn the_mid_turn_sweep_grows_the_band_the_same_way() {
    // The same plunge, turning 5 degrees AWAY from the wall as it sinks:
    // at its end pose the leaning holder is a millimetre clear, so law
    // (c) is silent - but the upright start pose is reachable all the way
    // down (the head may arrive late), 0.004 mm from the wall.
    let lean = [-(5.0_f64.to_radians().sin()), 0.0, 5.0_f64.to_radians().cos()];
    let turning = path(vec![mv(MoveKind::Plunge, (tip_x(0.004), 0.0, 30.0), (tip_x(0.004), 0.0, 5.0), Some(lean))]);
    assert!(block_verdict(&turning, 0.0).is_empty(), "{:?}", block_verdict(&turning, 0.0));
    assert_eq!(block_verdict(&turning, EXCESS), vec![Warning::SweepHit { at: 0, part: false }]);
    for excess in [TOL, 0.5] {
        assert!(block_verdict(&turning, excess).is_empty(), "past the bound: untightened (excess {excess})");
    }
}

// ---- (d): rapids ----------------------------------------------------------

const BOSS_R: f64 = 20.0;
const SIDES: usize = 90;

/// A round boss as its coarse mesh shows it: the inscribed 90-gon, whose
/// sides stand 0.0122 mm inside the true circle at their middles. One
/// side's middle is at the top (+Y).
fn coarse_boss() -> Region {
    region(
        (0..SIDES)
            .map(|k| {
                let a = std::f64::consts::TAU * k as f64 / SIDES as f64;
                Point2::new(BOSS_R * a.cos(), BOSS_R * a.sin())
            })
            .collect(),
    )
}

fn chord() -> f64 {
    BOSS_R * (1.0 - (std::f64::consts::PI / SIDES as f64).cos())
}

/// A rapid at Z 5 (boss top Z 10) past the top of the boss, its 6 mm
/// radius cutter standing `into` mm inside the TRUE circle.
fn rapid_past(into: f64, excess: f64) -> Vec<Warning> {
    let y = BOSS_R + 6.0 - into;
    let p = path(vec![mv(MoveKind::Rapid, (-3.5, y, 5.0), (3.5, y, 5.0), None)]);
    let boss = coarse_boss();
    let tool = ToolGeom { radius: 6.0, nose_radius: 0.0, flute_len: 20.0, shank_radius: 6.0, shank_len: 20.0, holder_radius: 6.0, holder_len: 10.0 };
    let cx = VerifyContext {
        region: &boss,
        stock: Some(&boss),
        tool: &tool,
        mode: Mode::Area,
        stock_top_z: 10.0,
        clearance_z: 20.0,
        gouge_tol: TOL,
        z_tol: 1e-6,
        model_excess: excess,
        ..Default::default()
    };
    verify(&p, &cx)
}

#[test]
fn a_rapid_grazing_a_convex_wall_inside_the_mesh_error_is_convicted_on_a_coarse_model() {
    let c = chord();
    assert!(c > 0.012 && c < 0.0125, "the fixture's chord error: {c}");
    let excess = c - CAP;
    // 0.018 mm into the real boss. Against the mesh the cutter is only
    // 0.0058 mm in, inside the footprint's 0.01 mm slack: clean today.
    assert!(rapid_past(0.018, 0.0).is_empty(), "{:?}", rapid_past(0.018, 0.0));
    assert_eq!(rapid_past(0.018, excess), vec![Warning::RapidInStock { at: 0 }], "the excess convicts it");
    // A rapid tangent to the true circle is clean both ways; one a tenth
    // of a millimetre in is convicted both ways.
    assert!(rapid_past(0.0, 0.0).is_empty() && rapid_past(0.0, excess).is_empty());
    assert!(!rapid_past(0.1, 0.0).is_empty() && !rapid_past(0.1, excess).is_empty());
    // Past the tolerance: untightened.
    for excess in [TOL, 0.5] {
        assert!(rapid_past(0.018, excess).is_empty(), "excess {excess}");
    }
}

// ---- (d), the posed second reading ---------------------------------------

/// The posed-rapid suite's part: a post X/Y [-5, 5], Z [0, 10] under a
/// ledge X [-5, 15], Y [-5, 5], Z [10, 12].
fn ledge() -> Vec<Triangle> {
    let mut f: Vec<Triangle> = cube(Point3::new(-5.0, -5.0, 0.0), Point3::new(5.0, 5.0, 10.0)).facets.to_vec();
    f.extend(cube(Point3::new(-5.0, -5.0, 10.0), Point3::new(15.0, 5.0, 12.0)).facets.iter().cloned());
    f
}

/// A rapid under the ledge in `Mode::Envelope` (the posed kinds' mode:
/// the solid law (b2) stands down, so the posed reading is the rapid's
/// whole verdict against the part), leaning 60 degrees toward -Y, its
/// flute's top edge raised `up` mm past where it meets the ledge's
/// underside. The cut it leaves was judged at a 0.05 mm finish
/// tolerance, so the reading forgives 0.06 mm of the part - measured
/// across the lean, which is 0.0486 mm of `up` here; with the excess
/// taken out, 0.0405 mm.
fn posed_under_ledge(up: f64, excess: f64) -> Vec<Warning> {
    let lean = [0.0, -(60.0_f64.to_radians().sin()), 60.0_f64.to_radians().cos()];
    // The flute's top edge: 4 mm up the axis, 3 mm out across it.
    let rise = 4.0 * lean[2] + 3.0 * 60.0_f64.to_radians().sin();
    let z = 10.0 - rise + up;
    let p = path(vec![mv(MoveKind::Rapid, (9.0, 0.0, z), (13.0, 0.0, z), Some(lean))]);
    let facets = ledge();
    let shadow = region(vec![Point2::new(-5.0, -5.0), Point2::new(15.0, -5.0), Point2::new(15.0, 5.0), Point2::new(-5.0, 5.0)]);
    let tool = ToolGeom { radius: 3.0, nose_radius: 0.0, flute_len: 4.0, shank_radius: 1.5, shank_len: 40.0, holder_radius: 1.5, holder_len: 10.0 };
    let cx = VerifyContext {
        region: &shadow,
        stock: Some(&shadow),
        model: Some(&facets),
        tool: &tool,
        mode: Mode::Envelope,
        stock_top_z: 12.0,
        clearance_z: 30.0,
        gouge_tol: TOL,
        z_tol: 1e-3,
        finish_tol: Some(0.05),
        model_excess: excess,
        ..Default::default()
    };
    verify(&p, &cx).into_iter().filter(|w| matches!(w, Warning::RapidInStock { .. })).collect()
}

#[test]
fn a_posed_rapid_standing_in_the_part_by_less_than_the_excess_is_convicted_on_a_coarse_model() {
    // Between the two allowances: acquitted today, convicted on the
    // coarse model.
    assert!(posed_under_ledge(0.0445, 0.0).is_empty(), "{:?}", posed_under_ledge(0.0445, 0.0));
    assert_eq!(posed_under_ledge(0.0445, EXCESS), vec![Warning::RapidInStock { at: 0 }]);
    // Clear of the ledge: acquitted both ways. Well inside it: convicted.
    assert!(posed_under_ledge(-0.05, 0.0).is_empty() && posed_under_ledge(-0.05, EXCESS).is_empty());
    assert!(!posed_under_ledge(0.5, 0.0).is_empty() && !posed_under_ledge(0.5, EXCESS).is_empty());
    // The allowance is gouge + finish tolerance, 0.06 mm: an excess that
    // uses up the 0.01 mm gouge tolerance (where the gate refuses) still
    // leaves it positive, so it keeps tightening - never loosening.
    assert_eq!(posed_under_ledge(0.0445, 0.02), vec![Warning::RapidInStock { at: 0 }]);
    // Nothing positive left: it reads untightened.
    assert!(posed_under_ledge(0.0445, 0.5).is_empty());
}

#[test]
fn full_fidelity_grows_nothing() {
    let cx = VerifyContext { gouge_tol: TOL, ..Default::default() };
    assert_eq!(cx.band_growth(), 0.0);
    let coarse = VerifyContext { gouge_tol: TOL, model_excess: EXCESS, ..Default::default() };
    assert!((coarse.band_growth() - EXCESS).abs() < 1e-12);
    assert!((coarse.tightened(TOL) + coarse.band_growth() - TOL).abs() < 1e-12, "what the kerf loses, the band gains");
    let past = VerifyContext { gouge_tol: TOL, model_excess: 0.02, ..Default::default() };
    assert_eq!(past.band_growth(), 0.0, "past the bound the gate refuses; the laws read untightened");
}
