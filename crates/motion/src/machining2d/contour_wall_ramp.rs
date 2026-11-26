//! The wall side's ramp entry: a contour with `side` outside (or auto)
//! and `entry` ramp or helix descends by the zig-zag ramp along the
//! lead-in and the ring, at the entry's ramp angle, instead of the
//! straight dive at the lead-in. Every point of the zig-zag lies on the
//! lead-in or the ring, the very path that the level cuts at full depth
//! after it, so the ramp is never nearer to the part than the cut itself
//! and needs no chord or orbit proved over the grown region (which holds
//! the part's own interior). A helix needs bounded air that the wall
//! side does not have, so it stands down to this ramp, named.
//!
//! Provenance: standard zig-zag ramp entry of published CAM practice and
//! machining handbooks (a linear ramp along the toolpath); no vendor
//! material consulted.

use crate::cl::PathBuilder;
use crate::entry::Entry3D;
use crate::geo2d::Point2;
use crate::links::DEFAULT_RAMP_ANGLE;

use super::arcs::Seg;
use super::contour_bore::{level_angle, zig_onto};
use super::contour_entry::{Engage, flatten};
use super::contour_step::Step;

/// Post the ramp of one wall-side level onto `(s.e2, s.z)`: `true` when it
/// stands (the tool ends on the lead-in's start at the cut plane), `false`
/// with nothing posted when the lead-in and ring are too short for the
/// angle, so the caller grazes by name.
pub(super) fn wall_ramp(b: &mut PathBuilder, eng: &Engage, s: &Step, tz: f64, lead: &[Seg], walk: &[Seg]) -> bool {
    let p = eng.p;
    let a = &p.links.approach;
    // The bore side's heights, read the same way: the plane above, or the
    // top of the cut on the first engagement.
    let fresh_top = s.first || s.prev_z >= p.clearance_z;
    let top = if fresh_top { p.entry_top().min(s.prev_z) } else { s.prev_z };
    let from = if fresh_top { (top + a.plunge_height).min(s.prev_z) } else { s.prev_z };
    let zf = if a.ramp_to_depth { (p.entry_top() + a.plunge_height).min(s.prev_z) } else { from };
    let angle = if a.ramp_zigs() {
        a.ramp_angle
    } else if !s.first && p.links.transition == Some(Entry3D::Ramp) {
        level_angle(p.links.ramp_angle)
    } else {
        DEFAULT_RAMP_ANGLE
    };
    let path = flatten(s.e2, &[lead, walk].concat());
    let cur = b.at();
    zig_onto(b, &path, Point2::new(cur.x, cur.y), tz, zf, s.z, angle, a.ramp_max_len, a.descent_feed(p.feed))
}
