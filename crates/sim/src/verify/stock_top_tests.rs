//! #1091: the stock-top re-reading of law (d)'s dives - pinned on a
//! stepped slab (Z 20, a boss to Z 30 over x 10..20) with an 8 mm tool.

use super::*;
use super::super::stock_top_shell::closed_shell;
use crate::cl::Move;
use crate::geometry::{Point3, cube};

fn tool() -> ToolGeom {
    ToolGeom {
        radius: 4.0,
        nose_radius: 0.0,
        flute_len: 20.0,
        shank_radius: 4.0,
        shank_len: 30.0,
        holder_radius: 16.0,
        holder_len: 40.0,
    }
}

fn quad(a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>) {
    f.push(Triangle::new(a, b, c));
    f.push(Triangle::new(a, c, d));
}

/// The slab with the boss as ONE closed, outward shell: the slab's low
/// top ringed round the boss's foot. `boss_top` false leaves the boss's
/// two top facets out - an open shell (the PR #1098 review's case).
fn stepped_shell(boss_top: bool) -> Vec<Triangle> {
    let slab = cube(Point3::new(-30.0, -30.0, -10.0), Point3::new(30.0, 30.0, 20.0)).facets;
    let boss = cube(Point3::new(10.0, -5.0, 20.0), Point3::new(20.0, 5.0, 30.0)).facets;
    let mut f: Vec<Triangle> = slab.iter().take(2).cloned().collect(); // bottom
    f.extend(slab.iter().skip(4).cloned()); // sides
    let o = [(-30.0, -30.0), (30.0, -30.0), (30.0, 30.0), (-30.0, 30.0)];
    let i = [(10.0, -5.0), (20.0, -5.0), (20.0, 5.0), (10.0, 5.0)];
    let p = |(x, y): (f64, f64)| Point3::new(x, y, 20.0);
    for k in 0..4 {
        let n = (k + 1) % 4;
        quad(p(o[k]), p(o[n]), p(i[n]), p(i[k]), &mut f);
    }
    f.extend(boss.iter().skip(if boss_top { 2 } else { 4 }).cloned());
    f
}

fn stepped() -> Vec<Triangle> {
    stepped_shell(true)
}

fn dive_at(x: f64, z1: f64, axis: Option<[f64; 3]>) -> Toolpath {
    Toolpath {
        moves: vec![Move {
            rotary: None,
            kind: MoveKind::Rapid,
            from: Point3::new(x, 0.0, 40.0),
            to: Point3::new(x, 0.0, z1),
            feed: None,
            arc: None,
            axis,
        }],
        cycles: Vec::new(),
        notes: Vec::new(),
        comps: Vec::new(),
        threads: Vec::new(),
        probes: Vec::new(),
        breaks: Vec::new(),
        plasma: None,
    }
}

fn judged(path: &Toolpath, mode: Mode, solids: &[&[Triangle]], t: &ToolGeom) -> Vec<Warning> {
    let mut w = vec![Warning::RapidInStock { at: 0 }];
    acquit_by_stock_top(path, mode, solids, t, 1e-3, 0.0, &mut w);
    w
}

const CONVICTED: [Warning; 1] = [Warning::RapidInStock { at: 0 }];

#[test]
fn a_dive_over_the_low_step_is_acquitted() {
    let s = stepped();
    assert!(judged(&dive_at(0.0, 22.0, None), Mode::Area, &[&s], &tool()).is_empty());
}

#[test]
fn a_dive_over_the_boss_stands() {
    let s = stepped();
    assert_eq!(judged(&dive_at(15.0, 22.0, None), Mode::Area, &[&s], &tool()), CONVICTED);
}

#[test]
fn a_boss_under_the_rim_only_stands() {
    // The centre 3 mm off the boss's edge, the 4 mm rim over it.
    let s = stepped();
    assert_eq!(judged(&dive_at(7.0, 22.0, None), Mode::Area, &[&s], &tool()), CONVICTED);
    // 4.5 mm off: the flute clears it, and so do shank and holder (the
    // holder's bottom stands 50 mm over the tip).
    assert!(judged(&dive_at(5.5, 22.0, None), Mode::Area, &[&s], &tool()).is_empty());
}

#[test]
fn a_holder_band_over_the_boss_stands() {
    // A stubby stack: the holder (radius 16) starts 6 mm over the tip at
    // Z 28, under the boss's top 6 mm away - a crash the flute disc misses.
    let s = stepped();
    let stub = ToolGeom { flute_len: 3.0, shank_len: 3.0, ..tool() };
    assert_eq!(judged(&dive_at(0.0, 22.0, None), Mode::Area, &[&s], &stub), CONVICTED);
}

