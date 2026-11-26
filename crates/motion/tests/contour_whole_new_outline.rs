//! #2597 pins for whole-model 2D Contour: an outline that grows below Top Z
//! is cut from Top Z down in steps no deeper than the stepdown - never with a
//! first pass buried at the level where the outline appeared. Each test reads
//! the Z of every cutting pass, outline by outline.
//!
//! Gate map: covers camcraft_motion::machining2d (contour_whole, contour_whole_reach, contour_whole_fence).
//!
//! Provenance: our own extruded manifold test meshes; no vendor material.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geo2d::Region;
use camcraft_motion::geometry::{Point3, Triangle, slice};
use camcraft_motion::machining2d::{ContourParams, Lead, Side, contour_whole_bodies};
use camcraft_motion::machining3d::Entry3D;
use camcraft_motion::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

fn quad(f: &mut Vec<Triangle>, a: Point3, b: Point3, c: Point3, d: Point3) {
    f.push(Triangle::new(a, b, c));
    f.push(Triangle::new(a, c, d));
}

/// Cap `outer` at `z` (with the ring to `inner` when given), facing up or down.
fn cap(f: &mut Vec<Triangle>, outer: &[(f64, f64); 4], inner: Option<&[(f64, f64); 4]>, z: f64, up: bool) {
    let p = |q: (f64, f64)| Point3::new(q.0, q.1, z);
    let mut push = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| {
        if up {
            f.push(Triangle::new(p(a), p(b), p(c)));
        } else {
            f.push(Triangle::new(p(a), p(c), p(b)));
        }
    };
    match inner {
        None => {
            push(outer[0], outer[1], outer[2]);
            push(outer[0], outer[2], outer[3]);
        }
        Some(inn) => {
            for k in 0..4 {
                let n = (k + 1) % 4;
                push(outer[k], outer[n], inn[n]);
                push(outer[k], inn[n], inn[k]);
            }
        }
    }
}

fn walls(f: &mut Vec<Triangle>, xy: &[(f64, f64); 4], z0: f64, z1: f64) {
    for k in 0..4 {
        let (a, b) = (xy[k], xy[(k + 1) % 4]);
        quad(f, Point3::new(a.0, a.1, z0), Point3::new(b.0, b.1, z0), Point3::new(b.0, b.1, z1), Point3::new(a.0, a.1, z1));
    }
}

fn square(h: f64) -> [(f64, f64); 4] {
    [(-h, -h), (h, -h), (h, h), (-h, h)]
}

/// A centred stack of square steps, `(half width, top Z)` from the bottom up;
/// the lowest step stands on Z 0.
fn stepped(steps: &[(f64, f64)]) -> Vec<Triangle> {
    let mut f = Vec::new();
    let mut z0 = 0.0;
    cap(&mut f, &square(steps[0].0), None, 0.0, false);
    for (k, &(h, top)) in steps.iter().enumerate() {
        walls(&mut f, &square(h), z0, top);
        match steps.get(k + 1) {
            Some(&(inner, _)) => cap(&mut f, &square(h), Some(&square(inner)), top, true),
            None => cap(&mut f, &square(h), None, top, true),
        }
        z0 = top;
    }
    f
}

fn params(top: f64, stepdown: f64, tool_diameter: f64) -> ContourParams {
    ContourParams {
        tool_diameter,
        stock_to_leave: 0.0,
        clearance_z: top + 10.0,
        retract_z: top + 5.0,
        top_z: top,
        bottom_z: 0.0,
        stepdown,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false, blend_radius: None,
        links: camcraft_motion::links::Links::PLANAR,
        picks: Default::default(),
    }
}

fn depth_verify(path: &Toolpath, region: &Region, f: &[Triangle], top: f64, r: f64) -> Vec<Warning> {
    let tool = ToolGeom {
        radius: r,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: r,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    };
    verify(
        path,
        &VerifyContext {
            region,
            stock: None,
            model: Some(f),
            tool: &tool,
            cutter: None,
            mode: Mode::Boundary,
            stock_top_z: top,
            clearance_z: top + 10.0,
            stock_to_leave: 0.0,
            gouge_tol: 0.01,
            z_tol: 1e-6,
            finish_tol: None,
            fixtures: &[],
            spindle: None,
            ..Default::default()
        },
    )
}

