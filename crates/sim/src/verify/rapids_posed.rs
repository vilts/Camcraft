//! Law (d)'s solid-aware second reading for POSED rapids: a lateral
//! rapid under a leaning tool that the column law convicts is re-read
//! against the part's solid and the open stock above it, not the
//! height field alone. The column law reads a rapid as a vertical
//! tool at its lowest Z and asks whether its three XY columns carry
//! material above that height - exact for an upright tool over open
//! stock, and blind by construction under an overhang: the air below
//! a ledge, below a tube's equator, reads as the stock standing above
//! it. A swarf link retracts along the contact normal - DOWN and
//! outward under the equator - so its every column reads a surface
//! top above it, and the flank it carries never enters the blade.
//!
//! The second reading samples the rapid as the controller runs it:
//! the tip along its line, the pose lerped between the held and the
//! stated one ([`super::midpose`]), and at every sample the stack
//! along the pose ([`super::probe::Cutter`]). It convicts when
//!
//! - the swept stack enters the PART solid - the solid law's own exact
//!   reader ([`super::solid_index`]), the part being stock that no
//!   cut ever removes; or
//! - a sample of the axis stands in OPEN stock: inside the stock
//!   footprint, under the stock top, with no part material at or
//!   above it (`slice::material_overhead`, read through
//!   [`super::rest::Rest::part_over`]) and below the column's
//!   cut-order surface ([`super::rest::Rest::surface_before`]) -
//!   there the column law is exact and keeps its word.
//!
//! What it stops convicting is the air under an overhang that no cut
//! reached: the column cannot tell that air from stock and the mesh
//! says it is outside the part. The upright reading (and every
//! unposed program, a stated upright included) is untouched; a
//! context without a model has no overhang to misread and never
//! asks. The overhead is read on a plane rounded UP to [`STEP`] - less
//! of the part stands above a higher plane, so rounding only adds
//! convictions. A mesh the solid reader cannot index, or a plane it
//! cannot slice, fails closed: the column verdict stands.
//!
//! Provenance: the sampling bound is [`super::posed_steel`]'s; the
//! modal pose rule RS-274's; swept-stack-against-solid is textbook
//! (Ericson, "Real-Time Collision Detection", public material). Our
//! own design; no vendor material consulted.

use super::midpose::{self, Pose};
use super::probe::{Cutter, Frame};
use super::rest::Rest;
use super::solid_index::SolidIndex;
use super::types::VerifyContext;

use super::upright::is_upright;
use crate::cl::{normalize_axis, Move};
use crate::geo2d::Point2;
use crate::geometry::Point3;

/// Sample spacing along the tip line and up the stack, mm.
const STEP: f64 = 0.5;
/// Samples per line cap (posed_steel's).
const MAX_STEPS: usize = 4096;

/// The lazily built part index of one verify pass: `None` until asked,
/// `Some(None)` when the mesh cannot be read (fail closed).
pub(super) struct PartIndex<'a> {
    index: Option<Option<SolidIndex<'a>>>,
}

impl<'a> PartIndex<'a> {
    pub(super) fn new() -> Self {
        Self { index: None }
    }
}

/// Whether the move's held or stated pose leans off the upright: only
/// such a rapid earns the second reading.
pub(super) fn leaning(pose: (Pose, Pose)) -> bool {
    let lean = |a: Pose| a.and_then(normalize_axis).is_some_and(|a| !is_upright(&a));
    lean(pose.0) || lean(pose.1)
}

