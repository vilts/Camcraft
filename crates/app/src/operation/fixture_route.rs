//! Link routing over the setup's workholding (#339 slice 5): a link -
//! the run of rapids between two cuts - whose tool stack would strike a
//! fixture is re-laid as retract, traverse, plunge at a height that
//! clears the steel under it, instead of only being reported.
//!
//! Provenance: the clearance-plane / retract-height link of published
//! CAM practice and machining handbooks (lift to a plane above every
//! obstacle under the traverse, cross, come back down - RS-274 / ISO
//! 6983 G0 moves), with the obstacle height read from the fixture boxes
//! the M3.2 fixture laws already judge. The laws here (which runs are
//! links, what height, what refuses) are our own; no vendor material
//! consulted (Clean-Room Protocol).
//!
//! ## The laws here
//!
//! - **A link is a run of rapids.** The maximal run of straight,
//!   upright rapids between two non-rapid moves (or a path end). Feeds
//!   are never re-laid: a keep-down link is a feed the CL stream cannot
//!   tell from a cut, and its stack sweeps steel the neighbouring cuts'
//!   stacks sweep too - its hits stay reported (the post is blocked by
//!   name). A rapid run that stays down at depth is a keep-down link,
//!   and it does become retract / traverse / plunge.
//! - **Only a struck link moves.** A run the gate's own fixture laws
//!   find clear is left byte for byte, so a document without fixtures,
//!   or whose links already clear, posts exactly what it did. Those
//!   laws read the whole upright stack on a rapid - shank, holder
//!   bands, housing, flute - over each move's true z span since #424,
//!   each meshed fixture acquitting what its box flagged (#476).
//! - **The height.** The tip rises to `H = max(the run's own top,
//!   highest steel under the traverse + ROUTE_GAP)`. "Under the
//!   traverse" is every box the widest band of the stack (flute, shank,
//!   holder bands, spindle housing) reaches along the XY segment; with
//!   the tip above all of it, every band - which stands above the tip -
//!   is above the steel too.
//! - **The cap.** With a stated Z stroke, `H` stops at its top (still
//!   [`MIN_GAP`] over the steel); steel taller than that is refused by
//!   name: no link can cross it.
//! - **Re-judged.** The re-laid link is judged again by the gate's
//!   laws, so a routed link posts. Still struck means the
//!   steel stands over the link's own end columns - no height helps:
//!   the link is left as laid, its hit stays reported, and the path's
//!   notes say why.
//! - **Upright mills only.** A path with a pose off vertical, a turned
//!   C, an arc rapid, a canned cycle, a thread or probe record keeps
//!   today's report-only verdict. A run inside a compensated span too.

use crate::cl::{is_vertical, lattice, Move, MoveKind, Toolpath};
use crate::fixture::{band_hits_box, FixtureBox};
use crate::geometry::Point3;
use crate::machining2d::GenError;
use crate::verify::SteelBand;

/// How far over the highest steel under a raised traverse the tip
/// rides: the clearance-plane margin, mm.
pub const ROUTE_GAP: f64 = 5.0;
/// The least margin kept when the Z stroke caps the height, mm.
pub const MIN_GAP: f64 = 0.5;

/// What the planner reads besides the path.
#[derive(Clone, Copy, Debug)]
pub struct LinkSteel<'a> {
    /// The setup's whole collision set, each box with its name.
    pub boxes: &'a [(String, FixtureBox)],
    /// The tool assembly, `verify::steel_stack`.
    pub stack: &'a [SteelBand],
    /// The highest world Z the tip reaches, when the machine states it.
    pub ceiling: Option<f64>,
}

/// One link's plan.
enum Plan {
    /// Re-laid: the new moves, the height, the steel it rose over.
    Raised(Vec<Move>, f64, usize),
    /// No height clears: the steel's index, its top.
    Unreachable(usize, f64),
}

/// Plan every struck link of `path` over `steel`. `laws` is the gate's
/// own verdict on a path of rapids (true: a fixture is struck) and the
/// only one read: a run it finds clear is never touched, and a routed
/// link is accepted only where the post would accept it.
pub fn plan_links(
    path: Toolpath,
    steel: &LinkSteel<'_>,
    laws: impl Fn(&Toolpath) -> bool,
) -> Result<Toolpath, GenError> {
    if steel.boxes.is_empty() || steel.stack.is_empty() || !routable(&path) {
        return Ok(path);
    }
    let runs = rapid_runs(&path);
    let mut swaps: Vec<(usize, usize, Vec<Move>)> = Vec::new();
    let (mut raised, mut first, mut stuck) = (0usize, None, None);
    for (i, j) in runs {
        if path.comps.iter().any(|c| c.first <= j && i <= c.last) {
            continue;
        }
        let Some(run) = path.moves.get(i..=j) else { continue };
        let (Some(head), Some(tail)) = (run.first(), run.last()) else { continue };
        if !struck(run, &laws) {
            continue;
        }
        match plan(run, steel) {
            Plan::Unreachable(k, top) => {
                let name = steel.boxes.get(k).map_or("?", |(n, _)| n.as_str());
                let cap = steel.ceiling.unwrap_or(f64::NAN);
                let (a, b) = (head.from, tail.to);
                return Err(GenError::Invalid(format!(
                    "the link at moves {i}..{j}, ({:.3}, {:.3}) to ({:.3}, {:.3}), cannot clear fixture '{name}': \
                     its steel stands to Z {top:.3}, and the machine's Z stroke tops out at Z {cap:.3} - \
                     move the fixture or the part",
                    a.x, a.y, b.x, b.y
                ))
                // #1015: an upright mill's path is world (no frame routes): it rides along.
                .with_ghost(&path));
            }
            Plan::Raised(moves, h, k) if !struck(&moves, &laws) => {
                raised += 1;
                first.get_or_insert((head.from, h, k));
                swaps.push((i, j, moves));
            }
            Plan::Raised(..) => {
                stuck.get_or_insert(head.from);
            }
        }
    }
    let mut out = splice(path, swaps);
    if let Some((a, h, k)) = first {
        let name = steel.boxes.get(k).map_or("?", |(n, _)| n.as_str());
        out.notes.push(format!(
            "fixture: {raised} link(s) raised over the workholding instead of striking it \
             (first: from ({:.3}, {:.3}) over '{name}' at Z {h:.3})",
            a.x, a.y
        ));
    }
    if let Some(a) = stuck {
        out.notes.push(format!(
            "fixture: the link from ({:.3}, {:.3}, {:.3}) strikes steel standing over its own end - no height \
             clears it, so it was left as laid and its fixture hit stands",
            a.x, a.y, a.z
        ));
    }
    Ok(out)
}

