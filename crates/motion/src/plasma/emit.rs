//! The plasma cut's moves (#670): per contour in cut order, the torch-
//! off travel at the clearance plane, the descent to the cut plane,
//! the lit lead-in, contour and lead-out (or a small hole's centre
//! lead-in and overburn), and the torch spans that say so - then the
//! kerf law over every lit move of a closed contour.
//!
//! Provenance: see the module head (`super`): Hypertherm's direction
//! rule, the LinuxCNC Plasma Primer's small-hole rule, Tormach's
//! PathPilot G41/G42 entry rule; lead arcs are plain circle geometry.
//! Our own code.

use crate::cl::{CompSide, CompSpan, ContourRole, Move, MoveKind, PathBuilder, Toolpath, TorchSpan};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;
use crate::machining2d::{fit_arcs, GenError, Seg};

use super::nest::Contour;
use super::ring::{along, centroid, dist, kerf_ring, longest_mid, nearest, unit, walk_from};
use super::{header, PlasmaParams, SMALL_HOLE_THICKNESS};

/// The arc fit's budget on the kerf ring (mm): inside [`super::KERF_TOL`].
pub(super) const FIT_TOL: f64 = 0.005;
/// How many times a lead that finds no room is halved before refusing.
const LEAD_TRIES: u32 = 4;

/// One lit piece: a straight move to a point, or an arc to it.
#[derive(Clone, Copy, Debug)]
enum Piece {
    Line(Point2),
    Arc { to: Point2, center: Point2, ccw: bool },
}

/// One contour, planned: where the torch pierces, what it rides lit,
/// the feed factor of the small-hole rule, and the programmed part
/// contour under control compensation.
struct Plan {
    pierce: Point2,
    lit: Vec<Piece>,
    small: Option<f64>,
    comp: Option<(CompSide, Vec<Piece>)>,
}

fn invalid(why: String) -> GenError {
    GenError::Invalid(format!("plasma profile: {why}"))
}

fn segs_to_pieces(segs: Vec<Seg>, out: &mut Vec<Piece>) {
    for s in segs {
        match s {
            Seg::Line(qs) => out.extend(qs.into_iter().map(Piece::Line)),
            Seg::Arc { center, ccw, to } => out.push(Piece::Arc { to, center, ccw }),
        }
    }
}

/// The ride of a closed walk as pieces: arcs where the ring is round.
fn ride(walk: &[Point2]) -> Vec<Piece> {
    let mut out = Vec::new();
    segs_to_pieces(fit_arcs(walk, FIT_TOL), &mut out);
    out
}

fn cross(a: Point2, b: Point2) -> f64 {
    a.x * b.y - a.y * b.x
}

/// Does every point of the stretch keep the kerf off the part?
fn clear(forbidden: Option<&Region>, pts: &[Point2]) -> bool {
    forbidden.is_none_or(|f| pts.iter().all(|p| !f.contains(*p)))
}

/// Points along a quarter-arc lead (for the room test).
fn arc_samples(c: Point2, a: Point2, b: Point2, ccw: bool) -> Vec<Point2> {
    let (a0, a1) = ((a.y - c.y).atan2(a.x - c.x), (b.y - c.y).atan2(b.x - c.x));
    let mut sweep = a1 - a0;
    if ccw && sweep < 0.0 {
        sweep += std::f64::consts::TAU;
    } else if !ccw && sweep > 0.0 {
        sweep -= std::f64::consts::TAU;
    }
    let r = dist(c, a);
    (0..=16).map(|k| {
        let t = a0 + sweep * f64::from(k) / 16.0;
        Point2::new(c.x + r * t.cos(), c.y + r * t.sin())
    }).collect()
}

