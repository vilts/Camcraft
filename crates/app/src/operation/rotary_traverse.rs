//! #3096: generate's seam to the live-tool first-traverse law (`verify::live_first_traverse`) for the rotary kinds that turn the bar on the spindle C under a radial live tool (a mill-turn, a lathe) - our own reading of the RS-274 / ISO 6983 lathe rapid and of the shop rule "never traverse through uncut stock or steel"; no vendor material consulted.
//!
//! A path that states the spindle C on its moves (`cl::stamp_c`) carries
//! `PARAM LiveTool`, and only a post that declares `liveTooling` (the
//! lathe post, `fanuc-lathe.cps`) writes it; the engine refuses every
//! mill post by name. That post's `liveMove` turns C to the first motion
//! record's angle with the turret at its index position, then sends X and
//! Z in one G00 to that record: the END of the path's first move. So the
//! order to judge is the lathe's, the one #3077 reads for the cross drill,
//! not the mill post's Z-then-X/Y order. The premise is #3077's: the
//! turret starts at the index position, the top of the X and Z travel
//! (decision #3096). A program that starts low is not covered.
//!
//! C stands at its angle before X and Z move, so the work and the steel
//! that turns with it (a chuck) are read in the part frame, about the
//! first point's own radius. The steel declared fixed (a tailstock, a
//! steady, the table) never turns: it is read in the machine frame, where
//! the station is the +X half-plane of the spindle line - the convention
//! of `verify::c_steel` (#486) for the same kinds.

use crate::cl::Toolpath;
use crate::fixture::FixtureBox;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::tools::Tool;
use crate::verify::{self, LiveHit, LiveSolids, LiveStart};

use super::cross_traverse::LiveWork;
use super::material::Z_TOL;
use super::OpParams;

/// Whether the live-tool law judges this rotary kind's first traverse:
/// the path states the spindle C (`stamped`) and the machine does not
/// post the kind in the mill order (`first_traverse::mill_order`: a
/// mill-turn or a lathe). A mill keeps #2965's swept law.
pub(super) fn answers(kind: crate::machine::Kind, lathe_tool: bool, params: &OpParams, stamped: bool) -> bool {
    stamped && params.kind().info().mode == verify::Mode::Rotary && !super::first_traverse::mill_order(kind, lathe_tool, params)
}

/// #3096: whether an UNSTAMPED rotary kind on a mill-turn answers to the
/// mill laws. With no C on its moves (a machine file that states no axis
/// line for its rotaries) the path carries no `PARAM LiveTool`, so the
/// mill post writes it in the mill order (Z, then X/Y, then the rotary
/// words), the premise of #2965's swept law.
pub(super) fn mill_post_rotary(kind: crate::machine::Kind, lathe_tool: bool, params: &OpParams, path: &Toolpath) -> bool {
    kind == crate::machine::Kind::MillTurn
        && !lathe_tool
        && params.kind().info().mode == verify::Mode::Rotary
        && !path.moves.iter().any(|m| m.rotary.is_some())
}

/// #3096: the rotary kinds that a lathe or a mill-turn setup brings under
/// a first-traverse law. Their op stamp takes one more word there, so a
/// cache judged without the law reads Stale once; no mill's stamp moves.
pub(super) fn newly_judged_on(kind: crate::machine::Kind, params: &OpParams) -> bool {
    matches!(kind, crate::machine::Kind::MillTurn | crate::machine::Kind::Lathe) && params.kind().info().mode == verify::Mode::Rotary
}

/// The work a rotary arm hands over: the source solid (`part`, the part
/// when `part_solid`, else the stock), the setup's kept and neighbour
/// parts, the stock (its facets, else its box), the steel split by
/// whether it turns (`fixture::workholding_for`) and the model excess.
pub(super) struct Work<'a> {
    pub part: &'a [Triangle],
    pub part_solid: bool,
    pub kept: &'a [Triangle],
    pub neighbours: &'a [Triangle],
    pub stock: Option<&'a [Triangle]>,
    pub stock_aabb: Option<(Point3, Point3)>,
    pub turning: &'a [FixtureBox],
    pub fixed: &'a [FixtureBox],
    pub excess: f64,
}

/// [`judge`] about the spindle line `ax` that the path was stamped about
/// (`rotary_gate::c_center`), over the arm's `work`.
pub(super) fn judge_about(path: &Toolpath, kind: &str, ax: &crate::multiaxis::axis::Axis, tool: &Tool, work: &Work<'_>) -> Result<(), GenError> {
    let spindle = super::rotary_gate::c_center(ax, kind)?;
    let own = if work.part_solid { "the part" } else { "the stock" };
    let stock = work.stock.filter(|s| !s.is_empty());
    let mut named: Vec<(&str, &[Triangle])> = vec![(own, work.part), ("a kept part", work.kept), ("a neighbour part", work.neighbours)];
    named.extend(stock.map(|s| ("the stock", s)));
    let live = LiveWork { solids: &named, stock_box: work.stock_aabb.filter(|_| stock.is_none()), turning: work.turning, fixed: work.fixed, excess: work.excess };
    judge(path, kind, spindle, tool, &live)
}

fn unjudged(kind: &str, why: &str) -> GenError {
    GenError::Invalid(format!(
        "{kind}: the first rapid from the turret's index position cannot be judged ({why}) - refused, never posted unjudged"
    ))
}

