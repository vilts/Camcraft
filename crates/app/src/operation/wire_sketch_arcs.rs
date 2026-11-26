//! A selected DXF wire chain cut on its own lines and arcs (#3937): any
//! chain that is not an exact polygon - one with an arc, a circle or a
//! joint gap under the stated tolerance - goes to the motion crate's
//! analytic wire contour, and the verifier reads its product on a
//! waste-side ring. TEXT, MTEXT and DIMENSION entities of the drawing no
//! longer block the cut; they are counted and said on the path.
//!
//! Provenance: our own glue over `wire_edm::wire_contour_arcs` (whose
//! module head cites the public sources) and the selected-chain door
//! (#3180).

use crate::cl::{Move, Toolpath};
use crate::geo2d::{Loop, Point2};
use crate::machining2d::GenError;
use crate::sketch_data::SketchData;
use crate::wire_edm::{wire_contour_arcs_led, ArcChain, WireLead};

use super::wire_op::{wire_params, WireCondition};
use super::wire_sketch::{single_pass, SelectedWireChain};
use super::{ResolvedHeights, WireContourOp, WireSide};

/// The skipped DXF entities that are annotation, never geometry.
const ANNOTATIONS: [&str; 3] = ["TEXT", "MTEXT", "DIMENSION"];

/// The count of annotation entities the import skipped. Any other
/// skipped entity (a HATCH, say) may be geometry whose clearance from the
/// wire cannot be checked: refused by name.
pub(super) fn annotations(data: &SketchData) -> Result<usize, String> {
    let mut ignored: usize = 0;
    for (kind, n) in &data.summary().skipped {
        if !ANNOTATIONS.contains(&kind.as_str()) {
            return Err(format!(
                "the DXF import skipped {n} {kind} entit{} whose clearance from the wire cannot be checked",
                if *n == 1 { "y" } else { "ies" }
            ));
        }
        ignored = ignored.saturating_add(*n);
    }
    if ignored != data.skipped_total() {
        return Err("the DXF import skipped entities whose clearance from the wire cannot be checked".into());
    }
    Ok(ignored)
}

/// The points that bound move `m` in XY: its ends, and for an XY arc each
/// axis extreme of its circle that the arc passes.
pub(super) fn extremes(m: &Move) -> Vec<Point2> {
    let (a, b) = (Point2::new(m.from.x, m.from.y), Point2::new(m.to.x, m.to.y));
    let mut out = vec![a, b];
    let Some(arc) = m.arc.filter(|a| a.plane.is_xy()) else { return out };
    let c = arc.center;
    let r = (a.x - c.x).hypot(a.y - c.y).max((b.x - c.x).hypot(b.y - c.y));
    let turn = |q: Point2| {
        let t = (q.y - c.y).atan2(q.x - c.x) - (a.y - c.y).atan2(a.x - c.x);
        (if arc.ccw { t } else { -t }).rem_euclid(std::f64::consts::TAU)
    };
    let sweep = turn(b);
    let sweep = if sweep > 0.0 { sweep } else { std::f64::consts::TAU };
    for (dx, dy) in [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)] {
        let q = Point2::new(c.x + r * dx, c.y + r * dy);
        if turn(q) <= sweep {
            out.push(q);
        }
    }
    out
}

/// The tangent arc lead beside what it is not cut with (#3991): the arc
/// lead is cut on a vertical, single-pass closed profile whose wire
/// returns to the start hole. Each other cut is refused by name at the
/// command door and at generation, never cut with a straight lead
/// silently. `None` when the op states no arc lead or none of those.
#[must_use]
pub fn lead_arc_refusal(w: &WireContourOp) -> Option<String> {
    w.lead_arc_radius?;
    let with = if matches!(w.side, WireSide::Left | WireSide::Right) {
        "an open run - it is threaded outside the stock, with no start hole to lead from".to_owned()
    } else if w.four_axis.is_some() {
        "a 4-axis cut".to_owned()
    } else if w.burnout.is_some() {
        "a no-core burnout".to_owned()
    } else if w.tabs.is_some() {
        "tabs".to_owned()
    } else if !w.skims.is_empty() {
        format!("{} skim pass(es)", w.skims.len())
    } else if w.taper_deg != 0.0 {
        format!("a {:.3} degree taper", w.taper_deg)
    } else if w.picks.first(crate::path_picks::PickRole::Exit).is_some() {
        "an exit point - the arc lead-out returns to the start hole; the exit point keeps the straight lead-out".to_owned()
    } else {
        return None;
    };
    Some(format!(
        "Wire Contour: a tangent arc lead is stated with {with}; the arc lead is cut on a vertical single-pass closed profile - set the straight lead or clear that setting"
    ))
}

