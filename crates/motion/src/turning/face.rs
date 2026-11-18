//! Facing: the station ladder of radial passes (M5.2 A).
//!
//! The law this kernel stands on: the CL point is the insert nose
//! CIRCLE'S CENTER, held at `station + nose_radius` along the axis,
//! so the nose ball is TANGENT to the facing plane at every point of
//! the pass - it cuts exactly the flat it is aimed at and can never
//! cross it. No gouging is therefore a construction, not a check:
//! a pass at station s removes precisely the band [s, s+2r], which
//! is why an infeed over twice the nose radius is REFUSED (adjacent
//! stations would leave an untouched sheet between them) rather
//! than trusted to the verifier to catch afterwards.
//!
//! The passes all run INWARD, from the outer band to the axis: the
//! spun work presents its fresh face to the tool only on the cut
//! move. At the axis the nose first LIFTS off the flat it just made
//! ([`LIFT`] along the axis, #632) and only then rapids out, so the
//! retract never drags the nose back across the fresh face. Rapids
//! travel at `r_max + stepdown + nose`, the rotary rough's own
//! clearance law read radially, where convexity of the distance-to-
//! axis makes `r_max` an exact bound from the vertices alone. The
//! program opens with a rapid to that radius on the clearance plane
//! (#632) - a commanded, judged approach - before the first plunge.
//!
//! Provenance: lathe facing as radial passes at axial stations -
//! standard turning practice from machine-tool handbooks and the
//! RS-274 G1 vocabulary; the ball-tangent no-gouge construction is
//! our own, on the same nose-envelope law the verifier's envelope
//! scan measures with. No vendor strategy consulted.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geometry::Triangle;
use crate::machining2d::GenError;

use super::profile;

/// The facing operation as the kernel reads it: three world heights
/// (the rapids plane, the claimed top of the work, and the plane
/// the last pass must leave flat), the RADIAL ladder's infeed, the
/// two feeds, and the insert's nose radius - the ball everything
/// turns on. There is no stepover because none is missed: one pass
/// per station runs the profile's whole width, the stepdown IS the
/// spacing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FacingParams {
    pub clearance_z: f64,
    pub top_z: f64,
    pub facing_z: f64,
    pub stepdown: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    pub nose_radius: f64,
}

/// A rung cap with room to spare: a finer promise than one pass in
/// 20 nanometres of 32-bit float is nobody's facing - refuse the
/// hang before it starts.
const MAX_RUNGS: f64 = 100_000.0;

/// How far the nose lifts off a just-cut face, along the axis, before
/// it rapids out (#632): 0.2 mm, held to half the stepdown. The cap is
/// what keeps the lifted rapid in air the ladder already cleared - the
/// pass above swept the band `[s + stepdown, s + stepdown + 2 nose]`,
/// and a lift no taller than the stepdown leaves the lifted ball inside
/// what this pass and the one above removed (the reach sweep's rung
/// windows say the same for anything wider).
const LIFT: f64 = 0.2;