/// Refuse by name a rotary kind (`kind`, its label) whose first rapid,
/// from the turret's index position onto the station about the spindle
/// line through `spindle` (X, Y), can pass the stock, a part or the steel.
pub(super) fn judge(path: &Toolpath, kind: &str, spindle: [f64; 2], tool: &Tool, work: &LiveWork<'_>) -> Result<(), GenError> {
    let Some(first) = path.moves.first() else {
        return Err(unjudged(kind, "the path has no first move"));
    };
    let facets: Vec<&[Triangle]> = work.solids.iter().map(|(_, f)| *f).collect();
    let boxes: Vec<FixtureBox> = work.stock_box.map(|(min, max)| FixtureBox { min, max }).into_iter().collect();
    let part = LiveSolids { work: &facets, work_boxes: &boxes, turning: work.turning, fixed: &[] };
    // The fixed steel read exactly in the machine frame: as steel that
    // stands still about a start on the +X station.
    let machine = LiveSolids { work: &[], work_boxes: &[], turning: work.fixed, fixed: &[] };
    let bad = |_| unjudged(kind, "its first point, the spindle line, the tool stack or the model excess is not finite");
    // The post moves to the first move's end; its start is judged too,
    // for a post that moves to the stated start (as #3077's cross drill).
    for at in [first.to, first.from] {
        let r = (at.x - spindle[0]).hypot(at.y - spindle[1]);
        let s = LiveStart { start: at, spindle, tool: tool.geom() };
        let station = LiveStart { start: Point3::new(spindle[0] + r, spindle[1], at.z), ..s };
        let hit = match verify::live_first_traverse(&s, &part, Z_TOL, work.excess).map_err(bad)? {
            Some(h) => Some((h, false)),
            None => verify::live_first_traverse(&station, &machine, Z_TOL, 0.0).map_err(bad)?.map(|h| (h, true)),
        };
        let Some((hit, fixed)) = hit else { continue };
        let (what, along, z) = match hit {
            LiveHit::Work { index, along_mm, z_mm } => (work.solids.get(index).map_or("the stock", |(n, _)| *n), along_mm, z_mm),
            LiveHit::Steel { along_mm, z_mm, .. } if fixed => ("a fixed fixture (a tailstock, a steady or the machine table)", along_mm, z_mm),
            LiveHit::Steel { along_mm, z_mm, .. } => ("a fixture that turns with the part (a chuck)", along_mm, z_mm),
        };
        let reach = if along.is_finite() && z.is_finite() {
            format!("reaches out to {along:.3} from the spindle at Z{z:.3} there")
        } else {
            "has a point that cannot be read".into()
        };
        return Err(GenError::Invalid(format!(
            "{kind}: the first rapid comes from the turret's index position (the top of the X and Z travel) to radius \
             {r:.3} at Z{:.3}; the lathe post turns C to that point, then sends X and Z in one rapid, so the tool can \
             pass every radius from {r:.3} out, from Z{:.3} up, on the live-tool station, and {what} {reach} - raise \
             the clearance height, or move it clear of that band",
            at.z,
            at.z - 0.5 * tool.diameter
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::{Move, MoveKind};
    use crate::machine::Kind;
    use crate::operation::Height;

    fn contour() -> OpParams {
        OpParams::RotaryContour {
            tolerance: 0.05,
            clearance_z: Height::fixed(25.0),
            retract_z: None,
            top_z: Height::fixed(6.0),
            bottom_z: Height::fixed(-8.0),
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            lead_deg: 0.0,
            lag_deg: 0.0,
            max_tilt_deg: 60.0,
            stated_axis: None,
            picks: Default::default(),
        }
    }

    fn rapid(from: Point3, to: Point3, c: Option<f64>) -> Toolpath {
        let mut m = Move::new(MoveKind::Rapid, from, to, None).with_axis([1.0, 0.0, 0.0]);
        m.rotary = c;
        Toolpath::new(vec![m])
    }

    /// Which law reads a rotary kind: the live-tool law where the path
    /// states C and the machine does not post the mill order; the mill
    /// laws where an unstamped path posts through the mill post.
    #[test]
    fn the_stated_c_picks_the_lathe_order_and_no_c_the_mill_order() {
        let p = contour();
        let at = |x: f64| Point3::new(x, 0.0, 6.0);
        let (stamped, bare) = (rapid(at(30.0), at(8.0), Some(0.0)), rapid(at(30.0), at(8.0), None));
        assert!(answers(Kind::MillTurn, false, &p, true) && answers(Kind::Lathe, false, &p, true));
        assert!(!answers(Kind::Mill, false, &p, true), "a mill keeps the swept law");
        assert!(!answers(Kind::MillTurn, false, &p, false));
        assert!(mill_post_rotary(Kind::MillTurn, false, &p, &bare) && !mill_post_rotary(Kind::MillTurn, false, &p, &stamped));
        assert!(!mill_post_rotary(Kind::Mill, false, &p, &bare), "a mill answers by mill_order already");
        assert!(newly_judged_on(Kind::MillTurn, &p) && newly_judged_on(Kind::Lathe, &p) && !newly_judged_on(Kind::Mill, &p));
    }
}
