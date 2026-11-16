//! The moves of a rest Adaptive plan (issue 2031): per cut depth, per
//! chain - a rapid on a travel plane to the proved-clear position, a
//! plunge down the proved-clear column, the chain's feeds, a rapid
//! straight up. Provenance: RS-274 / ISO 6983 motion vocabulary (G0
//! positioning on a clearance or retract plane, G1 for everything
//! below it); the planes are the pocket ladder's own (`levels`). Ours.
//!
//! Prime Directive: no rapid ends or runs below the top of the cut
//! except straight up out of where the tool has just cut; the chords
//! are posted exactly as they were measured (no arc is fitted over
//! them, so the posted path is the path that was read).

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::Region;
use crate::geometry::Point3;
use crate::machining2d::pocket::PocketParams;

use super::peel::LevelPlan;
use super::stock::RestStock;

pub(super) fn post(p: &PocketParams, levels: &[(f64, usize)], plans: &[(Region, LevelPlan)], stock: &RestStock) -> Toolpath {
    let clearance = p.clearance_z;
    let hop = p.retract_z.unwrap_or(clearance).min(clearance);
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, clearance));
    b.note(format!(
        "rest Adaptive is planned on the remaining stock of the earlier operations, proved down to Z{:.3} ({} floor(s)); it plunges only where that stock proves the cutter's footprint clear",
        stock.deepest(),
        stock.floors().len()
    ));
    let mut said: Vec<usize> = Vec::new();
    for &(z, at) in levels {
        let Some((_, plan)) = plans.get(at) else { continue };
        if !said.contains(&at) && !plan.chains.is_empty() {
            said.push(at);
            b.note(format!(
                "rest Adaptive from Z{z:.3}: {:.1} mm2 of rest material in {} passes, worst planned load {:.1}% of D against {:.1}%",
                plan.area,
                plan.passes,
                plan.worst * 100.0,
                p.optimal_load * 100.0
            ));
        }
        if let Some((mm2, at)) = plan.held_back {
            b.note_once(&format!(
                "rest Adaptive leaves {mm2:.2} mm2 of rest material where no pass holds the optimal load (near ({:.2}, {:.2})): a finishing pass takes it",
                at.x, at.y
            ));
        }
        for (ci, chain) in plan.chains.iter().enumerate() {
            let Some((e, rest)) = chain.split_first() else { continue };
            // A depth is reached over the clearance plane, a chain of
            // the same depth over the retract plane.
            let cur = b.at();
            let travel = cur.z.max(if ci == 0 { clearance } else { hop });
            b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, travel), None);
            b.push(MoveKind::Rapid, Point3::new(e.x, e.y, travel), None);
            let above = (p.top_z + p.links.approach.plunge_height.max(0.0)).min(travel).max(z);
            b.push(MoveKind::Rapid, Point3::new(e.x, e.y, above), None);
            b.push(MoveKind::Plunge, Point3::new(e.x, e.y, z), Some(p.plunge_feed));
            for q in rest {
                b.push(MoveKind::Feed, Point3::new(q.x, q.y, z), Some(p.feed));
            }
        }
    }
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, clearance), None);
    b.finish()
}
