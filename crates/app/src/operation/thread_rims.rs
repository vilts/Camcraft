//! The thread mill's region and solid-law grant inputs (#1071): each declared hole's rim moved out to the thread's major circle, and the bores the solid law excuses - our own reading of the ISO 724 minor/major relation the generator cites (`machining2d::thread_mill`); no vendor material consulted.

use super::material_source::{disk_loop, rim_is};
use super::*;

/// A thread mill's region with each hole's rim moved out to the thread's
/// MAJOR circle (#1071): the declared hole is the minor cylinder, and the
/// helix runs `depth` past it by design (`machining2d::thread_mill`), so
/// the circle the gouge law must hold a matched cutter to is the minor
/// circle grown by the thread depth. Any loop that is a hole's minor rim
/// (ours, or the model's own for a through hole) gives way to it; every
/// other wall stays as it was.
///
/// The minor circle is the declared one, unless the model's own rim loop
/// stands nearer the declared centre (PR #1099 review): then the major
/// circle is grown from that loop's nearest point - an edge's middle, not
/// only its vertices (PR #1099 third review: a D-bore's flat or a coarse
/// polygon's side dips inside the circle its corners stand on) - so a
/// declared hole wider than, or off the centre of, the model's own rim is
/// held to the model's major circle, not its own. On a reduced-fidelity model the rim
/// moves by exactly `depth` with no excess term: the containment solid law
/// (whose allowance reads the excess) is the backstop there.
pub(super) fn major_rims(region: Region, holes: &[Hole], depth: f64) -> Result<Region, GenError> {
    if holes.is_empty() {
        return Ok(region);
    }
    if !(depth.is_finite() && depth >= 0.0) {
        return Err(GenError::Invalid(format!("thread depth {depth} is not a length")));
    }
    let mut minor: Vec<f64> = holes.iter().map(|h| h.radius).collect();
    let mut loops: Vec<Loop> = Vec::new();
    for l in region.loops() {
        let mut theirs = false;
        for (k, h) in holes.iter().enumerate() {
            if rim_is(l, *h) {
                theirs = true;
                // The loop's nearest point (a chord's middle), credited
                // at most the solid law's tessellation allowance toward
                // its nearest vertex: our own inscribed rim and a fine
                // model rim read the declared circle, a flat where it
                // stands (the same reading as `verify::thread_bore`).
                let (edge, vertex) = loop_nearest(l, h.xy);
                let near = edge + (vertex - edge).clamp(0.0, crate::verify::CHORD_CREDIT);
                if let Some(m) = minor.get_mut(k) {
                    *m = m.min(near);
                }
            }
        }
        if !theirs {
            loops.push(l.clone());
        }
    }
    for (h, r) in holes.iter().zip(minor) {
        loops.push(disk_loop(h.xy, r + depth)?);
    }
    Ok(Region::new(loops)?)
}

/// The plan distance from `c` to loop `l`'s nearest point - over its
/// edges, the closing one included, so a chord between two on-circle
/// vertices reads its sagitta - and to its nearest vertex. Non-finite
/// points read as zero, which shrinks the major circle (convicts) rather
/// than growing it.
fn loop_nearest(l: &Loop, c: Point2) -> (f64, f64) {
    let pts = l.points();
    let n = pts.len();
    let (mut edge, mut vertex) = (f64::INFINITY, f64::INFINITY);
    for i in 0..n {
        let (Some(a), Some(b)) = (pts.get(i), pts.get((i + 1) % n)) else { continue };
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let dd = dx * dx + dy * dy;
        let t = if dd > f64::EPSILON { (((c.x - a.x) * dx + (c.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
        let d = (a.x + t * dx - c.x).hypot(a.y + t * dy - c.y);
        let v = (a.x - c.x).hypot(a.y - c.y);
        edge = if d.is_finite() { edge.min(d) } else { 0.0 };
        vertex = if v.is_finite() { vertex.min(v) } else { 0.0 };
    }
    (edge, vertex)
}

/// The holes a thread mill threads, as the solid law's grant reads them
/// (#1071): each declared (minor) circle and the major circle its helix
/// is cut to. Empty for every other kind, and for a pitch that is not a
/// length (the Thread arm refuses that before any path).
pub(super) fn thread_bores(params: &OpParams, pitch: f64) -> Vec<crate::verify::ThreadBore> {
    let OpParams::Thread { holes, .. } = params else { return Vec::new() };
    if !(pitch.is_finite() && pitch > 0.0) {
        return Vec::new();
    }
    let depth = crate::machining2d::THREAD_DEPTH_PER_PITCH * pitch;
    holes
        .iter()
        .map(|h| crate::verify::ThreadBore { xy: h.xy, minor: h.radius, major: h.radius + depth, top: h.entry_z })
        .collect()
}