#[test]
fn the_part_reading_counts_too() {
    // A part standing to Z 25 under the dive, stock low: still convicted.
    let s = stepped();
    let part = cube(Point3::new(-3.0, -3.0, 0.0), Point3::new(3.0, 3.0, 25.0)).facets.to_vec();
    assert_eq!(judged(&dive_at(0.0, 22.0, None), Mode::Area, &[&s, &part], &tool()), CONVICTED);
}

#[test]
fn unknowns_fail_closed() {
    let s = stepped();
    // No facet under the flute: no proof.
    assert_eq!(judged(&dive_at(100.0, 22.0, None), Mode::Area, &[&s], &tool()), CONVICTED);
    // No solids at all.
    assert_eq!(judged(&dive_at(0.0, 22.0, None), Mode::Area, &[], &tool()), CONVICTED);
    // A non-finite vertex anywhere.
    let mut bad = s.clone();
    if let Some(t) = bad.get_mut(0) {
        t.a.z = f64::NAN;
    }
    assert_eq!(judged(&dive_at(0.0, 22.0, None), Mode::Area, &[&bad], &tool()), CONVICTED);
    // A stack band that is not a number.
    let nan = ToolGeom { holder_radius: f64::NAN, ..tool() };
    assert_eq!(judged(&dive_at(0.0, 22.0, None), Mode::Area, &[&s], &nan), CONVICTED);
}

#[test]
fn only_upright_column_dives_are_re_read() {
    let s = stepped();
    // A posed dive, and the radial / revolved / rotary modes, keep it.
    assert_eq!(judged(&dive_at(0.0, 22.0, Some([0.0, 0.6, 0.8])), Mode::Area, &[&s], &tool()), CONVICTED);
    for mode in [Mode::Radial, Mode::Revolve, Mode::Rotary] {
        assert_eq!(judged(&dive_at(0.0, 22.0, None), mode, &[&s], &tool()), CONVICTED);
    }
    // A lateral rapid is re-read since #1095: this diagonal dive to Z 22
    // over the low step, the boss 7 mm past its end, is clear; posed, it
    // keeps its conviction.
    let mut lateral = dive_at(0.0, 22.0, None);
    if let Some(m) = lateral.moves.get_mut(0) {
        m.to.x = 3.0;
    }
    assert!(judged(&lateral, Mode::Area, &[&s], &tool()).is_empty());
    if let Some(m) = lateral.moves.get_mut(0) {
        m.axis = Some([0.0, 0.6, 0.8]);
    }
    assert_eq!(judged(&lateral, Mode::Area, &[&s], &tool()), CONVICTED);
}

/// An upright lateral rapid from plan point `a` at Z `z0` to `b` at `z1`.
fn link(a: (f64, f64), b: (f64, f64), z0: f64, z1: f64) -> Toolpath {
    let mut t = dive_at(a.0, z1, None);
    if let Some(m) = t.moves.get_mut(0) {
        m.from = Point3::new(a.0, a.1, z0);
        m.to = Point3::new(b.0, b.1, z1);
    }
    t
}

/// A link along X at `y`.
fn traverse(x0: f64, x1: f64, y: f64, z0: f64, z1: f64) -> Toolpath {
    link((x0, y), (x1, y), z0, z1)
}

#[test]
fn a_traverse_over_the_low_step_is_acquitted() {
    // #1095: a link at Z 22 across the low step, 6 mm clear of the boss
    // (x 10..20, y -5..5) all along.
    let s = stepped();
    assert!(judged(&traverse(-20.0, 0.0, 0.0, 22.0, 22.0), Mode::Area, &[&s], &tool()).is_empty());
    // Along Y toward the boss, stopping with the rim 3 mm short of it.
    assert!(judged(&link((15.0, -25.0), (15.0, -12.0), 22.0, 22.0), Mode::Area, &[&s], &tool()).is_empty());
    // Not level: judged at its low end, which is still clear.
    assert!(judged(&traverse(-20.0, 0.0, 0.0, 35.0, 22.0), Mode::Area, &[&s], &tool()).is_empty());
}

#[test]
fn a_traverse_crossing_the_boss_stands() {
    // The same link carried on through the boss.
    let s = stepped();
    assert_eq!(judged(&traverse(-20.0, 30.0, 0.0, 22.0, 22.0), Mode::Area, &[&s], &tool()), CONVICTED);
    // Climbing over it from Z 22 to Z 32: its low end is under the boss
    // top, so it is not proved clear (judged level at its low end).
    assert_eq!(judged(&traverse(0.0, 30.0, 0.0, 22.0, 32.0), Mode::Area, &[&s], &tool()), CONVICTED);
}

