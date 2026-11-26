//! #864: the gouge law on a model coarser than the importer promises. A
//! facet of a convex wall stands INSIDE the true surface by its chord
//! error, so a cut that deep into the real part reads as clean against
//! the mesh. `VerifyContext::model_excess` gives that much of the
//! tolerance back - while a positive tolerance is left; past that the law
//! reads untightened (the program gate refuses instead) so an exactly
//! finished flat face is never called a gouge.
//!
//! Provenance: our own fixtures; the sagitta `R * (1 - cos(pi / n))` of an
//! inscribed n-gon is elementary geometry. No vendor material.
//!
//! Gate map (#864, #904, #1057): covers camcraft_sim::verify (excess, section,
//! finish).

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::geometry::{Point3, Triangle};
use camcraft_sim::verify::{excess_floor, excess_floor_for, verify, Mode, ToolGeom, VerifyContext, Warning};

#[path = "model_excess/bullnose_envelope.rs"]
mod bullnose_envelope;

/// The importer's promise and the verdict tolerance, mm.
const CAP: f64 = 0.005;
const TOL: f64 = 0.01;
/// The true boss, the cutter, and the mesh's side count: a 90-gon of a
/// 20 mm circle stands 0.0122 mm inside it at each facet's middle.
const BOSS_R: f64 = 20.0;
const TOOL_R: f64 = 6.0;
const SIDES: usize = 90;

fn region(pts: Vec<Point2>) -> Region {
    match Loop::new(pts).and_then(|l| Region::new(vec![l])) {
        Ok(r) => r,
        Err(e) => panic!("region: {e:?}"),
    }
}

/// The boss as its coarse mesh shows it: the inscribed polygon.
fn coarse_boss() -> Region {
    region(ring(BOSS_R, SIDES).collect())
}

fn ring(r: f64, n: usize) -> impl Iterator<Item = Point2> {
    (0..n).map(move |k| {
        let a = std::f64::consts::TAU * k as f64 / n as f64;
        Point2::new(r * a.cos(), r * a.sin())
    })
}

/// The chord error of the coarse boss.
fn chord() -> f64 {
    BOSS_R * (1.0 - (std::f64::consts::PI / SIDES as f64).cos())
}

fn tool() -> ToolGeom {
    ToolGeom {
        radius: TOOL_R,
        nose_radius: 0.0,
        flute_len: 20.0,
        shank_radius: TOOL_R,
        shank_len: 20.0,
        holder_radius: TOOL_R,
        holder_len: 10.0,
    }
}

fn feeds(pts: &[Point2], z: f64) -> Toolpath {
    let at = |p: Point2| Point3::new(p.x, p.y, z);
    let moves = pts
        .iter()
        .zip(pts.iter().cycle().skip(1))
        .take(pts.len())
        .map(|(a, b)| Move::new(MoveKind::Feed, at(*a), at(*b), Some(600.0)))
        .collect();
    Toolpath { moves, cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), threads: Vec::new(), probes: Vec::new(), breaks: Vec::new(), plasma: None }
}

/// The gouge verdicts of `path` round `wall` with a model `excess`.
fn gouges(path: &Toolpath, wall: &Region, excess: f64) -> usize {
    gouges_with_cause(path, wall, excess, false)
}

fn gouges_with_cause(path: &Toolpath, wall: &Region, excess: f64, file_tolerance: bool) -> usize {
    let tool = tool();
    let cx = VerifyContext {
        region: wall,
        tool: &tool,
        mode: Mode::Boundary,
        stock_top_z: 10.0,
        clearance_z: 20.0,
        gouge_tol: TOL,
        z_tol: 1e-6,
        model_excess: excess,
        file_tolerance,
        ..Default::default()
    };
    verify(path, &cx).iter().filter(|w| matches!(w, Warning::Gouge { .. })).count()
}

/// A ring round the boss whose cutter stands `into` mm inside the TRUE
/// circle: 720 short feeds, so the path's own chords add under a micron.
fn ring_cut(into: f64) -> Toolpath {
    let pts: Vec<Point2> = ring(BOSS_R + TOOL_R - into, 720).collect();
    feeds(&pts, 5.0)
}