/// The Z of every level-plane cutting pass on the ring around the square of
/// half width `h` (its straight sides sit at `h + r`), descending.
fn ring_levels(path: &Toolpath, h: f64, r: f64) -> Vec<f64> {
    let side = h + r;
    let mut zs: Vec<f64> = path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && (m.from.z - m.to.z).abs() < 1e-9)
        .filter(|m| {
            let on = |x: f64, y: f64| (x.abs() - side).abs() < 1e-3 || (y.abs() - side).abs() < 1e-3;
            let inside = |x: f64, y: f64| x.abs() <= side + 1e-3 && y.abs() <= side + 1e-3;
            on(m.from.x, m.from.y) && on(m.to.x, m.to.y) && inside(m.from.x, m.from.y) && inside(m.to.x, m.to.y)
        })
        .map(|m| m.to.z)
        .collect();
    zs.sort_by(|a, b| b.total_cmp(a));
    zs.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    zs
}

/// Every pass of the ring is at most one stepdown under the one before it,
/// the first at most one stepdown under Top Z; the ring reaches `floor`.
fn assert_stepped(levels: &[f64], top: f64, stepdown: f64, floor: f64, what: &str) {
    assert!(!levels.is_empty(), "{what}: no pass");
    let mut above = top;
    for &z in levels {
        assert!(above - z <= stepdown + 1e-6, "{what}: pass at Z {z} is {:.3} under Z {above} (levels {levels:?})", above - z);
        above = z;
    }
    assert!((above - floor).abs() < 1e-6, "{what}: ends at Z {above}, not Z {floor} (levels {levels:?})");
}

#[test]
fn new_larger_outline_below_top_z_steps_down_from_top_z() {
    // The #2597 part: base 60 x 60 Z 0..10, boss 30 x 30 Z 10..25; D10,
    // stepdown 5, Top Z 25. The base outline appears at Z 10 - its first
    // pass must not be 20 mm under Top Z.
    let f = stepped(&[(30.0, 10.0), (15.0, 25.0)]);
    let (top, step, r) = (25.0, 5.0, 5.0);
    let top_reg = slice::slice(&f, top - 1e-3).expect("top slice");
    let path = contour_whole_bodies(&top_reg, &f, &[], &params(top, step, 2.0 * r)).expect("whole contour");
    assert!(path.all_finite() && path.is_chained());
    let boss = ring_levels(&path, 15.0, r);
    let base = ring_levels(&path, 30.0, r);
    assert_stepped(&boss, top, step, 10.0, "boss ring");
    assert_stepped(&base, top, step, 0.0, "base ring");
    assert_eq!(depth_verify(&path, &top_reg, &f, top, r), Vec::new(), "{:?}", path.notes);
}

#[test]
fn every_new_outline_of_three_steps_steps_down_from_top_z() {
    // Three outlines: the middle one appears between flat levels (its floor
    // pass at Z 10), the base at Z 10. Each steps down from Top Z 30.
    let f = stepped(&[(40.0, 10.0), (25.0, 20.0), (10.0, 30.0)]);
    let (top, step, r) = (30.0, 4.0, 3.0);
    let top_reg = slice::slice(&f, top - 1e-3).expect("top slice");
    let path = contour_whole_bodies(&top_reg, &f, &[], &params(top, step, 2.0 * r)).expect("whole contour");
    assert!(path.all_finite() && path.is_chained());
    for (h, floor, what) in [(10.0, 20.0, "boss ring"), (25.0, 10.0, "middle ring"), (40.0, 0.0, "base ring")] {
        assert_stepped(&ring_levels(&path, h, r), top, step, floor, what);
    }
    assert_eq!(depth_verify(&path, &top_reg, &f, top, r), Vec::new(), "{:?}", path.notes);
}

