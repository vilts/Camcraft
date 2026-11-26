//! #3588: law (c)/(c3) band reach on an indexed sample set gives the findings of the old pooled scan, and its cost grows with the moves, not with moves x planes - our own fixtures.
//!
//! Gate map (#3588): covers camcraft_sim::verify (reach, sample_grid).

use super::reach::{ReachStats, band_reach_counted};
use super::reach_reference::band_reach_reference;
use super::rest::Rest;
use super::types::{Mode, ToolGeom, VerifyContext, Warning};
use crate::cl::{Move, MoveKind, Toolpath};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::{Point3, Triangle, cube};

fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let pts = vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)];
    match Loop::new(pts).and_then(|l| Region::new(vec![l])) {
        Ok(r) => r,
        Err(e) => panic!("region: {e:?}"),
    }
}

/// A 6 mm cutter, a 10 mm shank 5 mm over the tip, a 30 mm holder.
fn tool() -> ToolGeom {
    ToolGeom { radius: 3.0, nose_radius: 0.0, flute_len: 5.0, shank_radius: 5.0, shank_len: 20.0, holder_radius: 15.0, holder_len: 30.0 }
}

/// A 20 mm boss (Z 0..10) on a 2 mm floor plate, in a 100 mm pocket.
fn model() -> Vec<Triangle> {
    let mut f = cube(Point3::new(0.0, 0.0, -2.0), Point3::new(100.0, 100.0, 0.0)).facets.to_vec();
    f.extend(cube(Point3::new(40.0, 40.0, 0.0), Point3::new(60.0, 60.0, 10.0)).facets.iter().cloned());
    f
}

/// A stepped boss on the same floor plate: a wide low block (X and Y 40..60,
/// Z 0..5) under a narrow tall block (X and Y 45..55, Z 5..10). The island
/// section is the wide block below Z 5 and the narrow block above it, so each
/// Z band puts its wall points at a different Y - the shape a levels-only
/// plane set cannot reproduce.
fn stepped_model() -> Vec<Triangle> {
    let mut f = cube(Point3::new(0.0, 0.0, -2.0), Point3::new(100.0, 100.0, 0.0)).facets.to_vec();
    f.extend(cube(Point3::new(40.0, 40.0, 0.0), Point3::new(60.0, 60.0, 5.0)).facets.iter().cloned());
    f.extend(cube(Point3::new(45.0, 45.0, 5.0), Point3::new(55.0, 55.0, 10.0)).facets.iter().cloned());
    f
}

fn feed(a: (f64, f64, f64), b: (f64, f64, f64)) -> Move {
    Move::new(MoveKind::Feed, Point3::new(a.0, a.1, a.2), Point3::new(b.0, b.1, b.2), Some(500.0))
}

/// Run the new law and the old one on the same model, each on its own rest
/// model (the island cache is per rest, so each pays its own slices).
fn both_on(
    model: Vec<Triangle>,
    moves: Vec<Move>,
    mode: Mode,
) -> ((Vec<Warning>, ReachStats), (Vec<Warning>, ReachStats)) {
    let (region, tool) = (square(0.0, 0.0, 100.0, 100.0), tool());
    let cx = VerifyContext {
        region: &region,
        stock: Some(&region),
        model: Some(&model),
        tool: &tool,
        mode,
        stock_top_z: 10.0,
        clearance_z: 30.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        ..Default::default()
    };
    let path = Toolpath { moves, ..Toolpath::default() };
    let mut new = Vec::new();
    let s_new = band_reach_counted(&path, &cx, &Rest::new(&path, &cx, cx.gouge_tol, &[]), cx.gouge_tol, &mut new);
    let mut old = Vec::new();
    // #3831: the reference reads every plane on its own, as the old scan did.
    let s_old = band_reach_reference(&path, &cx, &Rest::new(&path, &cx, cx.gouge_tol, &[]).plane_exact(), cx.gouge_tol, &mut old);
    ((new, s_new), (old, s_old))
}