/// A lead at `s` along travel `t` with waste normal `n`: the lead-in
/// (`entry`) arriving at `s`, or the lead-out leaving it. Returns the
/// far end and the piece set, halving the length while it finds no
/// room; `None` when even the shortest try reaches the part.
#[allow(clippy::too_many_arguments)]
fn lead(s: Point2, t: Point2, n: Point2, len: f64, straight: bool, entry: bool, forbidden: Option<&Region>, notes: &mut Vec<String>, label: &str) -> Option<(Point2, Vec<Piece>)> {
    if len <= 0.0 {
        return Some((s, Vec::new()));
    }
    let tries = if straight { 1 } else { LEAD_TRIES };
    let mut l = len;
    for k in 0..tries {
        let (far, piece, samples) = if straight {
            let far = Point2::new(s.x + n.x * l, s.y + n.y * l);
            let samples: Vec<Point2> = (0..=8).map(|i| {
                let f = f64::from(i) / 8.0;
                Point2::new(s.x + (far.x - s.x) * f, s.y + (far.y - s.y) * f)
            }).collect();
            (far, if entry { Piece::Line(s) } else { Piece::Line(far) }, samples)
        } else {
            let r = 2.0 * l / std::f64::consts::PI;
            let c = Point2::new(s.x + n.x * r, s.y + n.y * r);
            let sign = if entry { -1.0 } else { 1.0 };
            let far = Point2::new(c.x + sign * t.x * r, c.y + sign * t.y * r);
            let ccw = cross(t, n) > 0.0;
            let (a, b) = if entry { (far, s) } else { (s, far) };
            (far, Piece::Arc { to: b, center: c, ccw }, arc_samples(c, a, b, ccw))
        };
        if clear(forbidden, &samples) {
            if k > 0 {
                notes.push(format!(
                    "plasma: {label} lead-{} shortened to {l:.3} mm - the stated {len:.3} mm found no room beside the part",
                    if entry { "in" } else { "out" }
                ));
            }
            return Some((far, vec![piece]));
        }
        l *= 0.5;
    }
    None
}

/// Plan one closed contour (the kerf ring, its start, leads or the
/// small-hole rule, and the programmed contour under G41/G42).
fn plan_closed(c: &Contour, p: &PlasmaParams, forbidden: Option<&Region>, notes: &mut Vec<String>, label: &str) -> Result<Plan, GenError> {
    let r = p.kerf * 0.5;
    let ring = kerf_ring(c, r, label)?;
    let inner = c.role == ContourRole::Inner;
    let ccw = inner != p.reversed;
    let eq_d = 2.0 * (c.area / std::f64::consts::PI).sqrt();
    if inner && (eq_d < p.small_hole_dia || eq_d < SMALL_HOLE_THICKNESS * p.thickness) {
        // The Primer's small-hole rule: pierce near the centre, lead
        // in perpendicular, no lead-out, the overburn, the slow feed.
        let cpt = centroid(&c.pts);
        let cut = Region::new(vec![Loop::new(ring.clone())?])?;
        if !cut.contains(cpt) || !clear(forbidden, &[cpt]) {
            return Err(invalid(format!(
                "{label} is a small hole whose centre stands outside its own cut - the perpendicular lead-in from the centre has no room"
            )));
        }
        let (seg, s) = nearest(&ring, cpt);
        let walk = walk_from(&ring, seg, s, ccw);
        let mut lit = vec![Piece::Line(s)];
        lit.extend(ride(&walk));
        lit.extend(along(&walk, p.overburn).into_iter().map(Piece::Line));
        if p.control_comp {
            notes.push(format!(
                "plasma: {label} (small hole) rides the computer's kerf offset - its rule has no lead-out to carry the G40 cancel"
            ));
        }
        return Ok(Plan { pierce: cpt, lit, small: Some(p.small_hole_feed), comp: None });
    }
    let (seg, s) = longest_mid(&ring);
    let walk = walk_from(&ring, seg, s, ccw);
    let (t_in, t_out) = (unit(s, walk[1]), unit(walk[walk.len() - 2], s));
    // The waste side: the ring's interior is on the left of a CCW walk;
    // a hole's waste is its interior, an outline's its exterior.
    let waste = |t: Point2| {
        let left = Point2::new(-t.y, t.x);
        if inner == ccw { left } else { Point2::new(t.y, -t.x) }
    };
    let (n_in, n_out) = (waste(t_in), waste(t_out));
    let straight = p.control_comp;
    let refuse = |end: &str| {
        invalid(format!(
            "{label}: the lead-{end} finds no room beside the part - shorten it, or cut this contour with the leads off"
        ))
    };
    let (pierce, lead_in) =
        lead(s, t_in, n_in, p.lead_in, straight, true, forbidden, notes, label).ok_or_else(|| refuse("in"))?;
    let (_, lead_out) =
        lead(s, t_out, n_out, p.lead_out, straight, false, forbidden, notes, label).ok_or_else(|| refuse("out"))?;
    let mut lit = lead_in.clone();
    lit.extend(ride(&walk));
    lit.extend(lead_out.iter().copied());
    let comp = if p.control_comp {
        // G41/G42: the control offsets the PART contour; the engage and
        // cancel blocks are the straight leads to and from it.
        let (nseg, s_nom) = nearest(&c.pts, s);
        let nominal = walk_from(&c.pts, nseg, s_nom, ccw);
        let far = lead_out.last().map_or(s, |q| match q {
            Piece::Line(to) | Piece::Arc { to, .. } => *to,
        });
        let mut prog = vec![Piece::Line(s_nom)];
        prog.extend(ride(&nominal));
        prog.push(Piece::Line(far));
        let left = cross(t_in, n_in) > 0.0;
        Some((if left { CompSide::Left } else { CompSide::Right }, prog))
    } else {
        None
    };
    Ok(Plan { pierce, lit, small: None, comp })
}

