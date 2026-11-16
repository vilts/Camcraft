//! 5-axis swarf (M5.1 stage D): the Z-following raster run along
//! the surface's own straight direction. [`super::ruling`] reads the
//! ruling off the mesh; [`crate::finishing3d::swarf`] lays the
//! passes along it (and the rings under the steep flanks, as always);
//! and the stamp poses each cut move WITH the blade - axis tangent
//! to the flank along the ruling - wherever a facet holds one, and
//! by stage C's lean law on the floors and domes between. A ball
//! tangent along a ruling touches a line, not a point, which is the
//! whole reason the family exists: the flank is swept, not scalloped.
//!
//! Advanced swarf adds the tolerance ladder: the same path family
//! run coarse-first at `tol·4^i` per pass (every scallop law in this
//! kernel is tolerance-authored, so the rough pass lands at double
//! the spacing and double the stepdown), finishing at the nominal
//! tolerance. Centers always stand tangent to the MODEL, never to
//! the previous pass - the finishing pass's promise cannot hide a
//! gouge the rough pass left behind.
//!
//! Provenance: swarf (side machining of ruled surfaces) and
//! multi-pass roughing/finishing ladders are standard CAM practice;
//! the developability test used to find the direction is textbook
//! differential geometry. Built on this codebase's own raster and
//! stamp. No vendor material consulted.

use crate::cl::Toolpath;
use crate::finishing3d::{self, Finish3DParams};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::entry_ramp;
use super::ruling;
use super::stamp::stamp_axes_ruled;
use super::stamp_link::{guard_turns, link_swings};
use super::tilt::AxisControl;

/// The swarf close: the stamp's poses, every hand-off that turns past
/// the link threshold taken through air ([`link_swings`]), and the
/// guard that proves no cut turns ([`guard_turns`]) - a swing in
/// contact refuses the program by name, never posts.
fn pose_and_link(
    base: &Toolpath,
    facets: &[Triangle],
    r: f64,
    c: &AxisControl,
    f: &ruling::Field,
    p: &Finish3DParams,
) -> Result<Toolpath, GenError> {
    let (led, said) = lead_in(base, facets, r, p);
    let posed = stamp_axes_ruled(&led, facets, r, c, Point3::new(f.ux, f.uy, 0.0), &f.ruled, p.tolerance)?;
    // #1015: the link laws judge a posed world path; it rides their refusals.
    let (mut linked, links) = link_swings(&posed, facets, r, p.clearance_z, p.plunge_feed).map_err(|e| e.with_ghost(&posed))?;
    guard_turns(&linked, p.clearance_z).map_err(|e| e.with_ghost(&linked))?;
    linked.notes.extend(said);
    if links > 0 {
        linked.notes.push(format!(
            "swarf: {links} pose hand-off(s) past {} deg linked through air along the contact normal",
            super::stamp_link::LINK_TURN_DEG
        ));
    }
    Ok(linked)
}

/// The first raster entry under the sloped-entry law (#15): with
/// `entry_ramp` on (the centre-cutting guard's retry), the plan's first
/// drop onto the first line becomes an air drop to the Top of Part Z
/// behind it and a ramp down the line's own tangent, proved clear of
/// the part ([`entry_ramp::lead_first_entry`]); the stamp then poses
/// it with the cut it lands into. With no proof the drop stays and the
/// guard judges it. Off, the plan is untouched (byte-identical).
fn lead_in(base: &Toolpath, facets: &[Triangle], r: f64, p: &Finish3DParams) -> (Toolpath, Option<String>) {
    if !p.entry_ramp {
        return (base.clone(), None);
    }
    let (led, ok) = entry_ramp::lead_first_entry(base, facets, r, p.top_z, p.ramp_angle, p.tolerance, p.plunge_feed);
    let said = Some(if ok {
        format!("swarf: the first raster entry ramps in down the line's tangent at {:.0} deg from the Top of Part Z", p.ramp_angle.min(45.0))
    } else {
        "swarf: the first raster entry has no proved room to ramp (part behind the line's start) - it drops straight".into()
    });
    (led, said)
}

