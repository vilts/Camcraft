//! #3077: generate's seam to the live-tool first-traverse law (`verify::live_first_traverse`) - the cross drill on a lathe or a mill-turn, read from the turret's index position; our own reading of the RS-274 / ISO 6983 lathe rapid, no vendor material consulted.
//!
//! The lathe post (`fanuc-lathe.cps` `liveMove`) positions C, then sends
//! X and Z in one G00 to the first motion record: the END of the path's
//! first move (the R-plane point of the first hole). The start of that
//! move (the kernel's standoff) reaches the post only as the StartX/Y/Z
//! parameters, which `liveMove` does not move to. The control knows the
//! turret only at its index position, the top of the X and Z travel
//! (decision #3077, the premise of #2987 and of the upright law). The
//! end is judged first; the start is judged too, for a post that moves
//! to the stated start (on the same station line its swept solid lies
//! inside the end's, so it refuses nothing more there).

use crate::cl::Toolpath;
use crate::fixture::FixtureBox;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::tools::Tool;
use crate::verify::{self, LiveHit, LiveSolids, LiveStart};

use super::material::Z_TOL;

/// What the cross drill's first traverse is judged against: the stock
/// and the parts by name (`solids`), a stock known only by its box, the
/// setup's steel split by whether it turns (`fixture::workholding_for`),
/// and the work's model excess (a coarse model, #842).
pub(super) struct LiveWork<'a> {
    pub solids: &'a [(&'a str, &'a [Triangle])],
    pub stock_box: Option<(Point3, Point3)>,
    pub turning: &'a [FixtureBox],
    pub fixed: &'a [FixtureBox],
    pub excess: f64,
}

fn unjudged(why: &str) -> GenError {
    GenError::Invalid(format!(
        "cross drill: the first rapid from the turret's index position cannot be judged ({why}) - refused, never posted unjudged"
    ))
}

/// Refuse by name a cross drill whose first rapid, from the turret's
/// index position about the spindle line through `origin` (the work
/// offset), can pass the stock, a part or the steel.
pub(super) fn judge(path: &Toolpath, origin: [f64; 3], tool: &Tool, work: &LiveWork<'_>) -> Result<(), GenError> {
    let Some(first) = path.moves.first() else {
        return Err(unjudged("the path has no first move"));
    };
    let facets: Vec<&[Triangle]> = work.solids.iter().map(|(_, f)| *f).collect();
    let boxes: Vec<FixtureBox> = work.stock_box.map(|(min, max)| FixtureBox { min, max }).into_iter().collect();
    let solids = LiveSolids { work: &facets, work_boxes: &boxes, turning: work.turning, fixed: work.fixed };
    for at in [first.to, first.from] {
        let s = LiveStart { start: at, spindle: [origin[0], origin[1]], tool: tool.geom() };
        let hit = verify::live_first_traverse(&s, &solids, Z_TOL, work.excess)
            .map_err(|_| unjudged("its first point, the spindle line, the tool stack or the model excess is not finite"))?;
        let Some(hit) = hit else { continue };
        let (what, along, z) = match hit {
            LiveHit::Work { index, along_mm, z_mm } => (work.solids.get(index).map_or("the stock", |(n, _)| *n), along_mm, z_mm),
            LiveHit::Steel { turning: true, along_mm, z_mm, .. } => ("a fixture that turns with the part (a chuck)", along_mm, z_mm),
            LiveHit::Steel { turning: false, along_mm, z_mm, .. } => ("a fixed fixture (a tailstock or the machine table)", along_mm, z_mm),
        };
        let r = (at.x - origin[0]).hypot(at.y - origin[1]);
        let reach = if along.is_finite() && z.is_finite() {
            format!("reaches out to {along:.3} from the spindle at Z{z:.3} there")
        } else {
            "has a point that cannot be read".into()
        };
        return Err(GenError::Invalid(format!(
            "cross drill: the first rapid comes from the turret's index position (the top of the X and Z travel) \
             to radius {r:.3} at Z{:.3}; the post sends X and Z in one rapid after C, so the tool can pass every \
             radius from {r:.3} out, from Z{:.3} up, in the station's plane, and {what} {reach} - move it clear of \
             that band, raise the clearance height, or drill these holes in a setup where nothing stands beyond them",
            at.z,
            at.z - 0.5 * tool.diameter
        )));
    }
    Ok(())
}
