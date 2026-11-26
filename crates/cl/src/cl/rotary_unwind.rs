//! The C unwind law (issue #33, slice 2): a C-stamped live-tool path
//! (`super::rotary::stamp_c`) states its spindle angle UNWRAPPED, so a
//! path that rings the part N times reads N * 360 at its end - past a
//! C travel the machine states (the shipped mill-turn states
//! -360..360). [`unwind_c`] brings every move back inside that travel
//! by whole turns, and only where the tool stands clear: at a link.
//!
//! Provenance: our own law, from general rotary machining practice -
//! a rotary axis with limited travel (no roll-over) is rewound while
//! the tool is retracted, never in the cut; RS-274 / ISO 6983 name C
//! the rotation about Z. No vendor material consulted.
//!
//! The rewind is not a renumbering: the spindle physically turns back
//! while the tool waits, so in the PART frame the tool orbits the part
//! at the link point's radius and height. The unwind is therefore
//! written as that orbit - rapid chords around the circle through the
//! link point, each stating its own C on the radial station, split so
//! the machine's C-linear arc stays within `tol` of the chord - and
//! every law downstream (verify, the solid bar, steel, the machine
//! check) judges it like any other move. Nothing turns that a law has
//! not seen.
//!
//! A link is a point between two moves, reached by a rapid, that
//! stands at the path's own clearance: the highest Z its rapids reach
//! (the axial clearance plane a standing bar's rings retract to before
//! the next ring's plunge) or, when the caller admits it (`radial`),
//! the largest radius (the retract cylinder of a laid bar's radial
//! links) - each only where it stands strictly beyond every cutting
//! move's reach. Between two links the cut runs as
//! stamped; a stretch whose C span no shift by whole turns fits inside
//! the travel is refused by name - there is no clear point to rewind
//! at. The first stretch is shifted without an orbit: the section opens
//! with a rapid to its first C from wherever the spindle stood.

use crate::geometry::Point3;

use super::rotary::{azimuth, ON_LINE};
use super::{Move, MoveKind, Toolpath};

/// How close (mm) a point must stand to the path's clearance height or
/// radius to count as clear.
const CLEAR_EPS: f64 = 1e-6;
/// How far (mm) a move may start from where the one before it ended
/// and still count as a chain (the 1 um lattice).
const CHAIN_TOL: f64 = 1e-6;
/// Slack on the travel bounds (deg), the machine check's own.
const TRAVEL_EPS: f64 = 1e-9;
/// Orbit chords per unwind, at most: a rewind needing more is a
/// tolerance no machine block stream should carry - refused.
const MAX_ORBIT_CHORDS: usize = 100_000;

