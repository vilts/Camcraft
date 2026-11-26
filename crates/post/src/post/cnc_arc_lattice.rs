//! The stream writer's arc law on the micrometre lattice (issue #3442).
//!
//! Provenance: original. The `.cnc` stream writes every position as
//! whole micrometres (`cnc::serialize`), and its reader takes an arc
//! record whose end equals its start as a full turn (the
//! `full_circle_raw` form; RS-274 / ISO 6983 read such a block the same
//! way). A short arc whose chord is under the lattice step would round to
//! that form and be read as a full circle. Only the writer still knows
//! the true sweep, so the writer decides:
//! - the rounded in-plane ends differ, and they are not on one ray from
//!   the rounded centre: the arc record, as before;
//! - they differ on one ray from the rounded centre (the same angle about
//!   it, or one of them on it; issue #3474,
//!   `docs/decisions/3474-same-angle-arc.md`) and the true sweep is half
//!   a turn or less: the record is written as a `Feed` to its rounded
//!   end. The reader sees no sweep in such a record and takes it as a
//!   full turn, and a post that writes the lattice numbers (mm, three
//!   decimals) cannot tell the short arc from a turn of nearly a circle;
//! - they differ on one ray and the arc turns more than half a circle:
//!   the full turn within the lattice, the arc record as before;
//! - they are equal and the arc turns more than half a circle (or its
//!   true ends are equal): the full turn within one micrometre, the arc
//!   record as before;
//! - they are equal on a short arc: the record is written as a `Feed` to
//!   its rounded end. The tool moves less than one micrometre in the
//!   plane, and the normal axis (a helix) still moves as written. The
//!   record stays (a zero-length `Feed` when nothing moves) so the
//!   stream keeps one record for each toolpath move.

use super::cnc::MoveRec;
use crate::cl::ArcPlane;

/// The lattice word of a length: micrometres, nearest.
fn um(v: f64) -> i64 {
    (v * 1000.0).round() as i64
}

/// True when the rounded points `s` and `e` differ only along one ray from
/// the rounded centre `c` (the same angle about it), or one of them is on
/// it. Exact on the lattice: the zero of the cross product and the sign of
/// the dot product, in integers. An overflow reads as "not on one ray".
fn on_one_ray(s: (i64, i64), e: (i64, i64), c: (i64, i64)) -> bool {
    let d = |a: i64, b: i64| i128::from(a) - i128::from(b);
    let (su, sv, eu, ev) = (d(s.0, c.0), d(s.1, c.1), d(e.0, c.0), d(e.1, c.1));
    if (su == 0 && sv == 0) || (eu == 0 && ev == 0) {
        return true;
    }
    let (Some(a), Some(b), Some(p), Some(q)) =
        (su.checked_mul(ev), sv.checked_mul(eu), su.checked_mul(eu), sv.checked_mul(ev))
    else {
        return false;
    };
    a == b && p.checked_add(q).is_some_and(|dot| dot > 0)
}

/// True when the arc record `m`, which starts at `from`, must be written
/// as a straight `Feed`: its true sweep is half a turn or less, and its
/// in-plane ends round to the same micrometre, or (#3474) to two points on
/// one ray from the rounded centre. A record with no arc, no known start,
/// or a non-finite value is never changed.
pub fn short_arc_on_lattice(from: Option<[f64; 3]>, m: &MoveRec) -> bool {
    let (Some((c0, c1)), Some(s)) = (m.center, from) else {
        return false;
    };
    let e = [m.x, m.y, m.z];
    // The right-handed in-plane frame (u, v), u x v = +normal, and the
    // centre in it (the centre pair is in word order: I J, I K, J K).
    let (u, v, cu, cv) = match m.plane {
        ArcPlane::Xy => (0, 1, c0, c1),
        ArcPlane::Xz => (2, 0, c1, c0),
        ArcPlane::Yz => (1, 2, c0, c1),
    };
    let vals = [s[u], s[v], e[u], e[v], cu, cv];
    if vals.iter().any(|x| !x.is_finite()) {
        return false;
    }
    let (rs, re) = ((um(s[u]), um(s[v])), (um(e[u]), um(e[v])));
    let coincide = rs == re;
    if !coincide && !on_one_ray(rs, re, (um(cu), um(cv))) {
        return false;
    }
    if s[u] == e[u] && s[v] == e[v] {
        return false; // a full turn stated as one record
    }
    let a0 = (s[v] - cv).atan2(s[u] - cu);
    let a1 = (e[v] - cv).atan2(e[u] - cu);
    let mut sweep = if m.ccw { a1 - a0 } else { a0 - a1 };
    if sweep < 0.0 {
        sweep += std::f64::consts::TAU;
    }
    // #3474: ends that differ on one ray with no true sweep at all are no
    // short arc that this rule can read: the record stays as it is.
    sweep <= std::f64::consts::PI && (coincide || sweep > 0.0)
}
