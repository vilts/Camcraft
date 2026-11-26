//! The waterjet cut's moves (#1883): per contour in cut order, its
//! kerf ring and tab gaps, its plans, then per plan the jet-off travel
//! at the clearance plane, the descent to the cut plane and the lit
//! moves - and the kerf and cover laws over the whole cut.
//!
//! Provenance: see the module head (`super`). Our own code.

use crate::cl::{CompSpan, ContourRole, Move, MoveKind, PathBuilder, Toolpath};
use crate::geo2d::Region;
use crate::geometry::Point3;
use crate::machining2d::GenError;
use crate::sheet::laws::{cover_law, kerf_law};
use crate::sheet::lead::push_piece;
use crate::sheet::nest::Contour;
use crate::sheet::ring::kerf_ring;
use crate::sheet::{Lit, Voice};

use super::plan::{self, Plan};
use super::tabs::{self, Gap};
use super::WaterjetParams;

fn invalid(why: String) -> GenError {
    Voice::WATERJET.invalid(why)
}

/// Lay the contours down in `order` and certify the result.
pub(super) fn cut(cs: &[Contour], order: &[usize], part: &Region, p: &WaterjetParams) -> Result<Toolpath, GenError> {
    let r = p.kerf * 0.5;
    let tol = p.kerf_tol();
    let forbidden = if part.is_empty() { None } else { Some(part.offset((r - tol).max(0.0))?) };
    // Cut-order ids: contour `order[pos]` is C{pos}.
    let label = |pos: usize| format!("contour C{pos}");
    let mut rings: Vec<Option<Vec<_>>> = vec![None; cs.len()];
    for (pos, &k) in order.iter().enumerate() {
        let Some(c) = cs.get(k) else { continue };
        if c.role != ContourRole::Open {
            rings[k] = Some(kerf_ring(c, r, &label(pos), Voice::WATERJET)?);
        }
    }
    let gaps: Vec<Vec<Gap>> = match &p.tabs {
        Some(t) => tabs::lay(t, &rings, r)?,
        None => vec![Vec::new(); cs.len()],
    };
    let mut notes: Vec<String> = Vec::new();
    let mut plans: Vec<(usize, Plan)> = Vec::new();
    for (pos, &k) in order.iter().enumerate() {
        let Some(c) = cs.get(k) else { continue };
        let here = label(pos);
        match (&rings[k], gaps.get(k).filter(|g| !g.is_empty())) {
            (None, _) => plans.push((pos, plan::open(c, p, &mut notes, &here)?)),
            (Some(ring), None) => plans.push((pos, plan::closed(c, ring, p, forbidden.as_ref(), &mut notes, &here)?)),
            (Some(ring), Some(g)) => {
                let runs = tabs::stretches(ring, g, plan::ccw_of(c.role, p));
                for pl in plan::tabbed(c, &runs, p, forbidden.as_ref(), &mut notes, &here)? {
                    plans.push((pos, pl));
                }
            }
        }
    }
    let Some((_, first)) = plans.first() else {
        return Err(GenError::Empty("waterjet profile: no contour to cut".into()));
    };
    let feed = Some(p.feed);
    let mut b = PathBuilder::new(Point3::new(first.pierce.x, first.pierce.y, p.clearance_z));
    let mut lit: Vec<Lit> = Vec::with_capacity(plans.len());
    for (pos, pl) in &plans {
        let c = &cs[order[*pos]];
        if b.at().z < p.clearance_z {
            b.push(MoveKind::Rapid, Point3::new(b.at().x, b.at().y, p.clearance_z), None);
        }
        b.push(MoveKind::Rapid, Point3::new(pl.pierce.x, pl.pierce.y, p.clearance_z), None);
        b.push(MoveKind::Rapid, Point3::new(pl.pierce.x, pl.pierce.y, p.cut_z), None);
        let on = b.moves_len();
        for q in &pl.lit {
            push_piece(&mut b, *q, p.cut_z, feed, p.linearize);
        }
        if b.moves_len() == on {
            return Err(invalid(format!("contour C{pos} lays down no cut")).with_ghost_moves(b.moves()));
        }
        let off = b.moves_len() - 1;
        if let Some((side, prog)) = &pl.comp {
            let mut pb = PathBuilder::new(Point3::new(pl.pierce.x, pl.pierce.y, p.cut_z));
            for q in prog {
                push_piece(&mut pb, *q, p.cut_z, feed, p.linearize);
            }
            let programmed: Vec<Move> = pb.finish().moves;
            let span = CompSpan { side: *side, radius: r, wear: false, first: on, last: off, programmed };
            if !span.is_sane(b.moves()) {
                return Err(invalid(format!(
                    "contour C{pos}: the G41/G42 span does not chain onto its cut - cut it with the computer's offset"
                ))
                .with_ghost_moves(b.moves()));
            }
            b.push_comp(span);
        }
        let contour = u32::try_from(*pos).map_err(|_| invalid("too many contours".into()))?;
        lit.push(Lit { contour, role: c.role, on, off });
    }
    b.push(MoveKind::Rapid, Point3::new(b.at().x, b.at().y, p.clearance_z), None);
    let mut path = b.finish();
    let bridged = |pos: usize, q| order.get(pos).and_then(|k| gaps.get(*k)).is_some_and(|g| tabs::bridged(g, q));
    cover_law(&path, &lit, cs, order, (p.kerf, tol), Voice::WATERJET, &bridged).map_err(|e| e.with_ghost(&path))?;
    if gaps.iter().any(|g| !g.is_empty()) {
        let chords: Vec<_> = path.moves.iter().filter(|m| m.kind == MoveKind::Feed).flat_map(|m| m.chords(tol)).collect();
        let reach = tabs::bridge_reach(p.kerf, p.lead_in, p.lead_out);
        for (pos, &k) in order.iter().enumerate() {
            let (Some(c), Some(g)) = (cs.get(k), gaps.get(k)) else { continue };
            if let Err((no, q)) = tabs::bridge_law(g, &c.pts, &chords, r, reach) {
                return Err(invalid(format!(
                    "contour C{pos}: tab {no} holds nothing - a lit move cuts through its bridge near X{:.3} Y{:.3}, so the part falls free; move the tab or shorten the leads",
                    q.x, q.y
                ))
                .with_ghost(&path));
            }
        }
    }
    if let Some(f) = &forbidden {
        kerf_law(&path, &lit, f, Voice::WATERJET).map_err(|e| e.with_ghost(&path))?;
    } else {
        notes.push("waterjet: no part silhouette to judge - roles read by nesting, the kerf law not run".into());
    }
    path.notes.extend(head(cs, order, &gaps, plans.len(), p));
    path.notes.extend(notes);
    Ok(path)
}

