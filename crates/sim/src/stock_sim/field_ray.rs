//! The ray-entry companion of the cut layer (M7.1 C): where the
//! vertical ray through a claimed cell first MEETS the swept solid.
//! Split from `field_cut` by the 400-line law; the storage rides
//! `field`'s struct, the sweeps live in `field_cut`, the consumer of
//! the numbers is `march`'s verdict walk.
//!
//! Provenance: a sweep of a convex solid along a segment is convex
//! (Minkowski sums - the same law `field_cut`'s distance rests on),
//! so its intersection with any line is an interval and the ray's
//! first crossing is a 1-D convex root, found here by bisection on
//! the same conservative distance. Nothing from vendor binaries.
//!
//! ## The law here
//!
//! - **The cell's cut magnitude measures the WRONG direction for a
//!   flank.** A pocket rim column says "tool wall 0.12 away" while
//!   nothing was removed above the center at all; a cone flank column
//!   says "0.06 away" while its surface stands 0.08 up the ray. One
//!   number cannot serve both verdicts - so the field keeps a second
//!   one: the entry distance up the vertical ray, NaN where the ray
//!   never meets the solid. The march answers a column by its entry
//!   when one exists (the cut's own surface, conservative to a lower
//!   bound) and by the cell boundary otherwise (the mesh's surface,
//!   exact for every lattice-straddled plane).
//! - **Conservative means the removed side.** `phi_sweep >= true
//!   distance` (`field_cut`), so a probe that says INSIDE is a proof,
//!   and a bisection that reports the last proven-inside point can
//!   only sit at or below the true entry: the estimated surface
//!   under-runs, the verdict over-reports the tool's reach, and no
//!   gouge is ever hidden by this layer.
//! - **Merged by a min.** The surface is the lowest entry any move
//!   proved (removals union upward along a ray), just as the cut
//!   magnitude is a running max - both commutative, replay stays
//!   order-independent.

use super::field::SdfField;
use super::field_cut::Sweep;
use super::field_solid::RevKind;

use crate::geometry::Point3;

/// Bisection rounds on the convex root: the interval ends at
/// cell * 1e-12, far under any float the verdict compares with.
const ROOT_ITERS: u32 = 40;

impl SdfField {
    /// The entry layer's value at a cell: the distance from the
    /// cell's CENTER UP to the first point the vertical ray is
    /// proven inside the swept solid; NaN where no move's solid ever
    /// meets the ray above any center's cell.
    pub(crate) fn up_at(&self, key: [i32; 3]) -> f64 {
        match self.cut_ups.get(&super::field::brick_of(key)) {
            Some(b) => b[super::field::in_brick(key)],
            None => f64::NAN,
        }
    }

    /// `up_at`'s mirror (#637 E2b): the distance from the cell's CENTER
    /// DOWN to the first point the vertical ray is proven inside a
    /// swept solid below - a flipped bake's `cut_ups` records moved
    /// whole (`field_turn`), read by `march_up`. NaN where none.
    pub(crate) fn down_at(&self, key: [i32; 3]) -> f64 {
        match self.cut_downs.get(&super::field::brick_of(key)) {
            Some(b) => b[super::field::in_brick(key)],
            None => f64::NAN,
        }
    }

    /// Record one proven entry, keeping the LOWEST (the union law).
    pub(crate) fn up_record(&mut self, key: [i32; 3], u: f64) {
        // Read first: only a change takes the writable brick, which
        // copies it when a keyframe shares it (#2194).
        let cur = self.up_at(key);
        if cur.is_nan() || u < cur {
            super::field::layer_brick_mut(&mut self.cut_ups, super::field::brick_of(key))[super::field::in_brick(key)] = u;
        }
    }
}

