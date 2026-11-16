//! Issue #1037: a lathe holder body (#369) judged against the setup's workholding - the shop rule that a holder never meets the chuck, the tailstock or a steady rest (machine-tool handbooks), read in the meridian: steel that turns with the part as its revolved envelope (the stock's reading), steel that stands still as a box in the machine frame the tool's plane is fixed in; each test is exact for a straight tip motion (a linear function's interval on a segment); our own design, no vendor material consulted.
//!
//! ## The laws here
//!
//! - **Every move, every band, every box.** Each move's meridian pieces
//!   (the holder law's own: chords, a thread's helix at its radius, an
//!   arc's chords) carry each of the body's clearance spans
//!   (`LatheClearance::spans`) against each fixture box. The tip moves
//!   straight along a piece, so the instants a band meets a box form an
//!   interval of the piece's parameter, solved exactly - never sampled.
//! - **Turning steel is revolved.** A chuck jaw (the default fixture,
//!   M13.8 (2)) spins with the part, so a holder meets it wherever the
//!   jaw's revolved envelope stands: its box's stations, out to its
//!   farthest corner from the axis and in to the box's nearest reach
//!   (the frame box of its corners - nearer than the jaw, never
//!   farther).
//! - **Standing steel is a box in the machine frame.** A tailstock, a
//!   steady rest, a table (declared fixed) stands still while the part
//!   turns; the tool's plane is fixed in the machine, so the body's band
//!   is a box there too: its stations, its radial span about the tip's
//!   signed radius, and +-|y| across the plane (the band keeps |y| only,
//!   so both sides are read). The fixture is read as the frame box of
//!   its corners: exact for a spindle on a world axis, larger otherwise.
//! - **On the holder's own side** (#1102). A body point x stands along
//!   the HOLDER's radial, which is the path's signed radial or its
//!   negation (a rear tool post, a path whose first point sits past
//!   centre): the pieces and the steel are both read in the holder's
//!   frame (`sided_pieces`), and a path on both sides of the axis is
//!   refused by name, as the holder law refuses it - never read
//!   mirrored.
//! - **Contact is contact.** Each box is grown by the margin, as the
//!   fixture laws grow theirs (`point_inside`).
//! - **One finding per move**: a `FixtureHit` on the holder, blocking
//!   the post, naming the move.
//! - **Steel no box can be read for is named** on the op's notes
//!   ([`STEEL_UNREAD_NOTE`]), never passed as clear.

use camcraft_tools::lathe_holder::{LatheClearance, Reach};

use crate::cl::Toolpath;
use crate::fixture::FixtureBox;
use crate::geometry::{Point3, Triangle};
use crate::turning::profile::Frame;
use crate::verify::{FixturePart, Warning};

use super::kinds::Section;
use super::turning_holder::sided_pieces;
use super::turning_rapids::SignedMeridian;
use super::OpParams;

/// The start of the note a bodied turning op carries when the setup
/// holds a fixture the holder law cannot read.
pub const STEEL_UNREAD_NOTE: &str = "lathe holder body not judged against fixture";

/// A `t` range on a piece, inside `[0, 1]`.
type Span = (f64, f64);

/// One fixture read in the meridian frame.
#[derive(Clone, Copy, Debug)]
enum Steel {
    /// Turns with the part: stations `s`, radii `r` of its revolved
    /// envelope.
    Revolved { s: Span, r: Span },
    /// Stands in the machine frame: stations `s`, radial `u` and normal
    /// `v` spans.
    Standing { s: Span, u: Span, v: Span },
}

impl Steel {
    fn stations(&self) -> Span {
        match *self {
            Steel::Revolved { s, .. } | Steel::Standing { s, .. } => s,
        }
    }
}

/// The path with the unread-steel note, once: a turning op whose tool
/// carries a lathe body, in a setup holding fixtures (`unread`, by
/// name) no box could be read for.
pub(super) fn noted(mut path: Toolpath, params: &OpParams, has_body: bool, unread: &[String]) -> Toolpath {
    if !has_body || unread.is_empty() || params.kind().info().section != Section::Turning {
        return path;
    }
    let names = unread.iter().map(|n| format!("'{n}'")).collect::<Vec<_>>().join(", ");
    let n = format!("{STEEL_UNREAD_NOTE} {names} - it has no geometry the law can read, so a strike on it would pass unseen");
    if !path.notes.contains(&n) {
        path.notes.push(n);
    }
    path
}