#[test]
fn a_boss_under_the_rim_along_the_way_stands() {
    // Along y at x 7: the centre line 3 mm off the boss's edge, the 4 mm
    // rim over it while it passes - and only in the middle of the move.
    let s = stepped();
    assert_eq!(judged(&link((7.0, -25.0), (7.0, 25.0), 22.0, 22.0), Mode::Area, &[&s], &tool()), CONVICTED);
    // At x 5.5 the rim clears it all along.
    assert!(judged(&link((5.5, -25.0), (5.5, 25.0), 22.0, 22.0), Mode::Area, &[&s], &tool()).is_empty());
}

#[test]
fn a_holder_band_over_the_boss_along_the_way_stands() {
    // The stubby stack's holder (radius 16, 6 mm over the tip) sweeps
    // over the boss's top while the flute passes 9 mm beside it.
    let s = stepped();
    let stub = ToolGeom { flute_len: 3.0, shank_len: 3.0, ..tool() };
    let along = link((1.0, -25.0), (1.0, 25.0), 22.0, 22.0);
    assert_eq!(judged(&along, Mode::Area, &[&s], &stub), CONVICTED);
    // The full-length stack's holder stands 50 mm over the tip: clear.
    assert!(judged(&along, Mode::Area, &[&s], &tool()).is_empty());
}

#[test]
fn a_traverse_reads_the_part_and_fails_closed_like_a_dive() {
    let s = stepped();
    // A part to Z 25 under the middle of the link.
    let part = cube(Point3::new(-12.0, -3.0, 0.0), Point3::new(-8.0, 3.0, 25.0)).facets.to_vec();
    assert_eq!(judged(&traverse(-20.0, 0.0, 0.0, 22.0, 22.0), Mode::Area, &[&s, &part], &tool()), CONVICTED);
    // Off the stock: no facet under the flute anywhere, no proof.
    assert_eq!(judged(&traverse(100.0, 120.0, 0.0, 22.0, 22.0), Mode::Area, &[&s], &tool()), CONVICTED);
    // A non-finite end, a posed traverse, and the column-free modes.
    assert_eq!(judged(&traverse(-20.0, f64::NAN, 0.0, 22.0, 22.0), Mode::Area, &[&s], &tool()), CONVICTED);
    let mut posed = traverse(-20.0, 0.0, 0.0, 22.0, 22.0);
    if let Some(m) = posed.moves.get_mut(0) {
        m.axis = Some([0.0, 0.6, 0.8]);
    }
    assert_eq!(judged(&posed, Mode::Area, &[&s], &tool()), CONVICTED);
    for mode in [Mode::Radial, Mode::Revolve, Mode::Rotary] {
        assert_eq!(judged(&traverse(-20.0, 0.0, 0.0, 22.0, 22.0), mode, &[&s], &tool()), CONVICTED);
    }
}

#[test]
fn the_fixtures_are_closed_shells() {
    assert!(closed_shell(&stepped()));
    assert!(closed_shell(&cube(Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 2.0, 3.0)).facets));
}

#[test]
fn an_open_shell_keeps_the_conviction() {
    // The boss without its top: the facets over the dive are missing,
    // and only the slab's floor and the boss's walls stand under it.
    let open = stepped_shell(false);
    assert!(!closed_shell(&open));
    assert_eq!(judged(&dive_at(15.0, 22.0, None), Mode::Area, &[&open], &tool()), CONVICTED);
    // Open too far away to matter for the column - still no proof.
    assert_eq!(judged(&dive_at(0.0, 22.0, None), Mode::Area, &[&open], &tool()), CONVICTED);
    // A closed stock beside an open part is no proof either.
    let s = stepped();
    let mut part = cube(Point3::new(-3.0, -3.0, 0.0), Point3::new(3.0, 3.0, 25.0)).facets.to_vec();
    part.truncate(10);
    assert_eq!(judged(&dive_at(0.0, 22.0, None), Mode::Area, &[&s, &part], &tool()), CONVICTED);
}

#[test]
fn a_non_manifold_or_duplicated_edge_keeps_the_conviction() {
    // A fin: one extra facet on the slab's top edge - three facets there.
    let mut fin = stepped();
    fin.push(Triangle::new(Point3::new(-30.0, -30.0, 20.0), Point3::new(30.0, -30.0, 20.0), Point3::new(0.0, -30.0, 40.0)));
    assert!(!closed_shell(&fin));
    assert_eq!(judged(&dive_at(0.0, 22.0, None), Mode::Area, &[&fin], &tool()), CONVICTED);
    // A facet listed twice.
    let mut twice = stepped();
    if let Some(t) = twice.first().cloned() {
        twice.push(t);
    }
    assert!(!closed_shell(&twice));
    assert_eq!(judged(&dive_at(0.0, 22.0, None), Mode::Area, &[&twice], &tool()), CONVICTED);
    // A degenerate (zero-area) facet.
    let mut flat = stepped();
    flat.push(Triangle::new(Point3::new(0.0, 0.0, 20.0), Point3::new(1.0, 0.0, 20.0), Point3::new(2.0, 0.0, 20.0)));
    assert!(!closed_shell(&flat));
}