/// The vertical ray's first proven-inside point above `q`, measured
/// as a distance up from `q.z`, within the cell that holds `q`
/// (anything higher is the walk's business, not this cell's law).
/// NaN when the ray stays clear of the solid for the whole cell.
pub(crate) fn vertical_entry(ax: [f64; 3], sw: &Sweep, q: Point3, cell: f64) -> f64 {
    let g = |t: f64| sw.dist(ax, Point3::new(q.x, q.y, q.z + t));
    if g(0.0) < 0.0 {
        return 0.0; // the solid already covers the center
    }
    if let Some(u) = flat_floor_entry(ax, sw, q, cell) {
        return u;
    }
    // Convex minimum of g on [0, cell]: golden section, same floor
    // as `field_cut`'s sweep search.
    const PHI: f64 = 0.6180339887498949;
    let (mut lo, mut hi) = (0.0f64, cell);
    let mut c = hi - PHI * (hi - lo);
    let mut d = lo + PHI * (hi - lo);
    let (mut fc, mut fd) = (g(c), g(d));
    for _ in 0..32 {
        if fc < fd {
            hi = d;
            d = c;
            fd = fc;
            c = hi - PHI * (hi - lo);
            fc = g(c);
        } else {
            lo = c;
            c = d;
            fc = fd;
            d = lo + PHI * (hi - lo);
            fd = g(d);
        }
    }
    let t_min = if fc <= fd { c } else { d };
    if g(t_min) >= 0.0 {
        return f64::NAN; // the ray never meets the solid inside this cell
    }
    // Bisect the decreasing leg [0, t_min], keeping the INSIDE side
    // (the proven one) as the answer: a lower bound of the true root.
    let (mut inside, mut outside) = (t_min, 0.0f64);
    for _ in 0..ROOT_ITERS {
        let mid = (inside + outside) * 0.5;
        if g(mid) < 0.0 {
            inside = mid;
        } else {
            outside = mid;
        }
    }
    inside
}

/// The upright flat tool's floor, in closed form (M7.3): a level move
/// or a plunge of a vertical-axis cylinder sweeps a solid whose bottom
/// is the PLANE of its lowest tip, so a vertical ray strictly inside
/// the swept footprint enters at exactly that plane - the column
/// engine's own `min(top, tip z)`, with no search residue. The search
/// above answers the same entry only to its bisection floor (and the
/// cut's conservative 1e-12 hair), which left every machined floor
/// reading a hair ABOVE its own plane: harmless to a verdict within
/// `FIELD_VERDICT_EPS`, fatal to the strict `above(z)` mask the rest
/// ladder asks at exactly that plane. None for every other solid, a
/// ray outside the footprint, or a floor outside this cell's ray.
fn flat_floor_entry(ax: [f64; 3], sw: &Sweep, q: Point3, cell: f64) -> Option<f64> {
    // The bull nose (M13.1) has the same plane floor inside its
    // flat disc, `r - corner`; its arc ring is the search's business.
    if ax != [0.0, 0.0, 1.0] {
        return None;
    }
    let (rho, r, tip) = match sw {
        // A level arc piece (#413): the same plane, its nearest anchor
        // in closed form.
        Sweep::Arc(p) => p.level_floor(q)?,
        Sweep::Revolve { a, b, kind: RevKind::Flat { r, .. } } => (seg_rho(a, b, q)?, *r, a.z.min(b.z)),
        Sweep::Revolve { a, b, kind: RevKind::Bull { r, corner, .. } } => (seg_rho(a, b, q)?, r - corner, a.z.min(b.z)),
        _ => return None,
    };
    floor_entry(rho, r, tip, q, cell)
}

/// The XY distance from `q` to a level segment or a plunge's disc;
/// `None` for an oblique move (the search's business).
fn seg_rho(a: &Point3, b: &Point3, q: Point3) -> Option<f64> {
    let rho = if a.z == b.z {
        // XY distance to the level segment.
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let l2 = dx * dx + dy * dy;
        let t = if l2 > 0.0 { (((q.x - a.x) * dx + (q.y - a.y) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
        (q.x - a.x - t * dx).hypot(q.y - a.y - t * dy)
    } else if a.x == b.x && a.y == b.y {
        (q.x - a.x).hypot(q.y - a.y) // a plunge: one disc, every depth
    } else {
        return None; // an oblique move: the search's business
    };
    Some(rho)
}

/// The closed-form entry onto a floor plane `tip` for a ray at `rho`
/// from the floor disc's axis (radius `r`).
fn floor_entry(rho: f64, r: f64, tip: f64, q: Point3, cell: f64) -> Option<f64> {
    // Inside the footprint - the flank inclusive, `field_solid::
    // flat_tie`'s law - floor strictly above the center and inside
    // this cell's ray (the search's own domain).
    if !(rho.is_finite() && rho <= r && tip > q.z && tip - q.z <= cell) {
        return None;
    }
    // The read is `center + u` (`march::walk`): pick the `u` whose
    // sum lands ON the plane, never below it (an ulp low would claim
    // steel the tool never removed).
    let mut u = tip - q.z;
    for _ in 0..4 {
        let s = q.z + u;
        if s == tip {
            break;
        }
        u = if s < tip { u.next_up() } else { u.next_down() };
    }
    for _ in 0..4 {
        if q.z + u >= tip {
            break;
        }
        u = u.next_up();
    }
    (u.is_finite() && u >= 0.0 && q.z + u >= tip).then_some(u)
}