#[test]
fn a_cut_into_a_convex_wall_shallower_than_the_mesh_error_is_convicted_on_a_coarse_model() {
    let (c, wall) = (chord(), coarse_boss());
    assert!(c > 0.012 && c < 0.0125, "the fixture's chord error: {c}");
    let excess = c - CAP;
    assert!(excess_floor(TOL, excess).is_some_and(|t| t > 0.0), "a positive tolerance is left");
    // 0.006 mm into the real boss: less than the mesh's chord error, and -
    // measured against the mesh's vertices - inside the 0.01 mm verdict.
    let path = ring_cut(0.006);
    assert_eq!(gouges(&path, &wall, 0.0), 0, "the full-fidelity reading keeps today's verdict");
    assert!(gouges(&path, &wall, excess) > 0, "the coarse model's excess convicts it");
    // A nominal ring - the cutter on the true circle - is clean both ways.
    let nominal = ring_cut(0.0);
    assert_eq!(gouges(&nominal, &wall, 0.0), 0);
    assert_eq!(gouges(&nominal, &wall, excess), 0, "tightening is not a blanket conviction");
}

#[test]
fn an_exactly_finished_flat_face_is_never_a_gouge_on_a_coarse_model() {
    // A square boss: flat walls carry no chord error. The cutter runs
    // exactly its radius off the X = 20 wall.
    let wall = region(vec![Point2::new(-20.0, -20.0), Point2::new(20.0, -20.0), Point2::new(20.0, 20.0), Point2::new(-20.0, 20.0)]);
    let x = 20.0 + TOOL_R;
    let m = Move::new(MoveKind::Feed, Point3::new(x, -20.0, 5.0), Point3::new(x, 20.0, 5.0), Some(600.0));
    let path = Toolpath { moves: vec![m], cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), threads: Vec::new(), probes: Vec::new(), breaks: Vec::new(), plasma: None };
    for excess in [0.0, 0.004, 0.007, 0.009_999, 0.01, 0.02, 0.5] {
        assert_eq!(gouges(&path, &wall, excess), 0, "excess {excess}");
    }
}

#[test]
fn past_the_tolerance_the_law_reads_untightened_and_leaves_the_refusal_to_the_gate() {
    let wall = coarse_boss();
    // An excess that uses the whole tolerance: nothing honest is left.
    for excess in [TOL, 0.02, 0.5] {
        assert_eq!(excess_floor(TOL, excess), None, "excess {excess}");
        // Every path reads exactly as it does at full fidelity.
        for into in [0.0, 0.006, 0.05] {
            let path = ring_cut(into);
            assert_eq!(gouges(&path, &wall, excess), gouges(&path, &wall, 0.0), "excess {excess}, {into} mm in");
        }
    }
    // ...and a real gouge is still one there.
    assert!(gouges(&ring_cut(0.05), &wall, 0.5) > 0);
}

#[test]
fn the_excess_floor_only_ever_tightens() {
    assert_eq!(excess_floor(TOL, 0.0), Some(TOL), "no excess: the tolerance itself");
    assert_eq!(excess_floor(TOL, -1.0), Some(TOL), "a negative excess is none");
    assert_eq!(excess_floor(TOL, f64::NAN), Some(TOL), "a NaN excess is none");
    assert_eq!(excess_floor(TOL, f64::INFINITY), Some(TOL), "an infinite excess is none");
    let t = excess_floor(TOL, 0.004).expect("positive");
    assert!((t - 0.006).abs() < 1e-12 && t < TOL);
    assert_eq!(excess_floor(TOL, TOL), None, "zero left is a refusal, not a tolerance");
    // #2996: an excess past the limit (a chord over 0.02 mm) leaves none.
    assert_eq!(excess_floor(TOL, 0.016), None, "past the limit: a refusal");
    // The default context carries no excess: every law reads as before.
    let cx = VerifyContext::default();
    assert_eq!(cx.model_excess, 0.0);
    assert_eq!(cx.tightened(TOL), TOL);
}

