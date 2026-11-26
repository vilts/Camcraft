//! The `gap_link` chain law: two runs of a finish raster broken by an
//! opening are chained by the hop to retract/clearance (what every
//! pre-gap-link file ran), or by one straight link between the run
//! ends where the exact sweep certifies it. A holed plate pins both
//! authors, the refusals that keep the hop, and the ball's clearance
//! on every link either author writes.
//!
//! Gate map (gap_link): covers camcraft_motion::finishing3d (the
//! straight-link verdict, march chain, raster serpentine order).

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::finishing3d::{parallel, Finish3DParams, FinishPattern, GapLink};
use camcraft_motion::geometry::{Point3, Triangle};

const R: f64 = 1.5;
const TOL: f64 = 0.05;
const TOP: f64 = 8.0;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn quad(out: &mut Vec<Triangle>, p: [Point3; 4]) {
    out.push(Triangle::new(p[0], p[1], p[2]));
    out.push(Triangle::new(p[0], p[2], p[3]));
}

/// A stepped block on an XY grid of cells: each cell stands to its
/// height (`None` = through), with top, bottom and every wall where a
/// neighbour (or the outside) stands lower.
fn block(xs: &[f64], ys: &[f64], h: &dyn Fn(usize, usize) -> Option<f64>) -> Vec<Triangle> {
    let (nx, ny) = (xs.len() - 1, ys.len() - 1);
    let at = |i: isize, j: isize| {
        if i < 0 || j < 0 || i >= nx as isize || j >= ny as isize {
            None
        } else {
            h(i as usize, j as usize)
        }
    };
    let mut f = Vec::new();
    for j in 0..ny {
        for i in 0..nx {
            let Some(z) = h(i, j) else { continue };
            let (x0, x1, y0, y1) = (xs[i], xs[i + 1], ys[j], ys[j + 1]);
            quad(&mut f, [p3(x0, y0, z), p3(x1, y0, z), p3(x1, y1, z), p3(x0, y1, z)]);
            quad(&mut f, [p3(x0, y0, 0.0), p3(x0, y1, 0.0), p3(x1, y1, 0.0), p3(x1, y0, 0.0)]);
            let (ii, jj) = (i as isize, j as isize);
            let sides = [
                (at(ii - 1, jj), [p3(x0, y1, 0.0), p3(x0, y0, 0.0)]),
                (at(ii + 1, jj), [p3(x1, y0, 0.0), p3(x1, y1, 0.0)]),
                (at(ii, jj - 1), [p3(x0, y0, 0.0), p3(x1, y0, 0.0)]),
                (at(ii, jj + 1), [p3(x1, y1, 0.0), p3(x0, y1, 0.0)]),
            ];
            for (n, [a, b]) in sides {
                let lo = n.unwrap_or(0.0);
                if lo < z {
                    let (a0, b0) = (p3(a.x, a.y, lo), p3(b.x, b.y, lo));
                    quad(&mut f, [a0, b0, p3(b.x, b.y, z), p3(a.x, a.y, z)]);
                }
            }
        }
    }
    f
}

/// 40 x 20 x 8 plate, two 3.4 mm square through holes centred at
/// (10, 10) and (30, 10) - a 3 mm ball fits inside each.
fn holed_plate() -> Vec<Triangle> {
    let xs = [0.0, 8.3, 11.7, 28.3, 31.7, 40.0];
    let ys = [0.0, 8.3, 11.7, 20.0];
    block(&xs, &ys, &|i, j| (!(j == 1 && (i == 1 || i == 3))).then_some(TOP))
}

fn fp(gap_link: GapLink, retract_z: Option<f64>, clearance_z: f64) -> Finish3DParams {
    Finish3DParams {
        gap_link,
        pattern: FinishPattern::Parallel,
        stepover: None,
        tool_diameter: 2.0 * R,
        tolerance: TOL,
        clearance_z,
        retract_z,
        top_z: TOP,
        bottom_z: 0.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
        waterlines: true,
    }
}

/// The distinct z heights of every horizontal rapid (a traverse: both
/// ends share a z) whose X or Y span is non-degenerate.
fn traverse_heights(t: &Toolpath) -> Vec<f64> {
    let mut out: Vec<f64> = Vec::new();
    for m in &t.moves {
        if m.kind != MoveKind::Rapid || (m.from.z - m.to.z).abs() > 1e-9 {
            continue;
        }
        if (m.from.x - m.to.x).abs() < 1e-9 && (m.from.y - m.to.y).abs() < 1e-9 {
            continue;
        }
        out.push(m.from.z);
    }
    out.sort_by(f64::total_cmp);
    out.dedup();
    out
}

#[test]
fn jump_chains_at_the_retract_plane() {
    // What every pre-gap-link file ran, pinned: the chain between the
    // runs the holes break goes to `retract_z`, and nothing crosses
    // between the rim and that plane.
    let f = holed_plate();
    let t = parallel(&f, &fp(GapLink::Jump, Some(12.0), 20.0)).expect("jump path");
    let h = traverse_heights(&t);
    assert_eq!(h, vec![12.0, 20.0], "the hop goes to retract within a level and clearance between levels");
}