fn push_piece(b: &mut PathBuilder, q: Piece, z: f64, feed: Option<f64>) {
    match q {
        Piece::Line(to) => b.push(MoveKind::Feed, Point3::new(to.x, to.y, z), feed),
        Piece::Arc { to, center, ccw } => b.push_arc(MoveKind::Feed, Point3::new(to.x, to.y, z), feed, center, ccw),
    }
}

/// Lay the contours down in `order` and certify the result.
pub(super) fn cut(cs: &[Contour], order: &[usize], part: &Region, p: &PlasmaParams) -> Result<Toolpath, GenError> {
    let r = p.kerf * 0.5;
    // One tolerance for both laws (#1053): the kerf law's forbidden band
    // and the cover law's reach read the same model-aware figure, so
    // neither can fall back to the plain one alone.
    let tol = p.kerf_tol();
    let forbidden = if part.is_empty() { None } else { Some(part.offset((r - tol).max(0.0))?) };
    let mut id_of = vec![0u32; cs.len()];
    for (pos, &k) in order.iter().enumerate() {
        if let Some(slot) = id_of.get_mut(k) {
            *slot = u32::try_from(pos).map_err(|_| invalid("too many contours".into()))?;
        }
    }
    let mut notes: Vec<String> = Vec::new();
    let mut plans: Vec<(usize, Plan)> = Vec::new();
    for &k in order {
        let c = &cs[k];
        let label = format!("contour C{}", id_of[k]);
        let plan = if c.role == ContourRole::Open {
            notes.push(format!("plasma: open run C{} is cut on its centre line - no kerf side, no leads, not judged by the kerf law", id_of[k]));
            Plan { pierce: c.pts[0], lit: c.pts[1..].iter().copied().map(Piece::Line).collect(), small: None, comp: None }
        } else {
            plan_closed(c, p, forbidden.as_ref(), &mut notes, &label)?
        };
        plans.push((k, plan));
    }
    let Some((_, first)) = plans.first() else {
        return Err(GenError::Empty("plasma profile: no contour to cut".into()));
    };
    let mut b = PathBuilder::new(Point3::new(first.pierce.x, first.pierce.y, p.clearance_z));
    let mut rec = header(p);
    for (k, plan) in &plans {
        let c = &cs[*k];
        if b.at().z < p.clearance_z {
            b.push(MoveKind::Rapid, Point3::new(b.at().x, b.at().y, p.clearance_z), None);
        }
        b.push(MoveKind::Rapid, Point3::new(plan.pierce.x, plan.pierce.y, p.clearance_z), None);
        b.push(MoveKind::Rapid, Point3::new(plan.pierce.x, plan.pierce.y, p.cut_z), None);
        let on = b.moves_len();
        let feed = p.feed.map(|f| f * plan.small.unwrap_or(1.0));
        for q in &plan.lit {
            push_piece(&mut b, *q, p.cut_z, feed);
        }
        if b.moves_len() == on {
            // #871: part-way refusals hand back the contours cut so far.
            return Err(invalid(format!("contour C{} lays down no cut", id_of[*k])).with_ghost_moves(b.moves()));
        }
        let off = b.moves_len() - 1;
        if let Some((side, prog)) = &plan.comp {
            let mut pb = PathBuilder::new(Point3::new(plan.pierce.x, plan.pierce.y, p.cut_z));
            for q in prog {
                push_piece(&mut pb, *q, p.cut_z, feed);
            }
            let programmed: Vec<Move> = pb.finish().moves;
            let span = CompSpan { side: *side, radius: r, wear: false, first: on, last: off, programmed };
            if !span.is_sane(b.moves()) {
                return Err(invalid(format!(
                    "contour C{}: the G41/G42 span does not chain onto its cut - cut it with the computer's offset",
                    id_of[*k]
                ))
                .with_ghost_moves(b.moves()));
            }
            b.push_comp(span);
        }
        rec.torches.push(TorchSpan {
            contour: id_of[*k],
            role: c.role,
            parent: c.parent.and_then(|q| id_of.get(q).copied()),
            on,
            off,
            small: plan.small,
        });
    }
    b.push(MoveKind::Rapid, Point3::new(b.at().x, b.at().y, p.clearance_z), None);
    let mut path = b.finish();
    // #672: no edge may stand beyond the kerf's reach - a slot narrower
    // than the kerf that the offset bridged is refused, not left uncut.
    // #871: the laws judge the whole cut; a refusal hands it back.
    super::laws::cover_law(&path, &rec.torches, cs, order, (p.kerf, tol)).map_err(|e| e.with_ghost(&path))?;
    if let Some(f) = &forbidden {
        super::laws::kerf_law(&path, &rec.torches, f).map_err(|e| e.with_ghost(&path))?;
    } else {
        notes.push("plasma: no part silhouette to judge - roles read by nesting, the kerf law not run".into());
    }
    let holes = rec.torches.iter().filter(|t| t.role == ContourRole::Inner).count();
    let small = rec.torches.iter().filter(|t| t.small.is_some()).count();
    let mut head = vec![format!(
        "plasma: {} contour(s) cut inner first ({holes} hole(s), {small} by the small-hole rule), kerf {:.3} mm {}",
        rec.torches.len(),
        p.kerf,
        if p.control_comp { "compensated in the control (G41/G42)" } else { "offset in the computer" }
    )];
    head.push(if p.reversed {
        "plasma: cut direction REVERSED by the operation - outer contours counter-clockwise, holes clockwise (the square edge falls on the scrap side)".into()
    } else {
        "plasma: outer contours clockwise, holes counter-clockwise (the square edge on the part)".into()
    });
    head.extend(notes);
    path.notes.extend(head);
    if !rec.is_sane(&path.moves) {
        return Err(invalid("the torch spans do not sit on the moves - refused".into()).with_ghost(&path));
    }
    path.plasma = Some(rec);
    Ok(path)
}