#[test]
fn new_outline_steps_down_from_a_stock_top_over_top_z() {
    // The #2597 part with the stock top 3 mm over Top Z: Top Z then cuts
    // material, so a new outline takes a pass at Top Z too - its first pass
    // is at most one stepdown under the stock top, not 8 mm.
    let f = stepped(&[(30.0, 10.0), (15.0, 25.0)]);
    let (top, step, r, stock) = (25.0, 5.0, 5.0, 28.0);
    let top_reg = slice::slice(&f, top - 1e-3).expect("top slice");
    let mut p = params(top, step, 2.0 * r);
    p.links.approach.stock_top = Some(stock);
    let path = contour_whole_bodies(&top_reg, &f, &[], &p).expect("whole contour");
    assert!(path.all_finite() && path.is_chained());
    assert_stepped(&ring_levels(&path, 15.0, r), stock, step, 10.0, "boss ring");
    assert_stepped(&ring_levels(&path, 30.0, r), stock, step, 0.0, "base ring");
    assert_eq!(depth_verify(&path, &top_reg, &f, stock, r), Vec::new(), "{:?}", path.notes);
}

#[test]
fn new_outline_takes_top_z_when_the_stock_stands_a_stepdown_over_it() {
    // Stock top 31, 6 mm over Top Z 25 (stepdown 5): band 0 bites 6 mm at
    // Top Z, and only a pass at Top Z keeps the base ring's first bite as
    // small - so the base ring takes Top Z and the flat levels under it.
    let f = stepped(&[(30.0, 10.0), (15.0, 25.0)]);
    let (top, step, r) = (25.0, 5.0, 5.0);
    let top_reg = slice::slice(&f, top - 1e-3).expect("top slice");
    let mut p = params(top, step, 2.0 * r);
    p.links.approach.stock_top = Some(31.0);
    let path = contour_whole_bodies(&top_reg, &f, &[], &p).expect("whole contour");
    assert_eq!(ring_levels(&path, 15.0, r), vec![25.0, 20.0, 15.0, 10.0], "boss ring");
    assert_eq!(ring_levels(&path, 30.0, r), vec![25.0, 20.0, 15.0, 10.0, 5.0, 0.0], "base ring");
    assert_eq!(depth_verify(&path, &top_reg, &f, 31.0, r), Vec::new(), "{:?}", path.notes);
}

#[test]
fn leads_of_one_outline_do_not_cross_a_grown_outline_on_the_same_level() {
    // #2677: the base grows 1 mm out of the boss, so the base ring cuts the
    // boss ring's levels from the top down, 1 mm outside it. An 8 mm linear
    // lead of either ring must not cross the other ring on a level that
    // they share (the verifier's Crossing law blocks the post). With the
    // stock top 31 (a stepdown and more over Top Z 25) the base ring takes
    // Top Z, as in `contour_whole_step` case 1 at stock top 18.5.
    let f = stepped(&[(16.0, 10.0), (15.0, 25.0)]);
    let (top, step, r) = (25.0, 5.0, 5.0);
    let top_reg = slice::slice(&f, top - 1e-3).expect("top slice");
    for stock in [None, Some(31.0)] {
        let mut p = params(top, step, 2.0 * r);
        p.lead = Lead::Linear { length: 8.0 };
        p.links.approach.stock_top = stock;
        let path = contour_whole_bodies(&top_reg, &f, &[], &p).expect("whole contour");
        assert!(path.all_finite() && path.is_chained());
        let from = stock.unwrap_or(top);
        assert_stepped(&ring_levels(&path, 15.0, r), from, step.max(from - top), 10.0, "boss ring");
        assert_stepped(&ring_levels(&path, 16.0, r), from, step.max(from - top), 0.0, "base ring");
        let warns = depth_verify(&path, &top_reg, &f, from, r);
        assert_eq!(warns, Vec::new(), "stock {stock:?}: {:?}", path.notes);
    }
}

#[test]
fn an_outline_that_does_not_grow_is_not_walked_again_above() {
    // A plain 40 x 40 block: one outline at every level, so every ring keeps
    // one pass per flat level - nothing is re-cut from Top Z.
    let f = stepped(&[(20.0, 20.0)]);
    let (top, step, r) = (20.0, 5.0, 3.0);
    let top_reg = slice::slice(&f, top - 1e-3).expect("top slice");
    let path = contour_whole_bodies(&top_reg, &f, &[], &params(top, step, 2.0 * r)).expect("whole contour");
    assert_eq!(ring_levels(&path, 20.0, r), vec![15.0, 10.0, 5.0, 0.0]);
}