#[test]
fn an_inside_out_or_split_solid_keeps_the_conviction() {
    let flipped: Vec<Triangle> = stepped().iter().map(|t| Triangle::new(t.a, t.c, t.b)).collect();
    assert!(!closed_shell(&flipped));
    assert_eq!(judged(&dive_at(0.0, 22.0, None), Mode::Area, &[&flipped], &tool()), CONVICTED);
    // Two closed boxes handed as one solid.
    let mut two = cube(Point3::new(-30.0, -30.0, -10.0), Point3::new(30.0, 30.0, 20.0)).facets.to_vec();
    two.extend(cube(Point3::new(10.0, -5.0, 20.0), Point3::new(20.0, 5.0, 30.0)).facets.iter().cloned());
    assert!(!closed_shell(&two));
    assert_eq!(judged(&dive_at(0.0, 22.0, None), Mode::Area, &[&two], &tool()), CONVICTED);
}

#[test]
fn an_empty_or_non_finite_solid_keeps_the_conviction() {
    let s = stepped();
    let empty: Vec<Triangle> = Vec::new();
    assert_eq!(judged(&dive_at(0.0, 22.0, None), Mode::Area, &[&s, &empty], &tool()), CONVICTED);
    let mut bad = s.clone();
    if let Some(t) = bad.last_mut() {
        t.b.x = f64::INFINITY;
    }
    assert!(!closed_shell(&bad));
    assert_eq!(judged(&dive_at(0.0, 22.0, None), Mode::Area, &[&bad], &tool()), CONVICTED);
}

fn judged_coarse(path: &Toolpath, solids: &[&[Triangle]], excess: f64) -> Vec<Warning> {
    let mut w = vec![Warning::RapidInStock { at: 0 }];
    acquit_by_stock_top(path, Mode::Area, solids, &tool(), 1e-3, excess, &mut w);
    w
}

#[test]
fn a_coarse_models_excess_is_read_up_and_sideways() {
    let s = stepped();
    // 0.3 mm over the meshed low step: clear at full fidelity, not when
    // the real surface may stand 0.5 mm above its facets.
    assert!(judged_coarse(&dive_at(0.0, 20.3, None), &[&s], 0.0).is_empty());
    assert_eq!(judged_coarse(&dive_at(0.0, 20.3, None), &[&s], 0.5), CONVICTED);
    // The rim 0.3 mm short of the meshed boss's wall: clear at full
    // fidelity, not with 0.5 mm of excess.
    assert!(judged_coarse(&dive_at(5.7, 22.0, None), &[&s], 0.0).is_empty());
    assert_eq!(judged_coarse(&dive_at(5.7, 22.0, None), &[&s], 0.5), CONVICTED);
    // An excess that is not a number keeps it; a negative one is none.
    assert_eq!(judged_coarse(&dive_at(0.0, 22.0, None), &[&s], f64::NAN), CONVICTED);
    assert!(judged_coarse(&dive_at(0.0, 22.0, None), &[&s], -1.0).is_empty());
}


#[test]
fn a_traverse_takes_the_coarse_models_excess_like_a_dive() {
    // #1095 review: the lateral path takes the dive's excess rules.
    let s = stepped();
    // Up: a link 0.3 mm over the meshed low step.
    let low = traverse(-20.0, 0.0, 0.0, 20.3, 20.3);
    assert!(judged_coarse(&low, &[&s], 0.0).is_empty());
    assert_eq!(judged_coarse(&low, &[&s], 0.5), CONVICTED);
    // Sideways: along y at x 5.7, the rim 0.3 mm short of the boss's wall.
    let beside = link((5.7, -25.0), (5.7, 25.0), 22.0, 22.0);
    assert!(judged_coarse(&beside, &[&s], 0.0).is_empty());
    assert_eq!(judged_coarse(&beside, &[&s], 0.5), CONVICTED);
    // A non-finite excess keeps a lateral conviction too.
    let clear = traverse(-20.0, 0.0, 0.0, 22.0, 22.0);
    assert_eq!(judged_coarse(&clear, &[&s], f64::NAN), CONVICTED);
}