// ---- law (f): the envelope's gouge half (#904) ---------------------------

/// A 6 mm ball rastered over a flat 40 mm plate (top Z 0) with its tip
/// `d` mm below the surface: 0.5 mm passes, well inside the scallop.
fn sunk_raster(d: f64) -> Toolpath {
    let mut moves = Vec::new();
    let (mut y, mut right) = (-12.0, true);
    while y <= 12.0 + 1e-9 {
        let (x0, x1) = if right { (-12.0, 12.0) } else { (12.0, -12.0) };
        moves.push(Move::new(MoveKind::Feed, Point3::new(x0, y, -d), Point3::new(x1, y, -d), Some(600.0)));
        if y + 0.5 <= 12.0 + 1e-9 {
            moves.push(Move::new(MoveKind::Feed, Point3::new(x1, y, -d), Point3::new(x1, y + 0.5, -d), Some(600.0)));
        }
        y += 0.5;
        right = !right;
    }
    Toolpath { moves, cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), threads: Vec::new(), probes: Vec::new(), breaks: Vec::new(), plasma: None }
}

fn plate() -> Vec<Triangle> {
    let p = |x: f64, y: f64| Point3::new(x, y, 0.0);
    vec![Triangle::new(p(-20.0, -20.0), p(20.0, -20.0), p(20.0, 20.0)), Triangle::new(p(-20.0, -20.0), p(20.0, 20.0), p(-20.0, 20.0))]
}

/// The envelope verdict's gouge reading of `path`, `None` when silent.
fn envelope_gouge(path: &Toolpath, mode: Mode, finish_tol: f64, excess: f64) -> Option<f64> {
    let (facets, wall) = (plate(), region(vec![Point2::new(-20.0, -20.0), Point2::new(20.0, -20.0), Point2::new(20.0, 20.0), Point2::new(-20.0, 20.0)]));
    let ball = ToolGeom { radius: 3.0, nose_radius: 0.0, flute_len: 20.0, shank_radius: 3.0, shank_len: 20.0, holder_radius: 3.0, holder_len: 10.0 };
    let cx = VerifyContext {
        region: &wall,
        model: Some(&facets),
        tool: &ball,
        mode,
        stock_top_z: 0.0,
        clearance_z: 20.0,
        gouge_tol: TOL,
        z_tol: 1e-6,
        finish_tol: Some(finish_tol),
        model_excess: excess,
        ..Default::default()
    };
    verify(path, &cx).iter().find_map(|w| match w {
        Warning::Envelope { gouge_mm, .. } => Some(*gouge_mm),
        _ => None,
    })
}

#[test]
fn the_envelope_laws_gouge_half_answers_to_the_tolerance_less_the_excess() {
    // The posed kinds (5-axis, rotary) and the turning nose kinds are
    // judged by this law alone: a posed ball is the same ball about the
    // same centre. Its gate is the finish tolerance plus the scan's own
    // quantization floor (the ball's sagitta across one cell).
    const FINISH: f64 = 0.05;
    let cell = match camcraft_sim::stock_sim::envelope_scan(&plate(), &sunk_raster(0.0).moves, 3.0, FINISH) {
        Ok(rep) => rep.cell,
        Err(e) => panic!("the scan measures the plate: {e:?}"),
    };
    let gate = FINISH + cell * cell / 6.0;
    let excess = 0.007;
    for mode in [Mode::Envelope, Mode::Rotary] {
        // 0.004 mm inside the gate: silent at full fidelity, convicted
        // with a 0.007 mm excess - and measured, not invented.
        let near = sunk_raster(gate - 0.004);
        assert_eq!(envelope_gouge(&near, mode, FINISH, 0.0), None, "{mode:?}: today's verdict");
        let g = envelope_gouge(&near, mode, FINISH, excess).unwrap_or(0.0);
        assert!((g - (gate - 0.004)).abs() < 1e-6, "{mode:?}: convicted at its own depth, {g}");
        // 0.009 mm inside the gate: silent on the coarse model too.
        assert_eq!(envelope_gouge(&sunk_raster(gate - 0.009), mode, FINISH, excess), None, "{mode:?}");
        // On the surface: never a gouge, whatever the excess.
        for e in [0.0, excess, FINISH, 0.5] {
            assert_eq!(envelope_gouge(&sunk_raster(0.0), mode, FINISH, e), None, "{mode:?}, excess {e}");
        }
        // Past the tolerance the law reads untightened; a real gouge
        // is one at every excess.
        assert_eq!(envelope_gouge(&near, mode, FINISH, FINISH), None, "{mode:?}: the gate refuses instead");
        for e in [0.0, excess, FINISH, 0.5] {
            assert!(envelope_gouge(&sunk_raster(0.3), mode, FINISH, e).is_some_and(|g| g > 0.29), "{mode:?}, excess {e}");
        }
    }
}