/// The planner reads only upright, C-flat paths with no record whose
/// indices it would have to carry (a canned cycle's holes are posted
/// as the cycle, not as these moves).
fn routable(path: &Toolpath) -> bool {
    path.cycles.is_empty()
        && path.threads.is_empty()
        && path.probes.is_empty()
        && path
            .moves
            .iter()
            .all(|m| m.axis.is_none_or(|a| is_vertical(&a)) && m.rotary.is_none_or(|c| c == 0.0))
}

/// Every maximal run `i..=j` of straight rapids.
fn rapid_runs(path: &Toolpath) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    for (k, m) in path.moves.iter().enumerate() {
        let link = m.kind == MoveKind::Rapid && m.arc.is_none();
        match (link, start) {
            (true, None) => start = Some(k),
            (false, Some(s)) => {
                out.push((s, k - 1));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s, path.moves.len().saturating_sub(1)));
    }
    out
}

/// Does `run` strike steel by the gate's laws?
fn struck(run: &[Move], laws: &impl Fn(&Toolpath) -> bool) -> bool {
    laws(&Toolpath::new(run.to_vec()))
}

/// Retract, traverse, plunge at a height over every box the widest band
/// reaches along the traverse.
fn plan(run: &[Move], steel: &LinkSteel<'_>) -> Plan {
    let (Some(&like), Some(last)) = (run.first(), run.last()) else {
        return Plan::Raised(Vec::new(), f64::NAN, 0);
    };
    let (a, b) = (like.from, last.to);
    let own_top = run.iter().fold(f64::MIN, |t, m| t.max(m.from.z).max(m.to.z));
    let reach = steel.stack.iter().fold(0.0f64, |r, s| r.max(s.r));
    let (mut top, mut k_top) = (f64::MIN, 0usize);
    for (k, (_, bx)) in steel.boxes.iter().enumerate() {
        // Every z: only the XY footprint asks here.
        if band_hits_box(a.x, a.y, b.x, b.y, -1e12, 1e12, reach, bx) && bx.max.z > top {
            (top, k_top) = (bx.max.z, k);
        }
    }
    let mut h = own_top.max(top + ROUTE_GAP);
    if let Some(cap) = steel.ceiling.map(|c| (c * 1e6).floor() / 1e6)
        && h > cap
        && h > own_top
    {
        if top + MIN_GAP > cap {
            return Plan::Unreachable(k_top, top);
        }
        h = cap.max(own_top);
    }
    let h = lattice(h);
    let rapid = |from: Point3, to: Point3| {
        Move::new(MoveKind::Rapid, from, to, None).attach_axis(like.axis)
    };
    let (up, over) = (Point3::new(a.x, a.y, h), Point3::new(b.x, b.y, h));
    let mut moves = Vec::with_capacity(3);
    if a.z < h {
        moves.push(rapid(a, up));
    }
    if (a.x, a.y) != (b.x, b.y) {
        moves.push(rapid(if a.z < h { up } else { a }, over));
    }
    if b.z < h {
        moves.push(rapid(moves.last().map_or(a, |m| m.to), b));
    }
    let moves = moves.into_iter().map(|m| match like.rotary {
        Some(c) => m.with_rotary(c),
        None => m,
    });
    Plan::Raised(moves.collect(), h, k_top)
}

/// `path` with each `i..=j` replaced, compensated spans re-indexed (no
/// span overlaps a replaced run - those runs were skipped).
fn splice(mut path: Toolpath, swaps: Vec<(usize, usize, Vec<Move>)>) -> Toolpath {
    if swaps.is_empty() {
        return path;
    }
    let old = std::mem::take(&mut path.moves);
    let mut index = vec![0usize; old.len()];
    let mut moves = Vec::with_capacity(old.len() + 2 * swaps.len());
    let mut swaps = swaps.into_iter().peekable();
    let mut k = 0;
    while k < old.len() {
        if let Some((i, j, _)) = swaps.peek()
            && *i == k
        {
            let (i, j) = (*i, *j);
            if let Some((_, _, new)) = swaps.next() {
                for slot in index.iter_mut().take(j + 1).skip(i) {
                    *slot = moves.len();
                }
                moves.extend(new);
            }
            k = j + 1;
            continue;
        }
        if let (Some(slot), Some(m)) = (index.get_mut(k), old.get(k)) {
            *slot = moves.len();
            moves.push(*m);
        }
        k += 1;
    }
    for c in &mut path.comps {
        c.first = index.get(c.first).copied().unwrap_or(c.first);
        c.last = index.get(c.last).copied().unwrap_or(c.last);
    }
    path.moves = moves;
    path
}