/// A two-step stair with a through slot across the step: the high
/// top at 8 for x < 9.6, the low top at 4 for x > 13, and a 3.4 mm
/// through slot [9.6, 13] between them - the step's high wall
/// (z 4..8) stands at the slot's far opening, so the raster line
/// breaks across the slot and a straight link from the high rim's
/// tangency (tip 8) to the low tangency (tip 4) runs its ball into
/// that wall. The facets certify a crossing only above the high rim.
fn slot_over_step() -> Vec<Triangle> {
    let xs = [0.0, 9.6, 13.0, 40.0];
    let ys = [0.0, 20.0];
    block(&xs, &ys, &|i, _j| match i {
        0 => Some(TOP),
        1 => None,
        _ => Some(4.0),
    })
}

/// XY span of a move.
fn span(m: &camcraft_motion::cl::Move) -> f64 {
    ((m.to.x - m.from.x).powi(2) + (m.to.y - m.from.y).powi(2)).sqrt()
}

#[test]
fn straight_links_holes_at_the_run_height() {
    // The smoothest author the facets certify erases the hop over
    // the holes entirely: one feed link from rim to rim at the
    // runs' own tip height. The rim rows off the plate's outside
    // keep their hop: there the march itself rests the ball about
    // 0.005 mm into the flank inside exact tangency (its own chord
    // budget), and the verdict law refuses a link whose ends bite
    // the flank - certified or not at all. Only the pass-to-pass
    // climb to clearance rides high off the edge.
    let f = holed_plate();
    let t = parallel(&f, &fp(GapLink::Straight, Some(12.0), 20.0)).expect("straight path");
    assert_eq!(
        hop_over_a_hole(&t, 12.0),
        0,
        "a retract hop still crosses a hole"
    );
    let links = t
        .moves
        .iter()
        .filter(|m| {
            m.kind == MoveKind::Feed
                && span(m) > 2.0
                && (m.from.z - TOP).abs() < 1e-6
                && (m.to.z - TOP).abs() < 1e-6
        })
        .count();
    assert!(links >= 4, "no rim-to-rim links over the holes: {links}");
}

#[test]
fn jump_mode_still_hops_over_the_holes() {
    // The negative control: on the same fixture the plain author
    // does hop each hole, so the pin above proves the straight
    // author, not a fixture with no holes to cross.
    let f = holed_plate();
    let t = parallel(&f, &fp(GapLink::Jump, Some(12.0), 20.0)).expect("jump path");
    assert!(
        hop_over_a_hole(&t, 12.0) >= 2,
        "the plain author lost its hop over the holes"
    );
}

#[test]
fn straight_links_keep_the_ball_out_of_the_plate() {
    // The Prime Directive on the moves the straight link authors:
    // every link (a feed move longer than any march chord on this
    // fixture, at the run ends' own tip height) keeps the swept
    // ball clear of every facet within float's tangent slop of `r`
    // - the budget the certification itself answers on. The march's
    // corner-rolling arcs are its own authorship, identical bytes
    // under every gap link and budgeted by the verify laws; the
    // tip-height filter keeps them out of this one.
    let f = holed_plate();
    let t = parallel(&f, &fp(GapLink::Straight, Some(12.0), 20.0)).expect("straight path");
    for (i, m) in t.moves.iter().enumerate() {
        if m.kind != MoveKind::Feed
            || span(m) < 2.0
            || (m.from.z - TOP).abs() > 1e-6
            || (m.to.z - TOP).abs() > 1e-6
        {
            continue;
        }
        for (k, tri) in f.iter().enumerate() {
            let a = Point3::new(m.from.x, m.from.y, m.from.z + R);
            let b = Point3::new(m.to.x, m.to.y, m.to.z + R);
            let d = camcraft_motion::stock_sim::dist_seg_tri(a, b, tri);
            if d < R - 1e-6 {
                panic!(
                    "straight link {i} {:?} -> {:?} bites facet {k} a={:?} d={d}",
                    m.from, m.to, tri.a
                );
            }
        }
    }
}

/// Horizontal rapids at exactly `z` that CROSS the slot (the band
/// x = 9.6 .. 13) - the slot crossings' authored traverses. Jogs
/// along one level's own top never enter this band.
fn crossing_hops(t: &Toolpath, z: f64) -> Vec<(Point3, Point3)> {
    t.moves
        .iter()
        .filter(|m| {
            m.kind == MoveKind::Rapid
                && (m.from.z - z).abs() < 1e-9
                && (m.to.z - z).abs() < 1e-9
                && m.from.x.min(m.to.x) < 10.0
                && m.from.x.max(m.to.x) > 10.0
        })
        .map(|m| (m.from, m.to))
        .collect()
}