/// #1057: a containment kind's rim is our own polygon of a declared
/// circle, not a model facet - law (b) reads it at the plain tolerance,
/// every other loop still tightened.
#[test]
fn a_declared_rim_is_read_untightened_and_no_other_loop_is() {
    // A 48-gon rim of a 2 mm hole: 0.0043 mm of its own sagitta.
    let rim = region(ring(2.0, 48).collect());
    let drill = ToolGeom { radius: 2.0, ..tool() };
    let plunge = {
        let (a, b) = (Point3::new(0.0, 0.0, 12.0), Point3::new(0.0, 0.0, 8.0));
        let mut p = feeds(&[], 0.0);
        p.moves.push(Move::new(MoveKind::Feed, a, b, Some(100.0)));
        p
    };
    let judge = |wall: &Region, tool: &ToolGeom, path: &Toolpath, excess: f64, rims: &[(Point2, f64)]| {
        let cx = VerifyContext {
            region: wall,
            tool,
            mode: Mode::Boundary,
            stock_top_z: 10.0,
            clearance_z: 20.0,
            gouge_tol: TOL,
            z_tol: 1e-6,
            model_excess: excess,
            rims,
            ..Default::default()
        };
        verify(path, &cx).iter().filter(|w| matches!(w, Warning::Gouge { .. })).count()
    };
    let declared = [(Point2::new(0.0, 0.0), 2.0)];
    // Just under the bound: 0.0001 mm left of the tightened tolerance.
    let near = TOL - 1e-4;
    assert_eq!(judge(&rim, &drill, &plunge, 0.0, &[]), 0, "full fidelity: clean");
    // With no file-tolerance cause, the original strict law convicts this rim.
    assert!(judge(&rim, &drill, &plunge, near, &[]) > 0, "undeclared, the strict tolerance is below the rim's sagitta");
    assert_eq!(judge(&rim, &drill, &plunge, near, &declared), 0, "declared, the rim reads untightened");
    // An oversize drill against the declared rim still convicts.
    let fat = ToolGeom { radius: 2.01, ..drill };
    assert!(judge(&rim, &fat, &plunge, near, &declared) > 0);
    // A declaration that does not trace the loop changes nothing: the
    // coarse boss stays convicted with a circle 0.01 mm off its own.
    let (wall, excess) = (coarse_boss(), chord() - CAP);
    let off = [(Point2::new(0.0, 0.0), BOSS_R - 0.01)];
    let t6 = tool();
    assert!(judge(&wall, &t6, &ring_cut(0.006), excess, &off) > 0);
}

// ---- #2996: a chord past DEV_TOL, up to the hard limit -------------------