/// The raster along the ruling, with the tool lying with the blade.
pub fn swarf5d(
    facets: &[Triangle],
    p: &Finish3DParams,
    c: &AxisControl,
) -> Result<Toolpath, GenError> {
    let f = ruling::analyze(facets)?;
    let r = p.tool_diameter * 0.5;
    let base = finishing3d::swarf(facets, p, f.ux, f.uy)?;
    pose_and_link(&base, facets, r, c, &f, p)
}

/// The same with the multi-pass ladder; `passes` is the caller's
/// count, refused by name here when it says nothing sensible.
pub fn swarf5d_advanced(
    facets: &[Triangle],
    p: &Finish3DParams,
    c: &AxisControl,
    passes: u32,
) -> Result<Toolpath, GenError> {
    let f = ruling::analyze(facets)?;
    let r = p.tool_diameter * 0.5;
    let base = finishing3d::swarf_multi(facets, p, f.ux, f.uy, passes)?;
    pose_and_link(&base, facets, r, c, &f, p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::{Move, MoveKind};
    use crate::multiaxis::stamp::stamp_axes_ruled;
    use crate::multiaxis::tilt::angle_between;

    // The exact identity of the ruled pose, on one row of a tube's
    // facets (every face normal stands perpendicular to the ruling):
    // a ladder of tangent centers marching along x-hat. Where the
    // facet says ruled, the axis IS the ruling - to the last bit the
    // cross products allow; where it does not, stage C's lean law
    // speaks and the pose keeps its 15 degrees off the normal.

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    #[test]
    fn the_ruled_pose_is_the_ruling_and_the_unruled_pose_is_the_lean() {
        let (rb, r, dth) = (12.0, 3.0, std::f64::consts::TAU / 32.0);
        let v = |j: usize, i: usize| {
            let t = dth * j as f64;
            p(7.5 * i as f64, rb * t.cos(), rb * t.sin())
        };
        let (a, b, c) = (v(4, 0), v(5, 0), v(5, 1));
        let (u, w) = (b - a, c - a);
        let mut n = u.cross(w);
        let nl = (n.x * n.x + n.y * n.y + n.z * n.z).sqrt();
        n = p(n.x / nl, n.y / nl, n.z / nl);
        if n.y * a.y + n.z * a.z < 0.0 {
            n = p(-n.x, -n.y, -n.z); // outward from the tube axis
        }
        let q0 = p((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0, (a.z + b.z + c.z) / 3.0);
        let facets = vec![Triangle::new(a, b, c)];
        // The stamp reads upright-ball TIPS: the tangent centres r
        // down Z (the pivot law re-derives each posed tip from them).
        let centers = [-2.0, 0.0, 2.0]
            .map(|t| p(q0.x + t + r * n.x, q0.y + r * n.y, q0.z + r * n.z - r));
        let path = Toolpath::new(
            centers
                .windows(2)
                .map(|w| Move::new(MoveKind::Feed, w[0], w[1], Some(800.0)))
                .collect(),
        );
        let xhat = p(1.0, 0.0, 0.0);
        let posed = stamp_axes_ruled(&path, &facets, r, &AxisControl::default(), xhat, &[true], 0.05)
            .unwrap();
        for m in &posed.moves {
            let a = m.axis.expect("a tangent ladder on a ruled facet poses");
            let av = p(a[0], a[1], a[2]);
            assert!(angle_between(av, xhat) < 1e-9, "the ruled pose left the ruling: {a:?}");
        }
        let c15 = AxisControl {
            lead_deg: 15.0,
            lag_deg: 0.0,
            max_tilt_deg: None,
        };
        let leaned = stamp_axes_ruled(&path, &facets, r, &c15, xhat, &[false], 0.05).unwrap();
        for m in &leaned.moves {
            let a = m.axis.expect("a tangent ladder poses by the lean law too");
            let av = p(a[0], a[1], a[2]);
            assert!(
                (angle_between(av, n) - 15f64.to_radians()).abs() < 1e-9,
                "the unruled pose broke the lead identity: {a:?} vs {n:?}"
            );
        }
    }
}
