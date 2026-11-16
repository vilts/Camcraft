//! The thread mill's ride (#894): the moves of one feature - lead-in arc, helix, lead-out arc - per axial level and radial pass, from the plan `thread_mill` makes.
//!
//! Provenance: see [`super::thread_mill`] (public machine-tool handbook
//! thread milling, ISO 724 / ISO 965-1 depths, RS-274 arc senses).
//! Every arc is an XY arc with axial travel (a G2/G3 helix); the lead
//! arcs are tangent to the centre circle and travel axially at the
//! helix's own rate per unit of path.

use crate::cl::{lattice, MoveKind, PathBuilder, Toolpath};
use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::face::GenError;
use super::thread_mill::{HolePlan, Pass, ThreadHole, ThreadParams};

fn pt(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(lattice(x), lattice(y), lattice(z))
}

/// Where the tool stands before and after one pass's ride: the axis of
/// a hole; beside a boss, the quarter-arc's start and end, one tool
/// radius outside the centre circle.
fn stations(p: &ThreadParams, x: &ThreadHole, q: &Pass) -> (Point2, Point2, Point2) {
    if !p.external {
        return (x.xy, x.xy, Point2::new(x.xy.x + q.rho * 0.5, x.xy.y));
    }
    let a = p.tool_diameter * 0.5;
    let side = if p.ccw() { a } else { -a };
    let cx = x.xy.x + q.rho + a;
    (Point2::new(cx, x.xy.y - side), Point2::new(cx, x.xy.y + side), Point2::new(cx, x.xy.y))
}

/// One level of one pass: station, lead-in, `turns` helical turns in
/// half-arcs, lead-out, station. `low` is the level's lowest tip height.
fn ride(b: &mut PathBuilder, p: &ThreadParams, x: &ThreadHole, plan: &HolePlan, q: &Pass, low: f64) {
    let (s_in, s_out, lead_c) = stations(p, x, q);
    let zs = low + plan.lead_max;
    let ze = zs + f64::from(plan.turns) * p.pitch;
    let (z0, z1, z2, z3) =
        if p.rising() { (zs - q.lead_z, zs, ze, ze + q.lead_z) } else { (ze + q.lead_z, ze, zs, zs - q.lead_z) };
    let ccw = p.ccw();
    // The lead arcs turn with the orbit inside a hole (internal
    // tangency) and against it beside a boss (external tangency).
    let lead_ccw = ccw != p.external;
    // To the station at the height the tool already holds, then along
    // the axis (or beside the boss) to the lead-in's start: feeds, never
    // rapids, below the feature's top.
    let here = b.at();
    b.push(MoveKind::Feed, pt(s_in.x, s_in.y, here.z), Some(p.feed));
    b.push(MoveKind::Feed, pt(s_in.x, s_in.y, z0), Some(p.plunge_feed));
    let (ex, wx) = (x.xy.x + q.rho, x.xy.x - q.rho);
    b.push_arc(MoveKind::Feed, pt(ex, x.xy.y, z1), Some(q.feed), lead_c, lead_ccw);
    let halves = 2 * plan.turns;
    for j in 1..=halves {
        let z = z1 + (z2 - z1) * f64::from(j) / f64::from(halves);
        let px = if j % 2 == 1 { wx } else { ex };
        b.push_arc(MoveKind::Feed, pt(px, x.xy.y, z), Some(q.feed), x.xy, ccw);
    }
    b.push_arc(MoveKind::Feed, pt(s_out.x, s_out.y, z3), Some(q.feed), lead_c, lead_ccw);
}

/// Helical thread milling over a feature list. Per feature: rapid above
/// it, then for each radial pass and each axial level a feed to the
/// station, the lead-in arc, the helix, the lead-out arc; a feed back up
/// to the rapid height, and the link rapid.
pub fn thread_mill(p: &ThreadParams) -> Result<Toolpath, GenError> {
    p.validate()?;
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    let rising = p.rising();
    for (i, x) in p.holes.iter().enumerate() {
        let plan = HolePlan::of(p, x)?;
        let Some(first) = plan.passes.first() else {
            return Err(GenError::Invalid("thread ride has no pass".into()));
        };
        let (start, _, _) = stations(p, x, first);
        let travel = if i == 0 { p.clearance_z } else { p.link_z() };
        b.push(MoveKind::Rapid, pt(start.x, start.y, travel), None);
        b.push(MoveKind::Rapid, pt(start.x, start.y, plan.rapid_z), None);
        for q in &plan.passes {
            // A rising helix works its levels bottom-up, a falling one
            // top-down, so the tool never passes an uncut level.
            let levels: Box<dyn Iterator<Item = &f64>> =
                if rising { Box::new(plan.lows.iter()) } else { Box::new(plan.lows.iter().rev()) };
            for low in levels {
                ride(&mut b, p, x, &plan, q, *low);
            }
        }
        let end = b.at();
        b.push(MoveKind::Feed, Point3::new(end.x, end.y, lattice(plan.rapid_z)), Some(p.feed));
        b.push(MoveKind::Rapid, Point3::new(end.x, end.y, lattice(p.link_z())), None);
    }
    let end = b.at();
    b.push(MoveKind::Rapid, Point3::new(end.x, end.y, lattice(p.clearance_z)), None);
    let path = b.finish();
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}
