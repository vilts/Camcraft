//! The waterjet cut's plans (#1883): per contour, where the jet pierces
//! and what it rides - a closed contour with its leads (or the
//! tight-hole rule), a tabbed contour as led-in stretches between its
//! gaps, an open run on its stated side.
//!
//! Provenance: see the module head (`super`). Leads and rides are the
//! shared sheet kernel's ([`crate::sheet::lead`]). Our own code.

use crate::cl::{CompSide, ContourRole};
use crate::geo2d::{Loop, Point2, Region};
use crate::machining2d::GenError;
use crate::sheet::lead::{clear, cross, lead, ride, LeadAsk, Piece, LEAD_TRIES};
use crate::sheet::nest::Contour;
use crate::sheet::open::offset;
use crate::sheet::ring::{centroid, longest_mid, nearest, unit, walk_from};
use crate::sheet::Voice;

use super::{LeadShape, OpenSide, WaterjetParams};

/// One pierce and what the jet rides from it: the lit pieces, and the
/// programmed part contour under control compensation.
pub(super) struct Plan {
    pub pierce: Point2,
    pub lit: Vec<Piece>,
    pub comp: Option<(CompSide, Vec<Piece>)>,
}

fn invalid(why: String) -> GenError {
    Voice::WATERJET.invalid(why)
}

/// The waste side's normal for travel `t`: a hole's waste is its
/// interior, an outline's its exterior; a CCW walk has its ring's
/// interior on the left.
fn waste(t: Point2, inner: bool, ccw: bool) -> Point2 {
    if inner == ccw { Point2::new(-t.y, t.x) } else { Point2::new(t.y, -t.x) }
}

/// Is a contour of this role cut counter-clockwise?
pub(super) fn ccw_of(role: ContourRole, p: &WaterjetParams) -> bool {
    (role == ContourRole::Inner) != p.reversed
}

/// The leads of a stretch from `s` (travel `t_in`) to `e` (travel
/// `t_out`): the pierce and the lead pieces, or `None` when a lead
/// finds no room even shortened.
#[allow(clippy::type_complexity)]
fn leads(
    (s, t_in): (Point2, Point2),
    (e, t_out): (Point2, Point2),
    (inner, ccw): (bool, bool),
    p: &WaterjetParams,
    forbidden: Option<&Region>,
    notes: &mut Vec<String>,
    label: &str,
) -> Result<(Point2, Vec<Piece>, Vec<Piece>), &'static str> {
    let straight = p.control_comp || p.lead_shape == LeadShape::Straight;
    let tries = if p.control_comp { 1 } else { LEAD_TRIES };
    let ask_in = LeadAsk { s, t: t_in, n: waste(t_in, inner, ccw), len: p.lead_in, straight, entry: true };
    let ask_out = LeadAsk { s: e, t: t_out, n: waste(t_out, inner, ccw), len: p.lead_out, straight, entry: false };
    let (pierce, lead_in) = lead(ask_in, forbidden, tries, notes, ("waterjet", label)).ok_or("in")?;
    let (_, lead_out) = lead(ask_out, forbidden, tries, notes, ("waterjet", label)).ok_or("out")?;
    Ok((pierce, lead_in, lead_out))
}

/// Plan one closed contour with no tabs on its kerf `ring`.
pub(super) fn closed(c: &Contour, ring: &[Point2], p: &WaterjetParams, forbidden: Option<&Region>, notes: &mut Vec<String>, label: &str) -> Result<Plan, GenError> {
    let inner = c.role == ContourRole::Inner;
    let ccw = ccw_of(c.role, p);
    let (seg, s) = longest_mid(ring);
    let walk = walk_from(ring, seg, s, ccw);
    let (t_in, t_out) = (unit(s, walk[1]), unit(walk[walk.len() - 2], s));
    match leads((s, t_in), (s, t_out), (inner, ccw), p, forbidden, notes, label) {
        Ok((pierce, lead_in, lead_out)) => {
            let mut lit = lead_in;
            lit.extend(ride(&walk));
            lit.extend(lead_out.iter().copied());
            let comp = if p.control_comp {
                // G41/G42: the control offsets the PART contour; the
                // engage and cancel blocks are the straight leads.
                let (nseg, s_nom) = nearest(&c.pts, s);
                let nominal = walk_from(&c.pts, nseg, s_nom, ccw);
                let far = lead_out.last().map_or(s, |q| q.to());
                let mut prog = vec![Piece::Line(s_nom)];
                prog.extend(ride(&nominal));
                prog.push(Piece::Line(far));
                let left = cross(t_in, waste(t_in, inner, ccw)) > 0.0;
                Some((if left { CompSide::Left } else { CompSide::Right }, prog))
            } else {
                None
            };
            Ok(Plan { pierce, lit, comp })
        }
        Err(end) if inner => tight_hole(ring, (ccw, t_out), p, forbidden, notes, label).map_err(|e| {
            invalid(format!("{label}: the lead-{end} finds no room beside the part, and {e}"))
        }),
        Err(end) => Err(invalid(format!(
            "{label}: the lead-{end} finds no room beside the part - shorten it, or cut this contour with the leads off"
        ))),
    }
}