/// `path` (stamped by `stamp_c` about the Z line through `center`)
/// with every stated C inside `travel` (deg, `lo < hi`), unwound by
/// whole turns at clear links, the rewinds written as orbit rapids
/// split to `tol`. A path that already fits comes back unchanged.
///
/// `radial` admits the retract cylinder as a clearance beside the
/// axial plane. Rotary Flow asks for it (#2828: its band links rest
/// on the cylinder); Rotary Contour links over the axial plane, and a
/// stretch whose only clear point is radial is then refused by
/// name, saying so.
pub fn unwind_c(path: &Toolpath, center: [f64; 2], travel: (f64, f64), tol: f64, radial: bool) -> Result<Toolpath, String> {
    let (lo, hi) = travel;
    if !(lo.is_finite() && hi.is_finite() && lo < hi) {
        return Err(format!("C unwind: the stated C travel {lo}..{hi} is not a range"));
    }
    if !(tol.is_finite() && tol > 0.0) || !center.iter().all(|v| v.is_finite()) {
        return Err("C unwind: no usable tolerance or spindle line".into());
    }
    let moves = &path.moves;
    if !moves.iter().all(|m| m.rotary.is_some_and(f64::is_finite)) {
        return Err("C unwind: a move states no finite C - only a C-stamped path unwinds".into());
    }
    let finite = |p: Point3| p.x.is_finite() && p.y.is_finite() && p.z.is_finite();
    if let Some(i) = moves.iter().position(|m| !(finite(m.from) && finite(m.to))) {
        return Err(format!("C unwind: move {i} has a non-finite point - refused"));
    }
    // The link test reads a move's start as the point the move before
    // it reached: checked, never assumed.
    if let Some(i) = (1..moves.len()).find(|&i| {
        let (a, b) = (moves[i - 1].to, moves[i].from);
        (a.x - b.x).hypot(a.y - b.y).hypot(a.z - b.z) > CHAIN_TOL
    }) {
        return Err(format!("C unwind: move {i} does not start where move {} ended - the path is not a chain", i - 1));
    }
    let radius = |p: Point3| (p.x - center[0]).hypot(p.y - center[1]);
    // The clearance is the path's outermost reach - and counts only
    // where it stands strictly beyond everything the tool cuts (a
    // ring's own radius is never a clearance, however outermost).
    // A plunge's start is where it leaves the clearance, not where it
    // cuts; and a feed a rapid follows ENDED at rest - the retract's
    // arrival on the link cylinder (#2828's flow shape) - not in a
    // cut: the cut reach is every non-rapid end plus every feed
    // start, minus each such rest point.
    let rapid_ends = || moves.iter().filter(|m| m.kind == MoveKind::Rapid).flat_map(|m| [m.from, m.to]);
    let cut_ends = || {
        moves.iter().enumerate().filter(|(_, m)| m.kind != MoveKind::Rapid).flat_map(|(i, m)| {
            let ended_at_rest = m.kind == MoveKind::Feed && moves.get(i + 1).is_some_and(|n| n.kind == MoveKind::Rapid);
            let end = if ended_at_rest { None } else { Some(m.to) };
            let start = (m.kind == MoveKind::Feed).then_some(m.from);
            end.into_iter().chain(start)
        })
    };
    let zmax = rapid_ends().map(|p| p.z).fold(f64::NEG_INFINITY, f64::max);
    let rmax = rapid_ends().map(radius).fold(f64::NEG_INFINITY, f64::max);
    let z_cut = cut_ends().map(|p| p.z).fold(f64::NEG_INFINITY, f64::max);
    let r_cut = cut_ends().map(radius).fold(f64::NEG_INFINITY, f64::max);
    let clear = |p: Point3| {
        (zmax > z_cut + CLEAR_EPS && p.z >= zmax - CLEAR_EPS)
            || (radial && rmax > r_cut + CLEAR_EPS && radius(p) >= rmax - CLEAR_EPS)
    };
    let radial_unused = if !radial && rmax > r_cut + CLEAR_EPS {
        " (a retract cylinder stands beyond the cut, but radial links are not taken for this kind)"
    } else {
        ""
    };
    // Stretch starts: 0, then every move leaving a clear point a rapid
    // brought the tool to (a ring's retract, before its next plunge).
    let mut starts: Vec<usize> = vec![0];
    starts.extend((1..moves.len()).filter(|&i| moves[i - 1].kind == MoveKind::Rapid && clear(moves[i].from)));
    let mut out: Vec<Move> = Vec::with_capacity(moves.len());
    let mut shift = 0.0;
    for (s, &a) in starts.iter().enumerate() {
        let b = starts.get(s + 1).copied().unwrap_or(moves.len());
        let stretch = &moves[a..b];
        // The C the stretch opens at (its first move's start: the end
        // of the move before it), then every C it states.
        let open = if a == 0 { None } else { moves[a - 1].rotary };
        let cs = open.into_iter().chain(stretch.iter().filter_map(|m| m.rotary)).map(|c| c + shift);
        let (s_lo, s_hi) = cs.fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), c| (l.min(c), h.max(c)));
        if s_lo >= lo - TRAVEL_EPS && s_hi <= hi + TRAVEL_EPS {
            out.extend(stretch.iter().map(|m| shifted(m, shift)));
            continue;
        }
        let k_min = ((lo - TRAVEL_EPS - s_lo) / 360.0).ceil();
        let k_max = ((hi + TRAVEL_EPS - s_hi) / 360.0).floor();
        if k_min > k_max {
            return Err(format!(
                "C-axis cut from move {a} spans C {s_lo:.3}..{s_hi:.3} deg ({:.3} deg) between links; the C travel {lo:.3}..{hi:.3} cannot hold it, and no clear link stands inside it to unwind at{radial_unused}",
                s_hi - s_lo
            ));
        }
        // The fewest turns that fit: the least rewind at this link.
        let k = if k_min > 0.0 {
            k_min
        } else if k_max < 0.0 {
            k_max
        } else {
            0.0
        };
        if a > 0 {
            if !(path.cycles.is_empty() && path.comps.is_empty() && path.threads.is_empty() && path.probes.is_empty()) {
                return Err("C unwind: the path carries index-keyed records - an orbit cannot be inserted".into());
            }
            let c0 = open.map_or(0.0, |c| c + shift);
            orbit(&moves[a], c0, c0 + 360.0 * k, center, tol, &mut out)?;
        }
        shift += 360.0 * k;
        out.extend(stretch.iter().map(|m| shifted(m, shift)));
    }
    let mut t = path.clone();
    t.moves = out;
    Ok(t)
}

/// `m` with its C moved by `shift` whole turns' worth of degrees (its
/// part-frame geometry and pose are periodic in C, so they stand).
fn shifted(m: &Move, shift: f64) -> Move {
    Move { rotary: m.rotary.map(|c| c + shift), ..*m }
}

/// The rewind from `c0` to `c1` at the start of `link`, written as the
/// part-frame orbit it is: rapid chords round the circle through
/// `link.from`, each stating its C and the radial station pose there.
fn orbit(link: &Move, c0: f64, c1: f64, center: [f64; 2], tol: f64, out: &mut Vec<Move>) -> Result<(), String> {
    let p = link.from;
    let r = (p.x - center[0]).hypot(p.y - center[1]);
    let sweep = (c1 - c0).abs();
    // On the spindle line the tool has no orbit: one C-only block.
    let n = if azimuth(p, center).is_none() || r <= ON_LINE {
        1
    } else {
        // The machine's arc bulges past each chord by r(1 - cos(d/2)).
        let step = 2.0 * (1.0 - (tol / r).min(1.0)).acos().to_degrees();
        let step = step.clamp(1e-3, 90.0);
        (sweep / step).ceil().max(1.0) as usize
    };
    if n > MAX_ORBIT_CHORDS {
        return Err(format!(
            "C unwind of {sweep:.3} deg at radius {r:.3} needs {n} chords to stay within {tol} mm - refused"
        ));
    }
    let mut from = p;
    for j in 1..=n {
        let c = c0 + (c1 - c0) * j as f64 / n as f64;
        let phi = (-c).to_radians();
        let to = if j == n || r <= ON_LINE {
            p
        } else {
            Point3::new(center[0] + r * phi.cos(), center[1] + r * phi.sin(), p.z)
        };
        let axis = Some([phi.cos(), phi.sin(), 0.0]);
        out.push(Move { kind: MoveKind::Rapid, from, to, feed: None, arc: None, axis, rotary: Some(c) });
        from = to;
    }
    Ok(())
}

#[cfg(test)]
#[path = "rotary_unwind_tests.rs"]
mod tests;
