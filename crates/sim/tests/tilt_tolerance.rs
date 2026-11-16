//! #938: the reach laws judge "upright" with one stated angular
//! tolerance. A pose off the Z column by rounding noise (1e-10 degrees,
//! what a rotated setup or a posed operation can leave) is judged
//! exactly as a vertical move - a holder wider than the cutter no longer
//! flips to a false collision on float noise - while a real small tilt,
//! just above the tolerance, still takes the tilted (enclosing) reading
//! (which, since #1303, meets the upright one there: no cliff).
//! And one move reports one ShankHit per verdict, not one per band group.
//!
//! Provenance: our own fixtures, specced from the Prime Directive (no
//! false refusals on noise, no tilted tool passed as upright); no vendor
//! material.
//!
//! Gate map (#938): covers camcraft_sim::verify (upright, reach).

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::fixture::FixtureBox;
use camcraft_sim::geometry::Point3;
use camcraft_sim::verify::{Mode, ToolGeom, VerifyContext, Warning, bands_vs_fixtures, verify};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let l = Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ]);
    match l.and_then(|l| Region::new(vec![l])) {
        Ok(r) => r,
        Err(e) => panic!("rect: {e:?}"),
    }
}

/// A unit axis tilted `rad` off +Z toward +X.
fn tilt(rad: f64) -> [f64; 3] {
    [rad.sin(), 0.0, rad.cos()]
}

fn mv(kind: MoveKind, from: Point3, to: Point3, axis: Option<[f64; 3]>) -> Move {
    Move {
        rotary: None,
        kind,
        from,
        to,
        feed: if kind == MoveKind::Rapid { None } else { Some(500.0) },
        arc: None,
        axis,
    }
}

/// A plunge to Z -25 at the pocket centre, then a feed along X to 2,
/// every move held at `axis`.
fn program(axis: Option<[f64; 3]>) -> Toolpath {
    let moves = vec![
        mv(MoveKind::Plunge, Point3::new(0.0, 0.0, 5.0), Point3::new(0.0, 0.0, -25.0), axis),
        mv(MoveKind::Feed, Point3::new(0.0, 0.0, -25.0), Point3::new(2.0, 0.0, -25.0), axis),
    ];
    Toolpath { moves, cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), threads: Vec::new(), probes: Vec::new(), breaks: Vec::new(), plasma: None }
}

/// Upright, the holder's bottom stands 5 mm above the stock top (Z 0):
/// clean. Its 15 mm radius is wider than the 3 mm cutter; tilted, its
/// rim dips R·sin θ (#1303; the whole radius before, reaching the stock
/// on any tilt at all).
fn wide_holder() -> ToolGeom {
    ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 10.0,
        shank_radius: 3.0,
        shank_len: 20.0,
        holder_radius: 15.0,
        holder_len: 30.0,
    }
}

fn judge(p: &Toolpath, tool: &ToolGeom) -> Vec<Warning> {
    judge_in(p, tool, &rect(-6.0, -6.0, 8.0, 6.0))
}

