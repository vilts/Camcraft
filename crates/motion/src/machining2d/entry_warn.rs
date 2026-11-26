//! The warnings of a start-side pass whose entry is not the selected one
//! or plunges into uncut stock (#3884, 2D "Pass starts at" Inside or
//! Outside): each stand-in entry that the ladder names, and a selected
//! Plunge at a pass start in material, become one advisory operation
//! warning each, never a note only. Provenance: our own rule, from the
//! maintainer's words on #3748 ("it uses a safe entry and names it in a
//! warning") and the Prime Directive; no vendor material consulted.

use crate::approach::{AIR_ENTRY_REFUSAL, OUTSIDE_AIR_REFUSAL};
use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::entry::Entry3D;
use crate::geo2d::Point2;
use crate::verify::Warning;

use super::GenError;
use super::descent::{Descent, Lvl};
use super::levels::Motion;
use super::pass_start::PassStart;

/// The ladder's marker note: a start-side pass took at least one stand-in
/// entry. The notes before it name each one ([`is_entry_stand_in`]).
pub const ENTRY_STAND_IN_NOTE: &str =
    "entry: Pass starts at - the selected entry did not fit at one or more pass starts; each stand-in entry is named";

/// The ladder's note of a start-side pass whose selected Plunge enters a
/// pass start that no earlier cut cleared: it plunges into uncut stock.
pub const PLUNGE_INTO_STOCK_NOTE: &str =
    "entry: Pass starts at - the selected Plunge entry plunges into uncut stock at a pass start that no earlier cut cleared";

/// #3884: the one Outside-air check of every Outside entry path - the
/// plan (no air room at all), the first entry of a level (no air lead
/// fits, `air_entry::stand_in`) and the re-entry after a lift
/// (`descent_dive::entry_restart`, `descent_dive::restart`). A selected
/// Arc on an Outside ring pass goes down where it is `clear` - in air
/// outside the stock, or (a re-entry) on the floor already cut at that
/// depth (#3855) - or it is refused by name with `why`: never an automatic
/// helix, ramp or dive into uncut stock. A selected Helix, Ramp or Plunge
/// runs, with its one warning.
pub(super) fn outside_air_check(outside: bool, entry: Entry3D, clear: bool, why: &str) -> Result<(), String> {
    if outside && entry == Entry3D::Arc && !clear {
        return Err(format!("{OUTSIDE_AIR_REFUSAL} ({why})"));
    }
    Ok(())
}

/// #3884: the selected Arc on an Outside pass re-enters a pass start in
/// material from the air outside the stock, or from the floor already cut
/// at that depth (a plunge whose tool disc is all on cut floor, then the
/// link at depth through cut floor to the start), or the op is refused by
/// name - never a ramp, a helix or a plunge into uncut stock. `leg`: where
/// the tool lifted. `None`: not that case.
pub(super) fn outside_arc_restart(b: &PathBuilder, l: &Lvl, leg: Point2, e: Point2, z: f64, plane: f64, m: &Motion) -> Option<Descent> {
    let air = m.air.as_ref().filter(|a| a.outside() && m.entry == Entry3D::Arc)?;
    let site = format!("the pass start ({:.3}, {:.3}) at Z {z:.3} is in material", e.x, e.y);
    let clear = |d: &Descent| super::descent_dive::moves_clear(b, d, l, z, plane, m);
    if let Some(a) = super::air_entry::restart(l, m).filter(|d| clear(d)) {
        return Some(Descent::Named(Box::new(a), format!("entry: {site} - entered from the air outside the stock box instead")));
    }
    let floor = cut_floor_point(b, l, leg, e, z, m.tool_r()).map(Descent::Plunge).filter(|d| clear(d));
    let why = format!("{site}, and no air lead and no cut floor reach it");
    match (floor, outside_air_check(air.outside(), m.entry, false, &why)) {
        (Some(d @ Descent::Plunge(p)), _) => {
            let note = format!("entry: {site} - no air lead reaches it; entered on the floor already cut at ({:.3}, {:.3})", p.x, p.y);
            Some(Descent::Named(Box::new(d), note))
        }
        (_, Err(r)) => Some(Descent::Refused(r)),
        (_, Ok(())) => Some(Descent::Refused(format!("{AIR_ENTRY_REFUSAL} - {why}"))),
    }
}

/// Is the whole disc of radius `r` round `p` on the floor already cut at
/// `z`? "Already cut" is read from `moves`, the moves posted before this
/// re-entry - so only a cut earlier in the program order counts - and from
/// their swept width at or below `z`: every sample of the disc (the
/// centre, a ring at half a radius and the rim) lies within one tool
/// radius of an earlier cutting move's centre path at that depth. A disc
/// that a cut covers only in part does not count.
fn disc_on_cut_floor(moves: &[crate::cl::Move], p: Point2, z: f64, r: f64) -> bool {
    const EPS: f64 = 1e-6;
    let cut: Vec<(Point2, Point2)> = moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid && m.from.z.min(m.to.z) <= z + EPS)
        .flat_map(|m| m.segments3(0.01))
        .filter(|(a, b)| a.z.max(b.z) <= z + EPS)
        .map(|(a, b)| (Point2::new(a.x, a.y), Point2::new(b.x, b.y)))
        .collect();
    let swept = |q: Point2| cut.iter().any(|&(a, b)| seg_dist(q, a, b) <= r + EPS);
    let ring = |k: f64, n: u32| {
        (0..n).map(move |i| {
            let a = std::f64::consts::TAU * f64::from(i) / f64::from(n);
            Point2::new(p.x + k * r * a.cos(), p.y + k * r * a.sin())
        })
    };
    std::iter::once(p).chain(ring(0.5, 12)).chain(ring(1.0, 24)).all(swept)
}