/// The tight-hole rule: a hole too tight for its leads, even shortened,
/// is pierced at its centre and led in straight to its ring; the
/// lead-out is kept only where it finds room.
fn tight_hole(ring: &[Point2], (ccw, _t_out): (bool, Point2), p: &WaterjetParams, forbidden: Option<&Region>, notes: &mut Vec<String>, label: &str) -> Result<Plan, String> {
    let cpt = centroid(ring);
    let cut = Loop::new(ring.to_vec()).and_then(|l| Region::new(vec![l])).map_err(|e| format!("its ring is unusable: {e}"))?;
    if !cut.contains(cpt) || !clear(forbidden, &[cpt]) {
        return Err("its centre stands outside its own cut - a centre pierce has no room either".into());
    }
    let (seg, s) = nearest(ring, cpt);
    let walk = walk_from(ring, seg, s, ccw);
    let t_out = unit(walk[walk.len() - 2], s);
    let mut lit = vec![Piece::Line(s)];
    lit.extend(ride(&walk));
    let mut quiet = Vec::new();
    let ask_out = LeadAsk { s, t: t_out, n: waste(t_out, true, ccw), len: p.lead_out, straight: true, entry: false };
    if let Some((_, out)) = lead(ask_out, forbidden, LEAD_TRIES, &mut quiet, ("waterjet", label)) {
        lit.extend(out);
    }
    notes.push(format!(
        "waterjet: {label} is a tight hole - pierced at its centre and led in straight, its arc leads have no room"
    ));
    if p.control_comp {
        notes.push(format!("waterjet: {label} (tight hole) rides the computer's kerf offset - G41/G42 needs room for its leads"));
    }
    Ok(Plan { pierce: cpt, lit, comp: None })
}

/// Plan the stretches of a tabbed contour, each led in and out.
///
/// Every stretch end stands next to a tab gap, so its leads are always
/// straight along the waste normal. An arc lead-in comes from behind
/// its start and an arc lead-out runs on past its end - into the gap -
/// and the two kerfs under one tab can meet and cut the tab free of the
/// sheet. Straight leads leave the tab's full width between them.
pub(super) fn tabbed(c: &Contour, stretches: &[Vec<Point2>], p: &WaterjetParams, forbidden: Option<&Region>, notes: &mut Vec<String>, label: &str) -> Result<Vec<Plan>, GenError> {
    let inner = c.role == ContourRole::Inner;
    let ccw = ccw_of(c.role, p);
    if p.lead_shape == LeadShape::Arc && (p.lead_in > 0.0 || p.lead_out > 0.0) {
        notes.push(format!(
            "waterjet: {label} (tabbed) is led in and out straight - an arc lead beside a tab would cut into the tab's bridge"
        ));
    }
    let straight = WaterjetParams { lead_shape: LeadShape::Straight, ..p.clone() };
    let p = &straight;
    let mut out = Vec::with_capacity(stretches.len());
    for (j, run) in stretches.iter().enumerate() {
        let n = run.len();
        let (s, e) = (run[0], run[n - 1]);
        let (t_in, t_out) = (unit(s, run[1]), unit(run[n - 2], e));
        let here = format!("{label} stretch {}", j + 1);
        let (pierce, lead_in, lead_out) = leads((s, t_in), (e, t_out), (inner, ccw), p, forbidden, notes, &here)
            .map_err(|end| invalid(format!("{here}: the lead-{end} beside a tab finds no room - move the tab, or shorten the lead")))?;
        let mut lit = lead_in;
        lit.extend(ride(run));
        lit.extend(lead_out);
        out.push(Plan { pierce, lit, comp: None });
    }
    if p.control_comp {
        notes.push(format!("waterjet: {label} (tabbed) rides the computer's kerf offset - each stretch is too short a G41/G42 span to trust"));
    }
    Ok(out)
}

/// Plan an open run on the stated side.
pub(super) fn open(c: &Contour, p: &WaterjetParams, notes: &mut Vec<String>, label: &str) -> Result<Plan, GenError> {
    let r = p.kerf * 0.5;
    let pts = match p.open_side {
        OpenSide::Center => c.pts.clone(),
        OpenSide::Left => offset(&c.pts, r, p.cut_z, Voice::WATERJET)?,
        OpenSide::Right => offset(&c.pts, -r, p.cut_z, Voice::WATERJET)?,
    };
    let Some((&pierce, rest)) = pts.split_first() else {
        return Err(invalid(format!("{label} has no length - there is nothing to cut along")));
    };
    let side = match p.open_side {
        OpenSide::Center => "on its centre line",
        OpenSide::Left => "with the kerf left of its travel",
        OpenSide::Right => "with the kerf right of its travel",
    };
    notes.push(format!("waterjet: {label} is an open run, cut {side} - pierced at its start, no leads, not judged by the kerf law"));
    Ok(Plan { pierce, lit: rest.iter().copied().map(Piece::Line).collect(), comp: None })
}