/// The holder-versus-steel law (#1037) on `path`, cut about the part
/// `part` by a tool whose lathe body is `body`: one holder
/// [`Warning::FixtureHit`] per move whose body meets a box of `turning`
/// (steel spinning with the part) or `fixed` (standing steel), each
/// grown by `margin`. Nothing outside the turning section or with no
/// steel; refuses by name when the path cannot be read in the meridian.
pub(crate) fn steel_hits(
    params: &OpParams,
    path: &Toolpath,
    part: &[Triangle],
    body: &LatheClearance,
    (turning, fixed): (&[FixtureBox], &[FixtureBox]),
    margin: f64,
) -> Result<Vec<Warning>, String> {
    if params.kind().info().section != Section::Turning || (turning.is_empty() && fixed.is_empty()) {
        return Ok(Vec::new());
    }
    let unjudged = |why: String| format!("the lathe holder body cannot be judged against the fixtures: {why}");
    let frame = Frame::about(part).map_err(|e| unjudged(format!("no spindle fits the part ({e})")))?;
    let sm = SignedMeridian::of(&frame, path);
    // #1102: the holder's side; every radius below is read along the
    // holder's own +x (the path's signed radial times `side`).
    let (side, pieces) = sided_pieces(&sm, path, super::Z_TOL).map_err(unjudged)?;
    let m = if margin.is_finite() { margin.max(0.0) } else { 0.0 };
    let mut steel = Vec::with_capacity(turning.len() + fixed.len());
    for (b, turns) in turning.iter().map(|b| (b, true)).chain(fixed.iter().map(|b| (b, false))) {
        steel.push(read(&sm, b, turns, m, side).ok_or_else(|| unjudged("a fixture box is not finite".into()))?);
    }
    let spans = body.spans();
    let mut out = Vec::new();
    for (i, pieces) in pieces.iter().enumerate() {
        let pieces = pieces.iter().map(|&((ps, px), (qs, qx))| ((ps, side * px), (qs, side * qx)));
        let struck = pieces.filter(|&(p, q)| [p.0, p.1, q.0, q.1].iter().all(|v| v.is_finite())).any(|(p, q)| {
            // Only steel the body's whole length can reach along this piece.
            let (lo, hi) = (p.0.min(q.0) + body.z_lo(), p.0.max(q.0) + body.z_hi());
            steel.iter().filter(|st| st.stations().1 >= lo && st.stations().0 <= hi).any(|st| spans.iter().any(|&(z0, z1, reach)| meets(p, q, z0, z1, &reach, st)))
        });
        if struck {
            out.push(Warning::FixtureHit { at: i, fixture_part: FixturePart::Holder });
        }
    }
    Ok(out)
}

/// [`steel_hits`] at the post gate (`fixture_struck::op_hits`): the
/// op's part read as generation reads it, the steel as it stands now.
/// `Err` names why it cannot be judged - never read as clear.
pub(crate) fn lathe_steel_hits(
    world: &flecs_ecs::prelude::World,
    op: &crate::schema::Operation,
    path: &Toolpath,
    body: &LatheClearance,
    steel: (&[FixtureBox], &[FixtureBox]),
) -> Result<Vec<Warning>, String> {
    if op.params.kind().info().section != Section::Turning || (steel.0.is_empty() && steel.1.is_empty()) {
        return Ok(Vec::new());
    }
    let part = super::material_of(world, op.setup.as_deref(), &op.src)
        .map_err(|e| format!("the lathe holder body cannot be judged against the fixtures: {e}"))?;
    steel_hits(&op.params, path, &part.facets, body, steel, super::GOUGE_TOL)
}

