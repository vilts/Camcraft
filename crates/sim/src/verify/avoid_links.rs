//! Rapid-link Z elevation over "Do not touch" protected items (#2055).
//! Provenance: our own link-obstruction test over `avoid::link_obstruction`;
//! no vendor source consulted.

use super::avoid::{AVOID_VERIFY_TOL, AvoidSpec};
use crate::cl::{Move, MoveKind, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::Point3;

/// Extra Z safety clearance added when elevating a rapid link (mm).
pub const AVOID_LINK_GAP: f64 = 0.5;

/// Elevate rapid links that pass horizontally across protected items.
/// Returns notes for any link blocked by `z_ceiling` or vertical overhang overlap.
pub fn elevate_rapid_links(
    path: &mut Toolpath,
    avoid: &AvoidSpec,
    z_ceiling: Option<f64>,
    extra_pad: f64,
) -> Vec<String> {
    if avoid.is_empty() || path.moves.is_empty() {
        return Vec::new();
    }
    let mut padded = avoid.clone();
    padded.clearance = avoid.clearance + extra_pad.max(0.0);
    let mut out = Vec::with_capacity(path.moves.len());
    let mut notes = Vec::new();

    for m in &path.moves {
        let is_upright = m.axis.is_none();
        let dxy = (m.to.x - m.from.x).hypot(m.to.y - m.from.y);
        if m.kind != MoveKind::Rapid || !is_upright || dxy <= 1e-6 {
            out.push(*m);
            continue;
        }
        let seg_only = Toolpath {
            moves: vec![*m],
            cycles: Vec::new(),
            notes: Vec::new(),
            comps: Vec::new(),
            threads: Vec::new(),
            probes: Vec::new(),
            breaks: Vec::new(),
            plasma: None,
        };
        let hits = match padded.hits_named(&seg_only, AVOID_VERIFY_TOL) {
            Ok(h) => h,
            Err(e) => {
                notes.push(e);
                out.push(*m);
                continue;
            }
        };
        if hits.is_empty() {
            out.push(*m);
            continue;
        }
        let Some((z_req, idx)) = avoid.link_obstruction(
            Point2::new(m.from.x, m.from.y),
            Point2::new(m.to.x, m.to.y),
            extra_pad,
        ) else {
            out.push(*m);
            continue;
        };
        let label = avoid
            .items
            .get(idx.saturating_sub(1))
            .map_or("protected item", |it| it.label.as_str());
        if let Some(ceil) = z_ceiling
            && z_req > ceil + 1e-6
        {
            notes.push(format!(
                "rapid link cannot clear protected '{label}' below Z {ceil:.3} mm (requires Z {z_req:.3} mm)"
            ));
            out.push(*m);
            continue;
        }
        let z_hi = m.from.z.max(m.to.z);
        if z_req <= z_hi + 1e-6 {
            out.push(*m);
            continue;
        }
        let z_safe = z_ceiling.map_or(z_req + AVOID_LINK_GAP, |c| (z_req + AVOID_LINK_GAP).min(c)).max(z_hi);
        let mut cand = Vec::with_capacity(3);
        let p_up = Point3::new(m.from.x, m.from.y, z_safe);
        let p_across = Point3::new(m.to.x, m.to.y, z_safe);
        if m.from.z < z_safe - 1e-9 {
            cand.push(Move { from: m.from, to: p_up, ..*m });
        }
        cand.push(Move {
            from: if cand.is_empty() { m.from } else { p_up },
            to: p_across,
            ..*m
        });
        if m.to.z < z_safe - 1e-9 {
            cand.push(Move { from: p_across, to: m.to, ..*m });
        }
        let cand_path = Toolpath { moves: cand.clone(), ..seg_only };
        match avoid.hits_named(&cand_path, AVOID_VERIFY_TOL) {
            Ok(ch) if ch.is_empty() => out.extend(cand),
            Ok(_) => {
                notes.push(format!(
                    "rapid link cannot elevate past overhead protected '{label}' at Z {z_safe:.3} mm"
                ));
                out.push(*m);
            }
            Err(e) => {
                notes.push(e);
                out.push(*m);
            }
        }
    }
    path.moves = out;
    path.notes.extend(notes.clone());
    notes
}