/// The distance from `q` to the segment `a`-`b`.
fn seg_dist(q: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((q.x - a.x) * dx + (q.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    (q.x - a.x - t * dx).hypot(q.y - a.y - t * dy)
}

/// #3855: a plunge point for a re-entry onto `e` at `z` on the floor
/// already cut - the lift point `leg` first, then rings round `e` from one
/// tool radius out - whose disc is on cut floor and whose link to `e` runs
/// through cut floor up to one radius short of `e`, inside the area.
fn cut_floor_point(b: &PathBuilder, l: &Lvl, leg: Point2, e: Point2, z: f64, r: f64) -> Option<Point2> {
    if !(r.is_finite() && r > 0.0 && z.is_finite()) {
        return None;
    }
    let moves = b.moves();
    let linked = |p: Point2| {
        let len = (e.x - p.x).hypot(e.y - p.y);
        let run = (len - r).max(0.0);
        let n = (run / (0.25 * r)).ceil().clamp(1.0, 1e4) as usize;
        (0..=n).all(|k| {
            let t = run * k as f64 / n as f64 / len.max(1e-12);
            crate::plunge_law::column_cleared(moves, Point2::new(p.x + t * (e.x - p.x), p.y + t * (e.y - p.y)), z, r)
        })
    };
    let ok = |p: Point2| l.cen.chord_ok(p, e) && disc_on_cut_floor(moves, p, z, r) && linked(p);
    let rings = [1.0, 1.5, 2.0, 3.0, 4.0, 6.0].into_iter().flat_map(move |k| {
        (0..24).map(move |i| {
            let a = std::f64::consts::TAU * f64::from(i) / 24.0;
            Point2::new(e.x + k * r * a.cos(), e.y + k * r * a.sin())
        })
    });
    std::iter::once(leg).chain(rings).find(|&p| ok(p))
}

/// The plan's call of [`outside_air_check`]: an Outside ring pass
/// (`rings`: the resolved strategy walks rings, an Adaptive op's ring
/// fallback too) with no air room - no stock known, not round a part, or
/// the fixtures fill it.
pub(super) fn refuse_outside_arc(start: PassStart, entry: Entry3D, rings: bool, air: bool) -> Result<(), GenError> {
    outside_air_check(start == PassStart::Outside && rings, entry, air, "the pass has no air room outside the stock").map_err(GenError::Invalid)
}

/// The tails of the notes that name a stand-in entry: the entry that the
/// ladder used where the selected one did not fit.
const STAND_IN_TAILS: [&str; 6] = [
    " - ramp along the pass instead",
    " - helix entry instead",
    " - ramp along the first cut instead",
    "plunged into material at the plunge feed",
    "axial dive on the floor instead at (",
    "the floor has no dive room either; dived at the pass start",
];

/// Whether `note` names a stand-in entry.
#[must_use]
pub fn is_entry_stand_in(note: &str) -> bool {
    STAND_IN_TAILS.iter().any(|t| note.contains(t))
}

/// How many stand-in entries the notes name.
#[must_use]
pub fn entry_stand_ins(notes: &[String]) -> usize {
    notes.iter().filter(|n| is_entry_stand_in(n)).count()
}

/// Mark a stand-in entry `note` of a start-side ladder (`m.cleared_links`,
/// the start-side plan's link law of #3316).
/// `None`: the substituted axial dive, a stand-in always.
pub(super) fn mark_stand_in(b: &mut PathBuilder, m: &Motion, note: Option<&str>) {
    if m.cleared_links && note.is_none_or(is_entry_stand_in) {
        b.note_once(ENTRY_STAND_IN_NOTE);
    }
}

/// A start-side pass link that plunges at `e` into uncut stock at `z`: the
/// selected Plunge is named once; any other selected entry that comes to
/// this plunge (no certified area to plan it in) is a named stand-in.
pub(super) fn plunge_into_stock(b: &mut PathBuilder, m: &Motion, e: Point2, z: f64) {
    if m.entry == Entry3D::Plunge {
        b.note_once(PLUNGE_INTO_STOCK_NOTE);
        return;
    }
    let note = format!(
        "entry: the pass start ({:.3}, {:.3}) at Z {z:.3} is in material - plunged into material at the plunge feed",
        e.x, e.y
    );
    b.note_once(&note);
    mark_stand_in(b, m, Some(&note));
}

/// The start-side entry warnings of `path`: one `EntryStandIn` that counts
/// the named stand-ins, and one `PlungeIntoStock` for a selected Plunge
/// (none where `advised`: the op already names the selected Plunge in its
/// `EntryNotRecommended` warning, so the Plunge is named once).
#[must_use]
pub fn entry_warnings(path: &Toolpath, advised: bool) -> Vec<Warning> {
    let at = path.moves.iter().position(|m| m.kind != MoveKind::Rapid).unwrap_or(0);
    let mut out = Vec::new();
    if path.notes.iter().any(|n| n == ENTRY_STAND_IN_NOTE) {
        out.push(Warning::EntryStandIn { at, entries: entry_stand_ins(&path.notes).max(1) });
    }
    if !advised && path.notes.iter().any(|n| n == PLUNGE_INTO_STOCK_NOTE) {
        out.push(Warning::PlungeIntoStock { at });
    }
    out
}
