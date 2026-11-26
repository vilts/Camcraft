//! Walk each pass of a mirrored path backwards, so the mirror keeps the
//! climb cut (#3011, `MirrorMode::PreserveClimb`).
//!
//! Provenance: climb versus conventional milling (general machining
//! practice); a reflection reverses the cut sense and reversing the
//! travel reverses it back. Our own model; no vendor material consulted.
//!
//! The path must read as passes the reversal cannot make unsafe:
//!
//! - an entry column: straight down at one XY, rapids first, then feeds;
//! - a planar cut: feeds at one Z, straight or XY arcs, whose last move
//!   (the lead-out) has the shape and the length of its first move (the
//!   lead-in), so the reversed plunge stands as far off the contour;
//! - an exit column: straight up at one XY, feeds first, then rapids,
//!   back to the entry column's top and with the same rapid-to-feed
//!   height, so the column profile is the same on both ends;
//! - between passes: rapids on that one clearance plane.
//!
//! Each planar cut runs backwards (the lead-out becomes the lead-in).
//! The entry column moves to the old cut end and the exit column to the
//! old cut start, with the same heights, kinds and feeds. The clearance
//! links are re-drawn on the same plane. Anything else - a ramp or
//! helical entry, a level change without a retract, a link off the
//! clearance plane, a canned cycle - is refused by name.

use crate::geometry::Point3;

use super::super::comp::CompSpan;
use super::super::moves::{ArcMove, Move, MoveKind};
use super::super::path::Toolpath;
use super::{swap_side, TransformError};

fn refuse<T>(why: &str) -> Result<T, TransformError> {
    Err(TransformError::NotReversible(why.into()))
}

fn vertical(m: &Move) -> bool {
    m.arc.is_none() && m.from.x == m.to.x && m.from.y == m.to.y && m.from.z != m.to.z
}

fn down(m: &Move) -> bool {
    vertical(m) && m.to.z < m.from.z
}

fn up(m: &Move) -> bool {
    vertical(m) && m.to.z > m.from.z
}

/// A horizontal rapid on the plane `z`.
fn clearance(m: &Move, z: f64) -> bool {
    m.kind == MoveKind::Rapid && m.arc.is_none() && m.from.z == z && m.to.z == z
}

/// One pass: entry column `d..c`, planar cut `c..a`, exit column `a..e`.
struct Pass {
    d: usize,
    c: usize,
    a: usize,
    e: usize,
}

/// The height where a column switches between rapid and feed: the
/// lowest rapid point. The column's top when it has no rapid.
fn rapid_floor(col: &[Move], top: f64) -> f64 {
    col.iter().filter(|m| m.kind == MoveKind::Rapid).map(|m| m.from.z.min(m.to.z)).fold(top, f64::min)
}

/// Rapids only at the top of a column, never below a feed.
fn rapids_on_top(col: &[Move]) -> bool {
    let mut zs: Vec<(f64, bool)> = col.iter().map(|m| (m.from.z.max(m.to.z), m.kind == MoveKind::Rapid)).collect();
    zs.sort_by(|p, q| q.0.total_cmp(&p.0));
    zs.windows(2).all(|w| w[0].1 || !w[1].1)
}

fn read_passes(ms: &[Move]) -> Result<Vec<Pass>, TransformError> {
    let mut passes = Vec::new();
    let mut i = 0;
    while let Some(d) = (i..ms.len()).find(|&j| down(&ms[j])) {
        let c = (d..ms.len()).find(|&j| !down(&ms[j])).unwrap_or(ms.len());
        let level = ms[c - 1].to.z;
        let planar = |m: &Move| m.kind != MoveKind::Rapid && m.from.z == level && m.to.z == level
            && m.arc.is_none_or(|a| a.plane.is_xy());
        let a = (c..ms.len()).find(|&j| !planar(&ms[j])).unwrap_or(ms.len());
        if a == c {
            return refuse("the cut does not start with a planar move at the entry depth (a ramp or helical entry)");
        }
        if !leads_match(&ms[c], &ms[a - 1]) {
            return refuse(
                "the pass has no lead-out that matches its lead-in: reversed, it would plunge at the old cut end, on the contour",
            );
        }
        let e = (a..ms.len()).find(|&j| !up(&ms[j])).unwrap_or(ms.len());
        if e == a {
            return refuse("a pass leaves its cut level without a straight retract (a ramp, or a level change)");
        }
        let (entry, exit) = (&ms[d..c], &ms[a..e]);
        let top = ms[d].from.z;
        if ms[e - 1].to.z != top
            || !rapids_on_top(entry)
            || !rapids_on_top(exit)
            || rapid_floor(entry, top) != rapid_floor(exit, top)
        {
            return refuse("the entry and the exit columns of a pass differ in height or in rapid travel");
        }
        passes.push(Pass { d, c, a, e });
        i = e;
    }
    if passes.is_empty() {
        return refuse("the path has no cutting pass between an entry and a retract");
    }
    Ok(passes)
}

/// The path length of a move, over chords of 0.1 um sagitta.
fn length(m: &Move) -> f64 {
    m.segments3(1e-4).iter().map(|&(p, q)| (q - p).dot(q - p).sqrt()).sum()
}