/// Horizontal rapids at exactly `z` that CROSS a hole centre line
/// (x = 10 or x = 30) within that hole's own y band (8.3 .. 11.7) -
/// the holes' authored retract hops. Rim rows off the plate never
/// enter this band.
fn hop_over_a_hole(t: &Toolpath, z: f64) -> usize {
    t.moves
        .iter()
        .filter(|m| {
            m.kind == MoveKind::Rapid
                && (m.from.z - z).abs() < 1e-9
                && (m.to.z - z).abs() < 1e-9
                && (m.from.y - m.to.y).abs() < 1e-9
                && m.from.y > 8.3
                && m.from.y < 11.7
                && ((m.from.x.min(m.to.x) < 10.0 && m.from.x.max(m.to.x) > 10.0)
                    || (m.from.x.min(m.to.x) < 30.0 && m.from.x.max(m.to.x) > 30.0))
        })
        .count()
}

#[test]
#[ignore = "probe: dump traverses above z 6 on the slot fixture, run with -- --ignored --nocapture"]
fn probe_slot_crossings() {
    let f = slot_over_step();
    for gl in [GapLink::Straight, GapLink::Jump] {
        let t = parallel(&f, &fp(gl, Some(12.0), 20.0)).expect("path");
        println!("== {gl:?} ==");
        for (i, m) in t.moves.iter().enumerate() {
            if m.kind != MoveKind::Rapid || (m.from.z - m.to.z).abs() > 1e-9 || m.from.z < 6.0 {
                continue;
            }
            println!(
                "{i} ({:.3},{:.3},{:.4}) -> ({:.3},{:.3},{:.4})",
                m.from.x, m.from.y, m.from.z, m.to.x, m.to.y, m.to.z
            );
        }
    }
}

#[test]
fn straight_refused_over_a_biting_slot_keeps_the_hop() {
    // Where the straight line would bite - the slot crossing drops
    // the ball into the step's high wall standing in the opening -
    // it is refused and the plain hop stands: the retract plane at
    // 12, byte for byte as the `Jump` answer every pre-gap-link file
    // wrote. The runs are the same path under both authors, so every
    // slot crossing has the same endpoints and the crossings' hops
    // must match exactly.
    let f = slot_over_step();
    let j = parallel(&f, &fp(GapLink::Jump, Some(12.0), 20.0)).expect("jump path");
    let st = parallel(&f, &fp(GapLink::Straight, Some(12.0), 20.0)).expect("straight path");
    let (cj, cs) = (crossing_hops(&j, 12.0), crossing_hops(&st, 12.0));
    assert!(!cs.is_empty(), "no hop at 12 across the slot");
    assert_eq!(cj, cs, "a refused straight link keeps the plain hop");
}

#[test]
fn straight_refused_with_no_saving_below_keeps_the_jump_bytes() {
    // The same crossings with the hop plane AT the high top: the
    // straight line bites the wall and is refused, so those
    // crossings read byte for byte as the `Jump` answer: same
    // endpoints (the runs are identical), same plane.
    let f = slot_over_step();
    let j = parallel(&f, &fp(GapLink::Jump, None, TOP)).expect("jump path");
    let st = parallel(&f, &fp(GapLink::Straight, None, TOP)).expect("straight path");
    let (jb, sb) = (crossing_hops(&j, TOP), crossing_hops(&st, TOP));
    assert!(!sb.is_empty(), "no hop crossing at {TOP}");
    assert_eq!(jb, sb, "the straight link refused: the hop stands");
    // And the Prime Directive: no feed move crosses the slot band
    // below the high top - a link through it would cut the wall
    // the refusal saw. Only hops and plunges may cross there.
    for (i, m) in st.moves.iter().enumerate() {
        if m.kind != MoveKind::Feed {
            continue;
        }
        let (xlo, xhi) = (m.from.x.min(m.to.x), m.from.x.max(m.to.x));
        let zlo = m.from.z.min(m.to.z);
        if xlo < 9.4 && xhi > 12.8 && zlo < TOP - 0.5 {
            panic!("feed {i} {:?} -> {:?} crosses the slot", m.from, m.to);
        }
    }
}

#[test]
fn the_back_run_of_a_broken_line_crosses_its_gaps_not_the_field() {
    // A raster line broken by two holes is three pieces. Serpentine
    // runs the line back WHOLE - pieces in reverse order and each
    // piece's points reversed - so every chain move crosses one gap
    // between the facing edges. Reversing only the points kept the
    // pieces in grid order: the chain ran one piece backwards, then
    // jumped back across the whole field to the next piece's far
    // end. Under the plain lift that mis-pairing hid in a high
    // rapid; a gap link puts the same line where it shows. Pin:
    // every hop traverse the raster writes (a horizontal rapid
    // below clearance) spans one gap, never the field.
    let f = holed_plate();
    for gl in [GapLink::Jump, GapLink::Straight] {
        let t = parallel(&f, &fp(gl, Some(12.0), 30.0)).expect("path");
        let tag = format!("{gl:?}");
        for (i, m) in t.moves.iter().enumerate() {
            if m.kind != MoveKind::Rapid
                || (m.from.z - m.to.z).abs() > 1e-9
                || m.from.z > 25.0
            {
                continue; // climbs at clearance and the first entry are not gap crossings
            }
            let s = span(m);
            assert!(
                s < 6.0,
                "{tag}: hop traverse {i} {:?} -> {:?} spans {s:.3} - the chain jumped the field, not the gap",
                m.from,
                m.to
            );
        }
    }
}