#[test]
fn past_dev_tol_up_to_the_limit_only_the_promised_chord_is_taken() {
    use camcraft_sim::verify::{band_growth, PROMISED_CHORD, WIDE_CHORD_LIMIT};
    assert_eq!(PROMISED_CHORD, CAP);
    assert_eq!(WIDE_CHORD_LIMIT, 0.02, "a constant no file can raise");
    // Up to a chord of DEV_TOL: #864's law, the whole excess is taken.
    for excess in [0.001, 0.004, CAP] {
        let t = excess_floor_for(TOL, excess, true).expect("positive");
        assert!((t - (TOL - excess)).abs() < 1e-12, "excess {excess}: {t}");
    }
    // Past it, up to a chord of 0.02 mm: only the promised chord.
    for chord in [0.0101, 0.012, 0.019, WIDE_CHORD_LIMIT] {
        let t = excess_floor_for(TOL, chord - CAP, true).expect("judged under the limit");
        assert!((t - (TOL - CAP)).abs() < 1e-12, "chord {chord}: {t}");
        // Never wider than the law's own tolerance: a tolerance smaller
        // than the promised chord is still refused.
        assert!(excess_floor_for(0.003, chord - CAP, true).is_none(), "chord {chord}");
        // A collision band grows by the whole excess, never less.
        assert!((band_growth(TOL, chord - CAP) - (chord - CAP)).abs() < 1e-12, "chord {chord}");
    }
    // Past the limit: the whole excess again, so nothing is left.
    for chord in [0.0201, 0.021, 0.5] {
        assert_eq!(excess_floor_for(TOL, chord - CAP, true), None, "chord {chord}");
        assert_eq!(band_growth(TOL, chord - CAP), 0.0, "refused, not grown: chord {chord}");
    }
    assert_eq!(band_growth(TOL, 0.0), 0.0, "full fidelity grows nothing");
}

#[test]
fn a_cut_deeper_than_the_widened_tolerance_into_a_convex_wall_is_still_convicted() {
    // The boss's own chord error (0.0122 mm) is past DEV_TOL: judged at
    // the widened tolerance, a cut that deep into the real wall is caught.
    let (c, wall) = (chord(), coarse_boss());
    let excess = c - CAP;
    assert!(excess_floor_for(TOL, excess, true).is_some());
    assert!(gouges_with_cause(&ring_cut(c + TOL), &wall, excess, true) > 0, "a real gouge past the widened tolerance");
    assert_eq!(gouges_with_cause(&ring_cut(0.0), &wall, excess, true), 0, "the nominal ring stays clean");
}

#[test]
fn absent_or_budget_cause_keeps_the_original_mesh_tolerance() {
    for file_tolerance in [false, true] {
        let mesh = excess_floor_for(TOL, 0.012 - CAP, file_tolerance).expect("positive");
        let want = if file_tolerance { 0.005 } else { 0.003 };
        assert!((mesh - want).abs() < 1e-12, "file cause {file_tolerance}: {mesh}");
    }
    assert_eq!(excess_floor_for(TOL, 0.019 - CAP, false), None);
    assert_eq!(excess_floor(TOL, 0.019 - CAP), None, "missing cause is strict");
    let mesh = excess_floor_for(TOL, 0.019 - CAP, true).expect("file tolerance");
    assert!((mesh - 0.005).abs() < 1e-12, "mesh allowance");
    assert!((0.019 + mesh - 0.024).abs() < 1e-12, "true-surface bound");
}

#[test]
fn an_eligible_file_caps_larger_law_tolerances_at_the_header_allowance() {
    for tolerance in [TOL, 0.025, 0.1] {
        for chord in [0.012, 0.019, 0.020] {
            let excess = chord - CAP;
            let mesh = excess_floor_for(tolerance, excess, true).expect("file tolerance");
            assert!((mesh - CAP).abs() < 1e-12, "mesh allowance at {tolerance}");
            assert!(chord + mesh <= 0.025, "true-surface bound");
            let context = VerifyContext { gouge_tol: tolerance, model_excess: excess, file_tolerance: true, ..Default::default() };
            assert!((context.band_growth() - excess).abs() < 1e-12, "the collision band keeps the whole excess");
        }
    }
    assert!((excess_floor_for(0.025, 0.004, true).expect("mild") - 0.021).abs() < 1e-12);
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(excess_floor_for(invalid, 0.014, true), None, "invalid tolerance");
    }
}