/// The path's head notes: what was cut, in what order, how.
fn head(cs: &[Contour], order: &[usize], gaps: &[Vec<Gap>], pierces: usize, p: &WaterjetParams) -> Vec<String> {
    let holes = order.iter().filter(|&&k| cs.get(k).is_some_and(|c| c.role == ContourRole::Inner)).count();
    let tabs: usize = gaps.iter().map(Vec::len).sum();
    let mut out = vec![format!(
        "waterjet: {} contour(s) cut inner first ({holes} hole(s), {tabs} tab(s)), {pierces} pierce(s), kerf {:.3} mm {}",
        order.len(),
        p.kerf,
        if p.control_comp { "compensated in the control (G41/G42)" } else { "offset in the computer" }
    )];
    out.push(if p.reversed {
        "waterjet: cut direction REVERSED by the operation - outer contours counter-clockwise, holes clockwise".into()
    } else {
        "waterjet: outer contours clockwise, holes counter-clockwise".into()
    });
    out.push(format!(
        "waterjet: requested pierce time {:.2} s and abrasive delay {:.2} s at each pierce; jet and dwell records are not implemented",
        p.pierce_time, p.abrasive_delay
    ));
    if p.lead_in == 0.0 {
        out.push("waterjet: zero lead-in can place a pierce on the kerf ring at the part edge".into());
    }
    if let Some(t) = p.linearize {
        out.push(format!("waterjet: arcs sent as chords within {t:.4} mm - the control reads no G2/G3"));
    }
    out
}