/// A fixture box in the meridian frame on the holder's `side` (its
/// radial along the holder's +x), grown by `m`; `None` when it holds a
/// non-finite number.
fn read(sm: &SignedMeridian, b: &FixtureBox, turns: bool, m: f64, side: f64) -> Option<Steel> {
    let (lo, hi) = (b.min, b.max);
    let mut s = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut u, mut v) = (s, s);
    let mut far = 0.0f64;
    for k in 0..8 {
        let q = Point3::new(if k & 1 == 0 { lo.x } else { hi.x }, if k & 2 == 0 { lo.y } else { hi.y }, if k & 4 == 0 { lo.z } else { hi.z });
        let (cs, cu, cv) = sm.suv(q);
        let cu = side * cu;
        if !(cs.is_finite() && cu.is_finite() && cv.is_finite()) {
            return None;
        }
        s = (s.0.min(cs), s.1.max(cs));
        u = (u.0.min(cu), u.1.max(cu));
        v = (v.0.min(cv), v.1.max(cv));
        far = far.max(cu.hypot(cv));
    }
    let grow = |(a, b): Span| (a - m, b + m);
    Some(if turns {
        // The nearest reach of the corners' frame box: 0 on an axis it holds.
        let near = (u.0.max(-u.1).max(0.0)).hypot(v.0.max(-v.1).max(0.0));
        Steel::Revolved { s: grow(s), r: ((near - m).max(0.0), far + m) }
    } else {
        Steel::Standing { s: grow(s), u: grow(u), v: grow(v) }
    })
}

/// Does the body band `[z0, z1]` (tip-relative stations) with spans
/// `reach` meet `steel` while the tip runs straight from `p` to `q`
/// (station, signed radius)?
fn meets(p: (f64, f64), q: (f64, f64), z0: f64, z1: f64, reach: &Reach, steel: &Steel) -> bool {
    let (ds, dx) = (q.0 - p.0, q.1 - p.1);
    // The tip's radius X with X + x_lo < hi and X + x_hi > lo.
    let radial = |lo: f64, hi: f64| along(p.1, dx, lo - reach.x_hi, hi - reach.x_lo);
    match *steel {
        Steel::Standing { s, u, v } => {
            if !(-reach.y_hi < v.1 && reach.y_hi > v.0) {
                return false;
            }
            meet(along(p.0, ds, s.0 - z1, s.1 - z0), radial(u.0, u.1)).is_some()
        }
        Steel::Revolved { s, r } => {
            let Some(at) = along(p.0, ds, s.0 - z1, s.1 - z0) else { return false };
            // Nearest point of the band inside the envelope's skin: the
            // radial span meets (-R, R), R the skin less the band's |y|.
            if reach.y_lo >= r.1 {
                return false;
            }
            let big = (r.1 * r.1 - reach.y_lo * reach.y_lo).sqrt();
            let Some(at) = meet(Some(at), radial(-big, big)) else { return false };
            // Farthest point beyond the envelope's bore: |x| past Q.
            if r.0 <= reach.y_hi {
                return true;
            }
            let small = (r.0 * r.0 - reach.y_hi * reach.y_hi).sqrt();
            let out = along(p.1, dx, small - reach.x_hi, f64::INFINITY);
            let back = along(p.1, dx, f64::NEG_INFINITY, -small - reach.x_lo);
            meet(Some(at), out).is_some() || meet(Some(at), back).is_some()
        }
    }
}

/// The `t` in `[0, 1]` with `lo < a + d t < hi`.
fn along(a: f64, d: f64, lo: f64, hi: f64) -> Option<Span> {
    if d == 0.0 || !d.is_finite() {
        return (lo < a && a < hi).then_some((0.0, 1.0));
    }
    let (t0, t1) = ((lo - a) / d, (hi - a) / d);
    let (l, h) = (t0.min(t1).max(0.0), t0.max(t1).min(1.0));
    (l <= h).then_some((l, h))
}

fn meet(a: Option<Span>, b: Option<Span>) -> Option<Span> {
    let (a, b) = (a?, b?);
    let (l, h) = (a.0.max(b.0), a.1.min(b.1));
    (l <= h).then_some((l, h))
}

#[cfg(test)]
#[path = "turning_holder_steel_tests.rs"]
mod tests;