/// The second reading of posed rapid `i`: `true` when the swept stack
/// stands in rest material by the solid-aware reading above, so the
/// column conviction holds; `false` when the mesh and the open-stock
/// columns both clear it.
#[allow(clippy::too_many_arguments)] // one law's flat read
pub(super) fn in_rest<'a>(
    m: &Move,
    i: usize,
    pose: (Pose, Pose),
    cx: &VerifyContext<'a>,
    rest: &Rest<'_, 'a>,
    cutter: &Cutter,
    part: &mut PartIndex<'a>,
) -> bool {
    let Some(facets) = cx.model else {
        return true; // no overhang to misread: the column law is whole
    };
    let atoms = cutter.atoms();
    if atoms.is_empty() || !(m.from.is_finite() && m.to.is_finite()) {
        return true;
    }
    let reach = atoms.iter().map(|a| a.z1).fold(0.0, f64::max);
    let up = [0.0, 0.0, 1.0];
    let a0 = pose.0.and_then(normalize_axis).unwrap_or(up);
    let a1 = pose.1.and_then(normalize_axis).unwrap_or(up);
    // The pieces the stack is swept along, each under one pose: the
    // whole move under its end pose, and - when the pose changes -
    // the controller's intervals between the two.
    let mut pieces: Vec<(Point3, Point3, [f64; 3])> = vec![(m.from, m.to, a1)];
    if let Some((fa, fb)) = midpose::change(pose.0, pose.1) {
        pieces.extend(midpose::intervals(m, fa, fb, reach, cx.gouge_tol).into_iter().map(|iv| (iv.p0, iv.p1, iv.am)));
    }
    // The part: the rapid starts where the cut ended, so it is granted
    // the cut's own budget (the finish tolerance where one is judged)
    // on top of the gouge tolerance.
    // #904: less a coarse model's excess, while that is honest.
    let allow = cx.tightened(cx.gouge_tol.max(0.0) + cx.finish_tol.unwrap_or(0.0).max(0.0));
    let Some(index) = part.index.get_or_insert_with(|| SolidIndex::build(facets)).as_mut() else {
        return true; // unreadable mesh: fail closed
    };
    for &(p0, p1, a) in &pieces {
        let fr = Frame::new(p0, Some(a).filter(|a| !is_upright(a)));
        if index.hits(&fr, fr.local(p1), &atoms, allow, 0.0, &[], &[]) {
            return true;
        }
    }
    // The open stock: every sample of the axis, tip to reach, along
    // the tip line under the lerped pose.
    let len = dist(m.from, m.to);
    let n = if len.is_finite() { ((len / STEP).ceil() as usize).clamp(1, MAX_STEPS) } else { 1 };
    let k = ((reach / STEP).ceil() as usize).clamp(1, MAX_STEPS);
    for j in 0..=n {
        let t = j as f64 / n as f64;
        let tip = lerp(m.from, m.to, t);
        let a = lerp_pose(a0, a1, t);
        for s in 0..=k {
            let h = reach * s as f64 / k as f64;
            let p = Point3::new(tip.x + a[0] * h, tip.y + a[1] * h, tip.z + a[2] * h);
            if open_stock(cx, rest, p, i) {
                return true;
            }
        }
    }
    false
}

/// Whether `p` stands in open stock before move `upto`: in the
/// footprint, under the stock top, no part material at or above it
/// (the part is the probe's, the air under it the overhang's), and
/// below the column's cut-order surface. A failed slice fails closed.
fn open_stock(cx: &VerifyContext, rest: &Rest, p: Point3, upto: usize) -> bool {
    let q = Point2::new(p.x, p.y);
    if !rest.in_stock(q) || p.z >= cx.stock_top_z - cx.z_tol {
        return false;
    }
    // The plane rounded UP to the ladder: less of the part stands
    // above a higher plane, so the rounding only adds convictions.
    let plane = (p.z / STEP).ceil() * STEP;
    match rest.part_over(q, plane) {
        None => true,
        Some(true) => false,
        Some(false) => p.z < rest.surface_before(q, upto) - cx.z_tol,
    }
}

/// The lerped unit pose, upright when the lerp vanishes.
fn lerp_pose(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    let v = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
    normalize_axis(v).unwrap_or([0.0, 0.0, 1.0])
}

fn lerp(a: Point3, b: Point3, s: f64) -> Point3 {
    Point3::new(a.x + (b.x - a.x) * s, a.y + (b.y - a.y) * s, a.z + (b.z - a.z) * s)
}

fn dist(a: Point3, b: Point3) -> f64 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2) + (b.z - a.z).powi(2)).sqrt()
}