/// The reversed pass plunges where the old pass left its cut, so the
/// exit must stand off the contour as far as the entry did. The cl
/// layer cannot tell a lead from the contour, so the law asks for
/// symmetry: the first and the last cut move are of one shape (straight
/// or arc) and one length. A single-move cut matches itself.
fn leads_match(first: &Move, last: &Move) -> bool {
    first.arc.is_some() == last.arc.is_some() && (length(first) - length(last)).abs() <= 2e-3
}

/// The move run backwards: from and to swap, an arc turns the other way.
fn reversed(m: &Move) -> Move {
    Move { from: m.to, to: m.from, arc: m.arc.map(|a| ArcMove { ccw: !a.ccw, ..a }), ..*m }
}

/// The column moved to stand at `at`'s XY, heights kept.
fn column_at(col: &[Move], at: Point3) -> impl Iterator<Item = Move> + '_ {
    col.iter().map(move |m| Move {
        from: Point3::new(at.x, at.y, m.from.z),
        to: Point3::new(at.x, at.y, m.to.z),
        ..*m
    })
}

fn rapid(from: Point3, to: Point3) -> Option<Move> {
    (from != to).then(|| Move::new(MoveKind::Rapid, from, to, None))
}

/// Reverse every pass of the mirrored path `tp`.
pub(super) fn reverse_passes(tp: Toolpath) -> Result<Toolpath, TransformError> {
    if !tp.cycles.is_empty() {
        return refuse("a canned drilling cycle has no cut sense to keep - mirror it with GeometricMirror");
    }
    let ms = &tp.moves;
    let passes = read_passes(ms)?;
    let mut out: Vec<Move> = Vec::with_capacity(ms.len());
    // Old index -> new index of each planar cut move.
    let mut index: Vec<Option<usize>> = vec![None; ms.len()];
    // The approach: kept up to its clearance run, which is re-aimed.
    let first = &passes[0];
    let top = ms[first.d].from.z;
    let lead = (0..first.d).rev().take_while(|&j| clearance(&ms[j], top)).count();
    if lead == 0 && first.d > 0 {
        return refuse("the path does not reach its first pass along a clearance plane");
    }
    if ms[..first.d - lead].iter().any(|m| m.kind != MoveKind::Rapid) {
        return refuse("the path cuts before its first pass");
    }
    out.extend_from_slice(&ms[..first.d - lead]);
    let start = if lead > 0 { ms[first.d - lead].from } else { ms[first.d].from };
    for (k, p) in passes.iter().enumerate() {
        let top = ms[p.d].from.z;
        let (cut_start, cut_end) = (ms[p.c].from, ms[p.a - 1].to);
        let new_top = Point3::new(cut_end.x, cut_end.y, top);
        let here = out.last().map_or(start, |m| m.to);
        if k > 0 || lead > 0 {
            out.extend(rapid(here, new_top));
        }
        out.extend(column_at(&ms[p.d..p.c], cut_end));
        for j in (p.c..p.a).rev() {
            index[j] = Some(out.len());
            out.push(reversed(&ms[j]));
        }
        out.extend(column_at(&ms[p.a..p.e], cut_start));
        // The link to the next pass, or the exit: clearance rapids only.
        let next = passes.get(k + 1).map_or(ms.len(), |q| q.d);
        let link = &ms[p.e..next];
        let on_plane = link.iter().take_while(|m| clearance(m, top)).count();
        if let Some(q) = passes.get(k + 1) {
            // The new link runs from this pass's new exit (old cut start)
            // to the next pass's new entry (old cut end), on the plane.
            let next_end = ms[q.a - 1].to;
            let same_xy = cut_start.x == next_end.x && cut_start.y == next_end.y;
            if on_plane < link.len() || (link.is_empty() && !same_xy) {
                return refuse("two passes are linked off their clearance plane");
            }
            if ms[next].from.z != top {
                return refuse("two passes have different clearance planes");
            }
        } else if link[on_plane..].iter().any(|m| m.kind != MoveKind::Rapid) {
            return refuse("the path cuts after its last pass");
        } else if on_plane == 0 && !link.is_empty() {
            return refuse("the path leaves its last pass off the clearance plane");
        } else if on_plane > 0 {
            let here = out.last().map_or(start, |m| m.to);
            out.extend(rapid(here, link[on_plane - 1].to));
            out.extend_from_slice(&link[on_plane..]);
        }
    }
    let comps = tp.comps.iter().map(|c| reverse_comp(c, &index)).collect::<Result<_, _>>()?;
    let out = Toolpath { moves: out, comps, ..tp };
    if !out.is_chained() || !out.comps.iter().all(|c| c.is_sane(&out.moves)) {
        return refuse("the reversed path does not chain");
    }
    Ok(out)
}

/// A compensated span inside one planar cut, run backwards: the
/// cancel block becomes the engage block, and the cutter side swaps
/// back (the mirror swapped it once).
fn reverse_comp(c: &CompSpan, index: &[Option<usize>]) -> Result<CompSpan, TransformError> {
    let (Some(Some(f)), Some(Some(l))) = (index.get(c.first), index.get(c.last)) else {
        return refuse("a compensated span reaches outside its planar cut");
    };
    if c.last.checked_sub(c.first).and_then(|n| l.checked_add(n)) != Some(*f) {
        return refuse("a compensated span spans two passes");
    }
    let programmed = c.programmed.iter().rev().map(reversed).collect();
    Ok(CompSpan { side: swap_side(c.side), first: *l, last: *f, programmed, ..c.clone() })
}