/// A ramp of `n` short moves along one straight pass at Y 34 (the
/// tool's rim 3 mm from the boss wall, the shank's 2 mm short of it),
/// X 10 to 90 and Z 9 down to Z 1 (a 5.7 degree ramp): every move
/// ends at its own Z.
fn ramp(n: usize) -> Vec<Move> {
    ramp_at(n, 34.0)
}

/// The same ramp at Y `y`.
fn ramp_at(n: usize, y: f64) -> Vec<Move> {
    let (dz, dx) = (8.0 / n as f64, 80.0 / n as f64);
    (0..n)
        .map(|k| {
            let (a, b) = (k as f64, (k + 1) as f64);
            feed((10.0 + a * dx, y, 9.0 - a * dz), (10.0 + b * dx, y, 9.0 - b * dz))
        })
        .collect()
}

/// Square rings round the boss at four levels, `gap` mm from its walls,
/// four moves a side (the law reads a straight move at its end).
fn levels(gap: f64) -> Vec<Move> {
    let (lo, hi) = (40.0 - 3.0 - gap, 60.0 + 3.0 + gap);
    let corners = [(lo, lo), (hi, lo), (hi, hi), (lo, hi), (lo, lo)];
    let mut v = Vec::new();
    for z in [8.0, 6.0, 4.0, 2.0] {
        for w in corners.windows(2) {
            let (a, b) = (w[0], w[1]);
            for k in 0..4 {
                let at = |t: f64| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, z);
                v.push(feed(at(k as f64 / 4.0), at((k + 1) as f64 / 4.0)));
            }
        }
    }
    v
}

/// A helix of 3 turns round the boss centre, 72 chords a turn.
fn helix() -> Vec<Move> {
    let (c, rad, n) = (50.0, 20.0, 216);
    let at = |k: usize| {
        let t = k as f64 / 72.0 * std::f64::consts::TAU;
        (c + rad * t.cos(), c + rad * t.sin(), 9.0 - 7.0 * k as f64 / n as f64)
    };
    (0..n).map(|k| feed(at(k), at(k + 1))).collect()
}

fn assert_same(name: &str, moves: Vec<Move>, mode: Mode) -> (Vec<Warning>, ReachStats, ReachStats) {
    assert_same_on(name, model(), moves, mode)
}

/// #3615: every verdict, its order and the slice (reach) count must equal the
/// all-plane reference, and the sample grid may only cut the tested pairs.
fn assert_same_on(
    name: &str,
    model: Vec<Triangle>,
    moves: Vec<Move>,
    mode: Mode,
) -> (Vec<Warning>, ReachStats, ReachStats) {
    let ((new, s_new), (old, s_old)) = both_on(model, moves, mode);
    assert_eq!(new, old, "{name}: the findings and their order must not move");
    assert_eq!(s_new.slices, s_old.slices, "{name}: one slice per plane, as before");
    assert!(s_new.sample_tests <= s_old.sample_tests, "{name}: {s_new:?} vs {s_old:?}");
    (new, s_new, s_old)
}

#[test]
fn indexed_band_reach_keeps_every_finding_of_the_pooled_scan() {
    assert_same("levels clear", levels(3.0), Mode::Area);
    assert_same("helix", helix(), Mode::Area);
    assert_same("ramp", ramp(400), Mode::Area);
    assert_same("boundary mode", levels(3.0), Mode::Boundary);
    // A real finding: rings 1 mm off the boss walls put the 5 mm shank
    // (bottom 5 mm over the tip, under the boss top below Z 5) in the
    // boss - the part. Both laws must name the same moves.
    let (hits, _, _) = assert_same("levels hit", levels(1.0), Mode::Area);
    assert!(hits.iter().any(|w| matches!(w, Warning::ShankHit { part: true, .. })), "{hits:?}");
    // Leaning moves whose pose changes every move run the sweep (c3).
    let mut posed = levels(1.0);
    for (k, m) in posed.iter_mut().enumerate() {
        m.axis = Some(if k % 2 == 0 { [0.2, 0.0, 1.0] } else { [0.0, 0.2, 1.0] });
    }
    let (posed_hits, _, _) = assert_same("posed", posed, Mode::Area);
    assert!(!posed_hits.is_empty(), "the leaning rings meet the boss");
}