fn judge_in(p: &Toolpath, tool: &ToolGeom, region: &Region) -> Vec<Warning> {
    let stock = rect(-40.0, -40.0, 40.0, 40.0);
    let cx = VerifyContext {
        region,
        stock: Some(&stock),
        model: None,
        tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: 0.0,
        clearance_z: 30.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-3,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(p, &cx)
}

/// Rounding noise off the column: 1e-10 degrees, as #938 measured.
fn noise() -> Option<[f64; 3]> {
    Some(tilt(1e-10f64.to_radians()))
}

/// Just above the verifier's upright tolerance (1e-9 rad,
/// `verify::upright::UPRIGHT_TILT_RAD`): a tilt it must still see.
fn just_tilted() -> Option<[f64; 3]> {
    Some(tilt(1.01e-9))
}

fn hits_at_both_moves() -> Vec<Warning> {
    vec![Warning::ShankHit { at: 0, part: false }, Warning::ShankHit { at: 1, part: false }]
}

#[test]
fn a_wide_holder_reads_the_same_upright_and_at_a_rounding_error_of_tilt() {
    let tool = wide_holder();
    let none = judge(&program(None), &tool);
    assert!(none.is_empty(), "upright, the holder stands above the stock: {none:?}");
    assert_eq!(judge(&program(Some([0.0, 0.0, 1.0])), &tool), none);
    assert_eq!(judge(&program(noise()), &tool), none, "1e-10 degrees is upright steel");
}

/// Just above the tolerance the tilted reading is taken - and since
/// #1303 it meets the upright one there: the holder's rim dips by its
/// exact R·sin θ (1.5e-8 mm at 1.01e-9 rad), not by its whole 15 mm, so
/// a holder 5 mm clear of the stock stays clear. Before #1303 both of
/// the small tilts below convicted (a cliff at the tolerance). A tilt
/// that really brings the rim into the stock still convicts: at 20
/// degrees the holder's low rim stands 30·cos θ - 15·sin θ - 25 = -1.9
/// mm, under the stock top.
#[test]
fn a_tilt_just_above_the_tolerance_reads_like_upright_and_a_real_dip_convicts() {
    let tool = wide_holder();
    assert_eq!(judge(&program(just_tilted()), &tool), Vec::new(), "no cliff at the tolerance");
    assert_eq!(judge(&program(Some(tilt(0.001f64.to_radians()))), &tool), Vec::new());
    assert_eq!(judge(&program(Some(tilt(20f64.to_radians()))), &tool), hits_at_both_moves());
}

/// A shank wider than the cutter in a tight pocket convicts upright;
/// the noise-tilted pose must convict the very same way, no more.
#[test]
fn a_noise_tilt_keeps_the_upright_conviction_exactly() {
    let mut tool = wide_holder();
    tool.shank_radius = 5.0;
    let tight = rect(-4.0, -4.0, 6.0, 4.0);
    let up = judge_in(&program(None), &tool, &tight);
    assert_eq!(up, hits_at_both_moves());
    assert_eq!(judge_in(&program(noise()), &tool, &tight), up);
}

/// Leaning, the wide shank AND the wide holder both reach the stock:
/// one move, one ShankHit - not one per band group. At 20 degrees, so
/// the holder's rim really dips under the stock top (#1303: just above
/// the tolerance only the shank group convicts now).
#[test]
fn a_leaning_shank_and_holder_report_one_shank_hit_per_move() {
    let mut tool = wide_holder();
    tool.shank_radius = 5.0;
    let tight = rect(-4.0, -4.0, 6.0, 4.0);
    let w = judge_in(&program(Some(tilt(20f64.to_radians()))), &tool, &tight);
    assert_eq!(w, hits_at_both_moves());
    // Just above the tolerance the shank group alone convicts, exactly
    // as upright: the same single report.
    assert_eq!(judge_in(&program(just_tilted()), &tool, &tight), hits_at_both_moves());
}

/// The fixture band law (c2) branches on the same tolerance: a clamp
/// beside the cut whose top stands just under the upright holder's
/// bottom (Z 30) is clear upright and at noise, and convicts a real lean
/// (3 deg; the clamp is struck from about 2.1 deg on).
#[test]
fn the_fixture_band_law_reads_noise_as_upright_and_a_real_tilt_as_tilted() {
    let clamp = [FixtureBox { min: Point3::new(6.0, -8.0, -10.0), max: Point3::new(14.0, 8.0, 29.5) }];
    let feed = |axis| {
        let m = mv(MoveKind::Feed, Point3::new(0.0, 0.0, 0.0), Point3::new(2.0, 0.0, 0.0), axis);
        Toolpath::new(vec![m])
    };
    let tool = wide_holder();
    let up = bands_vs_fixtures(&feed(None), &tool, &clamp, None, 0.01);
    assert!(up.is_empty(), "upright, the holder clears the clamp: {up:?}");
    assert_eq!(bands_vs_fixtures(&feed(noise()), &tool, &clamp, None, 0.01), up);
    let lean = bands_vs_fixtures(&feed(Some(tilt(3f64.to_radians()))), &tool, &clamp, None, 0.01);
    assert!(!lean.is_empty(), "a real tilt keeps the enclosing reading");
}
