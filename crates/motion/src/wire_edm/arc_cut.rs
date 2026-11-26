//! The wire contour of an analytic line-and-arc profile (#3937): the
//! drawing's lines and arcs offset by the kerf (`arc_offset`), entered by
//! a straight lead from the stated start hole to the path's nearest
//! point (or, when stated, the tangent arc lead of `arc_lead`, #3991),
//! cut as true arcs, and certified by the laws read against the
//! drawing itself (`arc_laws`).
//!
//! - **Seam.** The nearest point of the path to the start hole; on a
//!   circle that is the point on the radius through the hole, so the lead
//!   enters square to the path. A hole exactly at the centre of a
//!   circular path has every point equally near: by rule the lead then
//!   enters towards +X (of equally near points, the one at the least
//!   counter-clockwise angle from +X wins), and the path says so.
//! - **Arcs.** An arc of more than half a turn - a full circle above all,
//!   which no single block states - is cut as two halves.
//! - **Lattice and gaps.** A recorded arc's centre stands on the CL
//!   lattice (`cl::lattice`, 1e-6 mm). Each arc is moved onto its lattice
//!   centre with its radius kept, its ends taken along their radii. When
//!   that move plus the drawing's widest joint gap passes [`MARGIN_FLOOR`],
//!   the path is laid that much farther on the waste side, so the kerf
//!   never reaches the product, and the path says so.
//! - **Arc joints** (#3959). Two arcs whose snapped ends differ by no more
//!   than [`JOIN_MAX`] meet at the mean of the two ends, arc to arc: no
//!   connector move of a few nanometres is written between them. Each arc
//!   end then stands up to half the gap off its circle, which the laws
//!   read (a spiral reading) and the margin holds. A wider gap keeps a real
//!   connector line.
//!
//! Provenance: our own implementation over the public wire EDM practice
//! the module head cites; RS-274 / ISO 6983 circular interpolation for
//! the arc blocks (an arc is stated by its end and centre, a full turn
//! needs more than one block on most controls).

use std::f64::consts::TAU;