/// The tangent arc lead radius against the resolved condition's kerf
/// offset (#3991, Validate early): the radius must be larger than the wire
/// radius plus the spark gap, or the lead arc is no smoother than a
/// corner. The command door reads this after the condition resolves; the
/// generator keeps its own check for a loaded file or a changed
/// condition. `None` when the op states no arc lead or a larger radius.
#[must_use]
pub fn lead_arc_radius_refusal(w: &WireContourOp, cond: &WireCondition) -> Option<String> {
    let radius = w.lead_arc_radius?;
    let k = cond.wire_diameter * 0.5 + cond.gap;
    (!(radius.is_finite() && radius > k)).then(|| {
        format!(
            "Wire Contour: the tangent lead arc radius ({radius} mm) must be larger than the kerf offset ({k:.4} mm) of condition {} (wire {} mm, gap {} mm) - set a larger radius or the straight lead",
            cond.id, cond.wire_diameter, cond.gap
        )
    })
}

impl SelectedWireChain {
    /// The chain as lines and arcs in world XY.
    fn arc_chain(&self) -> Result<ArcChain, GenError> {
        ArcChain::from_closed(&self.chain, |p| self.world_xy(p))
    }

    /// The product's section for the wire laws: the exact polygon, or the
    /// analytic chain on a waste-side ring within the laws' arc sagitta.
    pub(super) fn verify_ring(&self, w: &WireContourOp) -> Result<Loop, String> {
        if self.exact_polygon() {
            return self.nominal_ring(w);
        }
        single_pass(w)?;
        self.arc_chain()
            .and_then(|c| c.waste_ring(w.side, crate::verify::wire::ARC_SAG))
            .map_err(|e| e.to_string())
    }

    /// Cut a chain that is not an exact polygon on its own lines and arcs.
    /// `None` for an exact polygon, which keeps the polygon path.
    pub(super) fn generate_arcs(
        &self,
        op: &WireContourOp,
        cond: &WireCondition,
        hs: &ResolvedHeights,
    ) -> Result<Option<Toolpath>, GenError> {
        // #3991: an exact polygon keeps the polygon path, byte for byte,
        // unless a tangent arc lead is stated - that lead is cut here.
        if self.exact_polygon() && op.lead_arc_radius.is_none() {
            return Ok(None);
        }
        single_pass(op).map_err(GenError::Invalid)?;
        // #3332: the exit pick is read on a polygon profile or an open
        // run; on drawn lines and arcs it is refused by name, never dropped.
        if op.picks.first(crate::path_picks::PickRole::Exit).is_some() {
            return Err(GenError::Invalid(
                "wire contour: an exit point is stated on a drawn profile with arcs - the exit point is taken on a polygon profile or an open run so far; clear the exit point".into(),
            ));
        }
        if !cond.skims.is_empty() || !matches!(op.side, WireSide::Die | WireSide::Punch) {
            return Err(GenError::Invalid(
                "selected DXF wire cuts require a single vertical pass with slug capture and no tabs".into(),
            ));
        }
        let chain = self.arc_chain()?;
        let p = wire_params(op, cond, None, hs, None)?;
        let lead = op.lead_arc_radius.map_or(WireLead::Straight, |radius| WireLead::Arc { radius });
        wire_contour_arcs_led(&chain, &p, lead).map(Some)
    }

    /// Say on the path how many annotation entities were not cut.
    pub(super) fn annotate(&self, path: &mut Toolpath) {
        if self.ignored > 0 {
            path.notes.push(format!(
                "wire EDM: {} TEXT, MTEXT or DIMENSION entit{} of the drawing ignored - annotation is not cut, and the wire is not checked against it",
                self.ignored,
                if self.ignored == 1 { "y" } else { "ies" }
            ));
        }
    }
}