#[test]
fn degenerate_paths_match_the_pooled_scan_and_do_not_panic() {
    assert_same("empty", Vec::new(), Mode::Area);
    let mut bad = levels(1.0);
    bad.push(feed((30.0, 30.0, f64::NAN), (31.0, 30.0, f64::NAN)));
    bad.push(feed((f64::NAN, 30.0, 2.0), (31.0, f64::INFINITY, 2.0)));
    assert_same("non-finite", bad, Mode::Area);
}

#[test]
fn a_many_plane_ramp_reads_only_the_samples_near_its_discs() {
    let (_, short_new, short_old) = assert_same("ramp 500", ramp(500), Mode::Area);
    let (_, long_new, long_old) = assert_same("ramp 2000", ramp(2000), Mode::Area);
    eprintln!("ramp 500: new {short_new:?}, old {short_old:?}");
    eprintln!("ramp 2000: new {long_new:?}, old {long_old:?}");
    // One slice per distinct plane: linear in the moves of a ramp.
    assert!(long_new.slices <= 2001, "{long_new:?}");
    // The old scan paid moves x planes x samples (about sixteen times
    // for four times the moves); a disc clear of every wall reads at
    // most a few samples a move.
    assert!(long_old.sample_tests >= 15 * short_old.sample_tests, "{short_old:?} -> {long_old:?}");
    assert!(long_new.sample_tests <= 2000, "{long_new:?}");
    // A ramp whose shank reaches the boss wall (Y 36: the shank's rim
    // 1 mm into it) must read that wall's samples of EVERY plane - the
    // law reads no sample's plane, and each plane's section puts its
    // own vertices on the wall. Exactness keeps that growth; the index
    // only drops the samples no disc can hold, a fixed large share.
    let (short_hits, s_new, s_old) = assert_same("wall ramp 500", ramp_at(500, 36.0), Mode::Area);
    let (long_hits, l_new, l_old) = assert_same("wall ramp 2000", ramp_at(2000, 36.0), Mode::Area);
    eprintln!("wall ramp 500: new {s_new:?}, old {s_old:?}; wall ramp 2000: new {l_new:?}, old {l_old:?}");
    assert!(!short_hits.is_empty() && !long_hits.is_empty(), "the shank meets the boss");
    assert!(s_new.sample_tests > 0, "{s_new:?}");
    assert!(20 * l_new.sample_tests <= l_old.sample_tests, "{l_new:?} vs {l_old:?}");

    // #3615: a stepped part has a different island section in each Z band,
    // so a levels-only plane set would keep one band's wall points and miss
    // the other's. Keeping every plane keeps both; the all-plane reference
    // must agree with the indexed law on every verdict, its order and the
    // slice count. The ramp at Y 49 puts the shank's annulus on the narrow
    // upper block's Y 45 wall (4 mm off the path, so the flute cannot clear
    // it) and clear of the wide lower block's nearest wall sample (over 5 mm
    // away): a hit the levels-only deepest section cannot reproduce.
    let (stepped_hits, st_new, st_old) =
        assert_same_on("stepped wall ramp", stepped_model(), ramp_at(500, 49.0), Mode::Area);
    eprintln!("stepped wall ramp: new {st_new:?}, old {st_old:?}");
    assert!(
        stepped_hits.iter().any(|w| matches!(w, Warning::ShankHit { part: true, .. })),
        "the shank meets the stepped wall: {stepped_hits:?}"
    );
    assert!(st_new.sample_tests > 0, "{st_new:?}");
}