use crate::cl::{lattice, MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{GeoError, Loop, Point2, Region};
use crate::geometry::Point3;
use crate::machining2d::GenError;

use super::arc_chain::ArcChain;
use super::arc_dist::{nearest, pt_span};
use super::arc_laws;
use super::arc_lead::{arc_leads, WireLead};
use super::arc_offset::parallel_walk;
use super::arc_span::{dist, mul, sub, add, unit, Span};
use super::{laws, WireParams, WireSide, MIN_LEAD};

/// The least margin (mm) the path is moved out by: 1 nm. Under it the
/// kerf law's own [`super::PATH_TOL`] (10 nm) holds the difference.
const MARGIN_FLOOR: f64 = 1e-6;

/// The widest gap (mm) two neighbour arcs are joined across at the mean
/// of their ends (2.5 um, the walk's own joint reach). A gap past it is a
/// real move and keeps its connector line.
const JOIN_MAX: f64 = 10.0 * super::arc_chain::JOINT_TOL;

fn invalid(why: String) -> GenError {
    GenError::Invalid(format!("wire contour: {why}"))
}

fn at3(p: Point2, z: f64) -> Point3 {
    Point3::new(p.x, p.y, z)
}

fn on_lattice(c: Point2) -> Point2 {
    Point2::new(lattice(c.x), lattice(c.y))
}

/// Cut the closed line-and-arc `chain` with the wire, as true arcs.
///
/// # Errors
/// Every refusal of the module head and of the wire contour's, by name.
pub fn wire_contour_arcs(chain: &ArcChain, p: &WireParams) -> Result<Toolpath, GenError> {
    wire_contour_arcs_led(chain, p, WireLead::Straight)
}

/// [`wire_contour_arcs`] with the stated lead (#3991): the straight lead,
/// or a tangent arc lead-in and lead-out (`arc_lead`).
///
/// # Errors
/// As [`wire_contour_arcs`], and each refusal of the arc lead by name.
pub fn wire_contour_arcs_led(chain: &ArcChain, p: &WireParams, lead: WireLead) -> Result<Toolpath, GenError> {
    p.door()?;
    if p.tabs.is_some() {
        return Err(invalid("tabs are stated on a drawn line-and-arc profile - tabs are not cut on it yet; remove the tabs".into()));
    }
    let sign = match p.side {
        WireSide::Die => 1.0,
        WireSide::Punch => -1.0,
        WireSide::Left | WireSide::Right => {
            return Err(invalid(format!(
                "the side is {} of travel, but the profile is closed - a closed profile states its die or punch side",
                p.side.word()
            )));
        }
    };
    let k = p.kerf_offset();
    let (first, miss) = parallel_walk(chain.spans(), sign * k, p.sharp_corners)?;
    let snap = first
        .iter()
        .filter_map(|s| match *s {
            Span::Arc { c, .. } => Some(dist(c, on_lattice(c))),
            Span::Line { .. } => None,
        })
        .fold(0.0, f64::max);
    let shift = 0.5 * arc_gaps(&first.iter().copied().map(snapped).collect::<Vec<_>>());
    let margin = snap + chain.max_gap() + miss + shift;
    let margin = if margin > MARGIN_FLOOR { margin } else { 0.0 };
    let walk = if margin > 0.0 { parallel_walk(chain.spans(), sign * (k + margin), p.sharp_corners)?.0 } else { first };
    simple(&walk, k)?;
    let Some(start) = p.start else {
        return Err(invalid(format!(
            "no start hole is stated - a closed profile is cut from a threading point ({}); the wire cannot be threaded through solid material",
            match p.side {
                WireSide::Die => "a start hole in the slug, inside the profile",
                _ => "a start hole or the stock's edge, outside the profile",
            }
        )));
    };
    start_side(chain, start, p)?;
    let (seam, at, centred) = seam(&walk, start).ok_or_else(|| invalid("the wire path is empty".into()))?;
    let lead_len = dist(start, at);
    if !(lead_len.is_finite() && lead_len > MIN_LEAD) {
        return Err(GenError::invalid_at(
            format!(
                "wire contour: the start hole at X{:.3} Y{:.3} stands on the wire path itself - move it off the profile by more than the kerf offset ({k:.4} mm)",
                start.x, start.y
            ),
            at3(start, p.plane_z),
        ));
    }
    let seq = stitch(from_seam(&walk, seam, at).iter().flat_map(Span::halves).map(snapped).collect());
    let z = p.plane_z;
    let leads = match lead {
        WireLead::Straight => None,
        WireLead::Arc { radius } => Some(arc_leads(start, &seq, p.side == WireSide::Die, radius, chain, k, z)?),
    };
    let mut b = PathBuilder::new(at3(start, z));
    match &leads {
        Some(l) => l.lead_in.iter().for_each(|s| push_span(&mut b, s, z)),
        None => {
            if let Some(first) = seq.first() {
                b.push(MoveKind::Feed, at3(first.start(), z), None);
            }
        }
    }
    for s in &seq {
        push_span(&mut b, s, z);
    }
    match &leads {
        Some(l) => l.lead_out.iter().for_each(|s| push_span(&mut b, s, z)),
        None => b.push(MoveKind::Feed, at3(start, z), None),
    }
    let mut path = b.finish();
    if path.moves.len() < 4 {
        return Err(invalid("the wire path lays down no cut".into()));
    }
    arc_laws::kerf(&path, chain, k, z)?;
    let rest = arc_laws::cover(&path, chain, p)?;
    let mut said = super::pass_notes(p, start, lead_len, false);
    if let (Some(l), Some(line)) = (&leads, said.get_mut(1)) {
        *line = format!(
            "wire EDM: threaded at X{:.3} Y{:.3}; tangent arc lead-in of radius {:.4} mm (a tangent straight of {:.3} mm, then an arc onto the path's nearest point, met in the path's own direction), the profile cut counter-clockwise, the lead-out the mirror arc and a tangent straight back to the threading point; the two leads close a small waste piece between them, which drops free like the slug",
            start.x, start.y, l.radius, l.straight
        );
    }
    let arcs = chain.spans().iter().filter(|s| s.is_arc()).count();
    said.push(format!(
        "wire EDM: the drawn profile is read exactly - {} line(s) and {arcs} arc(s); the path holds {} arc move(s) (G2/G3 on a post that writes arcs), concentric with the drawing's arcs, with pivot arcs about outside corners",
        chain.spans().len() - arcs,
        path.arc_count()
    ));
    if centred {
        said.push("wire EDM: the start hole stands at the centre of the circular path - every path point is equally near, so by rule the lead enters towards +X".into());
    }
    if margin > 0.0 {
        said.push(format!(
            "wire EDM: the path stands {:.1} nm farther on the waste side - arc centres are written on the 1e-6 mm grid (up to {:.1} nm off the drawing's centres), the drawing's joints gap up to {:.1} nm, and near-tangent parallels are joined across up to {:.1} nm; this keeps the kerf out of the product",
            margin * 1e6,
            snap * 1e6,
            chain.max_gap() * 1e6,
            miss * 1e6
        ));
        if shift > 0.0 {
            said.push(format!(
                "wire EDM: neighbour arcs meet at the mean of their written ends, up to {:.1} nm off each arc's circle - the margin above holds it",
                shift * 1e6
            ));
        }
    }
    said.extend(rest.map(|r| laws::sharp_note(r, k)));
    path.notes.extend(said);
    Ok(path)
}

fn push_span(b: &mut PathBuilder, s: &Span, z: f64) {
    match *s {
        Span::Line { b: e, .. } => b.push(MoveKind::Feed, at3(e, z), None),
        Span::Arc { c, b: e, ccw, .. } => b.push_arc(MoveKind::Feed, at3(e, z), None, c, ccw),
    }
}

/// The walk must not cross itself: a crossing is a neck under the kerf
/// width, where the parallels of two far spans of the profile meet.
fn simple(walk: &[Span], k: f64) -> Result<(), GenError> {
    let sag = (k / 8.0).min(1e-3);
    let mut pts = Vec::new();
    for s in walk {
        pts.push(s.start());
        if let Span::Arc { r, sweep, .. } = *s {
            let step = 2.0 * (1.0 - sag / r).clamp(-1.0, 1.0).acos();
            let n = (sweep / step).ceil().clamp(2.0, 4096.0) as u32;
            pts.extend((1..n).map(|j| s.point_at(f64::from(j) / f64::from(n))));
        }
    }
    let neck = |at: Point2| {
        GenError::invalid_at(
            format!(
                "wire contour: the profile narrows to a neck under the kerf width ({:.4} mm) near X{:.3} Y{:.3} - the wire's path crosses itself there; widen the profile or cut with a thinner wire",
                2.0 * k,
                at.x,
                at.y
            ),
            at3(at, 0.0),
        )
    };
    let at = pts.first().copied().unwrap_or(Point2::new(0.0, 0.0));
    match Loop::new(pts).and_then(|l| Region::new(vec![l])) {
        Ok(r) if r.loops().len() == 1 => Ok(()),
        Ok(_) => Err(neck(at)),
        Err(GeoError::Invalid(w)) if w.contains("crosses itself") => Err(neck(at)),
        Err(e) => Err(invalid(format!("the wire path is unusable: {e}"))),
    }
}

/// The start hole stands on the waste side, at least the kerf offset off
/// the profile.
fn start_side(chain: &ArcChain, start: Point2, p: &WireParams) -> Result<(), GenError> {
    let k = p.kerf_offset();
    let clear = chain.spans().iter().map(|s| pt_span(start, s)).fold(f64::INFINITY, f64::min);
    let inside = chain.contains(start)?;
    let why = if clear < k {
        format!(
            "the start hole at X{:.3} Y{:.3} stands closer to the profile than the kerf offset ({k:.4} mm) - the wire would burn the {} there; move it away from the profile",
            start.x,
            start.y,
            p.side.word()
        )
    } else {
        match (p.side, inside) {
            (WireSide::Die, true) | (WireSide::Punch, false) => return Ok(()),
            (WireSide::Die, false) => format!(
                "the start hole at X{:.3} Y{:.3} lies outside the profile - a die is cut from a start hole inside it, in the slug",
                start.x, start.y
            ),
            _ => format!(
                "the start hole at X{:.3} Y{:.3} lies inside the profile - a punch is cut from outside it; the wire would burn through the punch",
                start.x, start.y
            ),
        }
    };
    Err(GenError::invalid_at(format!("wire contour: {why}"), at3(start, p.plane_z)))
}

/// The seam: the index of the walk's span nearest `start`, the point on
/// it, and whether `start` is the centre of that (circular) span. Equally
/// near points go to the least counter-clockwise angle from +X.
fn seam(walk: &[Span], start: Point2) -> Option<(usize, Point2, bool)> {
    let near: Vec<(usize, f64, Point2)> = walk.iter().enumerate().map(|(i, s)| { let (d, q) = nearest(start, s); (i, d, q) }).collect();
    let best = near.iter().map(|n| n.1).fold(f64::INFINITY, f64::min);
    let angle = |q: Point2| (q.y - start.y).atan2(q.x - start.x).rem_euclid(TAU);
    let (i, _, q) = near
        .into_iter()
        .filter(|n| n.1 <= best + 1e-9)
        .min_by(|a, b| angle(a.2).total_cmp(&angle(b.2)))?;
    let centred = matches!(walk[i], Span::Arc { c, .. } if dist(c, start) <= 1e-9);
    Some((i, q, centred))
}

/// The closed walk re-ordered to start and end at `at` on span `i`.
fn from_seam(walk: &[Span], i: usize, at: Point2) -> Vec<Span> {
    let s = walk[i];
    if let Span::Arc { c, r, ccw, .. } = s
        && s.full()
    {
        return vec![Span::Arc { c, r, a: at, b: at, ccw, sweep: TAU }];
    }
    let (before, after) = s.split(at);
    let n = walk.len();
    after.into_iter().chain((1..n).map(|j| walk[(i + j) % n])).chain(before).collect()
}

/// An arc moved onto its lattice centre: radius kept, ends taken along
/// their radii from that centre.
fn snapped(s: Span) -> Span {
    let Span::Arc { c, r, a, b, ccw, sweep } = s else { return s };
    let c2 = on_lattice(c);
    let onto = |q: Point2| unit(sub(q, c2)).map_or(q, |u| add(c2, mul(u, r)));
    Span::Arc { c: c2, r, a: onto(a), b: onto(b), ccw, sweep }
}

/// The widest gap (mm) between the end of an arc and the start of the
/// arc after it, the closing pair included, that [`stitch`] joins at
/// their mean.
fn arc_gaps(seq: &[Span]) -> f64 {
    let n = seq.len();
    (0..n)
        .filter_map(|i| {
            let (s, t) = (seq[i], seq[(i + 1) % n]);
            let g = dist(s.end(), t.start());
            joins(&s, &t, g).then_some(g)
        })
        .fold(0.0, f64::max)
}

/// Two arcs whose ends `g` apart meet at their mean: the gap is under
/// [`JOIN_MAX`], and the half gap is under a quarter of each arc's chord,
/// so no end crosses the other and no arc turns into a full circle.
fn joins(s: &Span, t: &Span, g: f64) -> bool {
    let chord = |x: &Span| dist(x.start(), x.end());
    s.is_arc() && t.is_arc() && g > 0.0 && g <= JOIN_MAX && 2.0 * g < chord(s) && 2.0 * g < chord(t)
}

/// Make each span start where the one before it ends: a line's end moves
/// to its neighbour arc's; two arcs that do not share their end meet at the
/// mean of the two ends ([`joins`], #3959); only two arcs farther apart are
/// joined by a line, a real move.
fn stitch(seq: Vec<Span>) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::with_capacity(seq.len());
    for mut s in seq {
        let ss = s.start();
        let mut joiner = None;
        if let Some(last) = out.last_mut()
            && last.end() != ss
        {
            let e = last.end();
            if let Span::Line { b, .. } = last {
                *b = ss;
            } else if let Span::Line { a, .. } = &mut s {
                *a = e;
            } else if joins(last, &s, dist(e, ss)) {
                let m = mul(add(e, ss), 0.5);
                if let (Span::Arc { b, .. }, Span::Arc { a, .. }) = (last, &mut s) {
                    *b = m;
                    *a = m;
                }
            } else {
                joiner = Some(Span::Line { a: e, b: ss });
            }
        }
        out.extend(joiner);
        out.push(s);
    }
    out
}