/// Face the work: fit the axis, ladder the stations, walk them top
/// down. Refusals are by name; a clean pass-set is a Toolpath in
/// world coordinates, ready for the envelope verdict.
///
/// `stock` is the setup's raw stock (#122): the axis is still fitted
/// to the part alone, but the ladder climbs to whichever end stands
/// higher and every rung's band and the rapids radius read the stock
/// too - so a facing allowance declared in the stock, above a part at
/// its finished length, is faced off rather than reported as nothing.
pub fn facing(
    facets: &[Triangle],
    stock: Option<&[Triangle]>,
    p: &FacingParams,
) -> Result<Toolpath, GenError> {
    if facets.is_empty() {
        return Err(GenError::Invalid("no facets to face".into()));
    }
    if ![
        p.clearance_z,
        p.top_z,
        p.facing_z,
        p.stepdown,
        p.feed,
        p.plunge_feed,
        p.nose_radius,
    ]
    .iter()
    .all(|v| v.is_finite())
    {
        return Err(GenError::Invalid(
            "facing was asked with a number that is not one".into(),
        ));
    }
    if p.nose_radius <= 0.0 {
        return Err(GenError::Invalid(format!(
            "facing is an arc, not a line - a nose radius of {:.3} is a mathematical point, \
             and a point cannot face",
            p.nose_radius
        )));
    }
    if p.stepdown <= 0.0 {
        return Err(GenError::Invalid("facing needs a positive stepdown".into()));
    }
    if p.stepdown > 2.0 * p.nose_radius {
        return Err(GenError::Invalid(format!(
            "a stepdown of {:.3} is past the nose's reach: a ball of radius {:.3} clears \
             a band {:.3} tall, and the next station up would leave a sheet nothing touches \
             - tighten the stepdown",
            p.stepdown,
            p.nose_radius,
            2.0 * p.nose_radius
        )));
    }
    let f = profile::Frame::about(facets)?;

    // The work's whole-body extents, folded once from the vertices -
    // sound HERE (the body is the hull of all of them; profile.rs's
    // note says where that promise ends).
    let prof = profile::profile_of(facets, &f)?;
    let (mut r_max, mut z_top) = (prof.r_max, prof.z_top);
    let stock = stock.filter(|s| !s.is_empty()).unwrap_or(&[]);
    if !stock.is_empty() {
        // Read in the PART's frame: the stock's extents about the
        // spindle the part was fitted to, never an axis of its own.
        let sp = profile::profile_of(stock, &f)?;
        r_max = r_max.max(sp.r_max);
        z_top = z_top.max(sp.z_top);
    }
    if r_max <= 0.0 {
        return Err(GenError::Invalid(
            "the work has no radius to face - every facet sits on the axis".into(),
        ));
    }

    // The station ladder, anchored on the facing plane and stopped
    // at the claimed top of the work.
    let s_face = f.station_of(p.facing_z);
    let hi = f.station_of(p.top_z).min(z_top);
    if hi <= s_face {
        return Err(GenError::Empty(
            "nothing to face - the facing plane stands at or above the top of both the part \
             and its stock"
                .into(),
        ));
    }
    let mut rungs = ((hi - s_face) / p.stepdown).floor() + 1.0;
    // #2004: a rung AT the top of the work (a span an exact multiple of
    // the stepdown) has its whole ball band [s, s + 2r] above the work -
    // a full facing pass through air. It is dropped; the rung under it
    // already reaches the top (`stepdown <= 2r`). A rung at a Top Z that
    // stands BELOW the work's top still has material over it, and stays.
    let tol = (hi - s_face) * 1e-9;
    if rungs > 1.0 && s_face + (rungs - 1.0) * p.stepdown >= z_top - tol {
        rungs -= 1.0;
    }
    if rungs > MAX_RUNGS {
        return Err(GenError::Invalid(format!(
            "the facing ladder would run {:.0} stations - the stepdown is far too fine \
             for the height between the plane and the top",
            rungs
        )));
    }
    let n = rungs as usize;
    // The topmost rung reaches its ball's ceiling at `station + 2r`;
    // the ladder's spacing law (`stepdown <= 2r`, refused above)
    // already guarantees that ceiling clears `hi`, so no separate
    // coverage test is needed here.

    // The infeed radius rung by rung, asked of the TRIANGLES: one
    // whose stations span [lo, top] occupies a pass's window
    // [s, s + 2 nose] exactly for s in [lo - 2 nose, top], and its
    // widest VERTEX bounds the whole triangle (convexity over the
    // hull of its own corners). Vertices filtered into the window
    // would miss a cylinder's side - both rim rings outside, the
    // wall itself at full radius inside - and the rung would then
    // cut only its little disk near the axis and leave an annulus
    // of stubble standing above the facing plane: the silent kind
    // of wrong, which no envelope verdict ever catches. The sweep
    // is events by rung start into a max-heap of (radius, life); a
    // rung's band is the top still alive at it. Overshoot is safe
    // (the bound is the triangle's widest vertex wherever in the
    // window it stands), so the whole-triangle bound is taken as
    // given.
    let mut bands = vec![0.0f64; n];
    {
        let mut events: Vec<(usize, Reach)> = Vec::with_capacity(facets.len() + stock.len());
        for t in facets.iter().chain(stock) {
            let (a, b, c) = (f.sr(t.a), f.sr(t.b), f.sr(t.c));
            let lo = a.0.min(b.0).min(c.0);
            let top = a.0.max(b.0).max(c.0);
            let rr = a.1.max(b.1).max(c.1);
            let k0 = ((lo - 2.0 * p.nose_radius - s_face) / p.stepdown).ceil();
            let k1 = ((top - s_face) / p.stepdown).floor();
            if k1 < 0.0 || k0 >= n as f64 || k0 > k1 {
                continue;
            }
            let k0 = k0.max(0.0) as usize;
            let k1 = k1.min((n - 1) as f64) as usize;
            events.push((k0, Reach { radius: rr, end: k1 }));
        }
        events.sort_unstable_by_key(|e| e.0);
        let mut heap: std::collections::BinaryHeap<Reach> =
            std::collections::BinaryHeap::with_capacity(events.len());
        let mut ei = 0usize;
        for (k, band) in bands.iter_mut().enumerate() {
            while ei < events.len() && events[ei].0 <= k {
                heap.push(events[ei].1);
                ei += 1;
            }
            while heap.peek().is_some_and(|e| e.end < k) {
                heap.pop();
            }
            *band = heap.peek().map_or(0.0, |e| e.radius);
        }
    }

    let rho_clear = r_max + p.stepdown + p.nose_radius;
    let center = |s: f64| s + p.nose_radius;
    let lift = LIFT.min(0.5 * p.stepdown);
    let c_clear = center(f.station_of(p.clearance_z));
    // The approach (#632): the path opens on the axis on the clearance
    // plane - the lathe's reading of the mill kernels' origin start -
    // so its first move is a RAPID out to the clearance point, a
    // commanded traverse the meridian rapid law (#103) judges against
    // the revolved stock, never a feed from wherever the turret parked.
    let mut b = PathBuilder::new(f.point(0.0, c_clear));
    b.push(MoveKind::Rapid, f.point(rho_clear, c_clear), None);
    for k in (0..n).rev() {
        let s = s_face + (k as f64) * p.stepdown;
        // The controlled axial stationing, outside the whole work.
        b.push(MoveKind::Feed, f.point(rho_clear, center(s)), Some(p.plunge_feed));
        // Where this rung's air begins, from the reach sweep above - fed
        // in, not rapided (#103): the band is the MODEL's reach, and a
        // stock wider than the model stands between it and rho_clear at
        // this rung, below the last pass's floor. A feed through air is
        // slow; a rapid through the bar's corner is a crash.
        let r_band = bands[k];
        b.push(MoveKind::Feed, f.point(r_band + p.nose_radius, center(s)), Some(p.feed));
        // The pass itself: inward, tangent to the station, to the axis.
        b.push(MoveKind::Feed, f.point(0.0, center(s)), Some(p.feed));
        // Off the face first (#632), then out at the lifted station:
        // the nose never slides back across the face it just made.
        b.push(MoveKind::Rapid, f.point(0.0, center(s) + lift), None);
        b.push(MoveKind::Rapid, f.point(rho_clear, center(s) + lift), None);
    }
    // Home to the rapids plane, outside the whole work.
    b.push(MoveKind::Rapid, f.point(rho_clear, c_clear), None);
    Ok(b.finish())
}

/// One triangle's claim on the ladder: its widest radius, alive
/// through rung `end`. The heap is a max-heap on the radius; f64
/// is not `Ord`, but `total_cmp` is a total order on finite ones
/// and a corrupt vertex died at `profile_of`'s gate before any of
/// this was built.
#[derive(Clone, Copy)]
struct Reach {
    radius: f64,
    end: usize,
}

impl PartialEq for Reach {
    fn eq(&self, o: &Self) -> bool {
        self.radius.total_cmp(&o.radius) == std::cmp::Ordering::Equal && self.end == o.end
    }
}

impl Eq for Reach {}

impl Ord for Reach {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.radius
            .total_cmp(&o.radius)
            .then_with(|| self.end.cmp(&o.end))
    }
}

impl PartialOrd for Reach {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
