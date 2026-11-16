//! Fitting a mod-vise pair to the stock (#820), in the plate's frame:
//!
//! 1. The fixed block stands on the named hole, turned a quarter turn
//!    (0, 90, 180 or 270 degrees) so its jaw faces along a grid row or
//!    column - direction `d`.
//! 2. The adjustable block, turned half a turn from it, goes on the grid
//!    line through the fixed block (shifted across so the two bodies
//!    line up), at the hole NEAREST the stock that still leaves it clear
//!    of the stock (a non-negative gap).
//! 3. It slides along its slots by that gap, so its face touches the
//!    stock's far side - refused by name when the gap is more than the
//!    slot travel.
//!
//! The stock must stand in front of the fixed jaw (any gap there is
//! reported, not closed: the fixed block does not move), across the
//! width of both jaws, and on the plate below the jaw tops. Every bolt
//! must land on a hole of the grid and every pin on a dowel bore.
//!
//! Provenance: our own placement rules over the #819 grid; rigid-frame
//! algebra from published quaternion identities.

use crate::geometry::{Aabb, Point3, transform_point};
use crate::plate::{Grid, PlateDef, location_name};
use crate::schema::Location;
use crate::setup_align::inverse;

use super::{JawParams, TOL};

/// How near a bolt must land to a grid node (the #819 snap tolerance).
const ON_NODE: f64 = 0.05;

/// A fitted pair.
#[derive(Clone, Debug, PartialEq)]
pub struct PairFit {
    /// The fixed block's hole (canonical name) and turn.
    pub fixed_at: String,
    pub yaw_deg: f64,
    /// The adjustable block's hole; its turn is `yaw_deg + 180`.
    pub adjustable_at: String,
    /// How far the adjustable block slides to touch the stock.
    pub slide: f64,
    /// Between the fixed jaw's face and the stock's near side (0 when the
    /// stock is against it).
    pub fixed_gap: f64,
    /// Between the two jaw faces once fitted (the stock's depth plus the
    /// fixed gap).
    pub opening: f64,
}

/// The unit clamping direction `d` and across direction `e` of a block
/// turned `k` quarter turns, exact (no trig dust).
fn axes(k: i64) -> ((f64, f64), (f64, f64)) {
    match k.rem_euclid(4) {
        0 => ((0.0, 1.0), (1.0, 0.0)),
        1 => ((-1.0, 0.0), (0.0, 1.0)),
        2 => ((0.0, -1.0), (-1.0, 0.0)),
        _ => ((1.0, 0.0), (0.0, -1.0)),
    }
}

/// The hole of `g` at plate point (`x`, `y`), or why there is none.
fn hole_at(g: &Grid, x: f64, y: f64) -> Result<String, String> {
    let c = ((x - g.x0) / g.pitch_x).round();
    let r = ((y - g.y0) / g.pitch_y).round();
    if !(c.is_finite() && r.is_finite()) || c < 0.0 || r < 0.0 || c >= f64::from(g.cols) || r >= f64::from(g.rows) {
        return Err(format!("it is off the grid ({})", g.extent()));
    }
    let (nx, ny) = g.node(r as u32, c as u32);
    let name = location_name(r as u32, c as u32);
    let off = (x - nx).hypot(y - ny);
    if off > ON_NODE {
        return Err(format!("it lands {off:.3} mm from the nearest hole {name}, between holes"));
    }
    g.locate(&name)?;
    Ok(name)
}

/// Every bolt of `b` (anchor at `o`, turned to `d`/`e`) on a hole of the
/// grid, and every pin on a dowel bore.
fn footprint(plate: &PlateDef, b: &JawParams, o: (f64, f64), d: (f64, f64), e: (f64, f64)) -> Result<(), String> {
    let at = |p: &[f64; 2]| (o.0 + p[0] * e.0 + p[1] * d.0, o.1 + p[0] * e.1 + p[1] * d.1);
    for p in &b.bolts {
        let (x, y) = at(p);
        hole_at(&plate.holes, x, y).map_err(|e| format!("the {} bolt at [{:.3}, {:.3}] mm: {e}", b.what(), p[0], p[1]))?;
    }
    if !b.pins.is_empty() {
        let dowels = plate.dowels.as_ref().ok_or_else(|| format!("the {} has locating pins, but the plate has no dowel bores", b.what()))?;
        for p in &b.pins {
            let (x, y) = at(p);
            hole_at(dowels, x, y).map_err(|e| format!("the {} pin at [{:.3}, {:.3}] mm: {e}", b.what(), p[0], p[1]))?;
        }
    }
    Ok(())
}

/// The quarter turns `yaw_deg` is, or `None` when it is not a whole
/// (finite, sane) number of them.
fn quarter(yaw_deg: f64) -> Option<i64> {
    let q = yaw_deg / 90.0;
    (q.is_finite() && (q - q.round()).abs() <= 1e-9 && q.abs() <= 1e6).then(|| q.round() as i64)
}

/// Every bolt of block `b`, anchored on hole `at` of `plate` turned
/// `yaw_deg`, on a hole of the grid and every pin on a dowel bore - the
/// footprint law a figure edit of a placed block (`SetModJaw`, #832)
/// re-runs where the block stands. Refused by name.
pub fn footprint_at(plate: &PlateDef, b: &JawParams, at: &str, yaw_deg: f64) -> Result<(), String> {
    let k = quarter(yaw_deg).ok_or_else(|| format!("the {} stands off a quarter turn ({yaw_deg} degrees)", b.what()))?;
    let (d, e) = axes(k);
    let o = plate.holes.locate(at).map_err(|e| format!("the {}: {e}", b.what()))?;
    footprint(plate, b, o, d, e)
}

/// The stock box's corners in the plate's frame.
fn corners(plate_pose: Location, stock: &Aabb) -> [Point3; 8] {
    let inv = inverse(plate_pose);
    std::array::from_fn(|k| {
        let c = Point3::new(
            if k & 1 == 0 { stock.min.x } else { stock.max.x },
            if k & 2 == 0 { stock.min.y } else { stock.max.y },
            if k & 4 == 0 { stock.min.z } else { stock.max.z },
        );
        transform_point(c, inv.pos, inv.rot)
    })
}

/// Fit pair (`fixed`, `adjustable`) on `plate` (standing at
/// `plate_pose`) with the fixed block at hole `at` turned `yaw_deg`, to
/// stock box `stock` (world). Refused by name: a turn off a quarter, an
/// unknown hole, stock in or behind the fixed jaw, beside the jaws, under
/// the plate or over the jaws, no hole past the stock, the stock out of
/// the slots' reach, a bolt or pin off the grid.
pub fn fit_pair(
    plate: &PlateDef,
    plate_pose: Location,
    fixed: &JawParams,
    adjustable: &JawParams,
    at: &str,
    yaw_deg: f64,
    stock: &Aabb,
) -> Result<PairFit, String> {
    fixed.check()?;
    adjustable.check()?;
    if fixed.side != super::JawSide::Fixed || adjustable.side != super::JawSide::Adjustable {
        return Err("a mod vise pair is a fixed and an adjustable jaw block".into());
    }
    let k = quarter(yaw_deg)
        .ok_or_else(|| format!("a mod vise pair runs along a grid row or column: turn it 0, 90, 180 or 270 degrees (got {yaw_deg})"))?;
    let (d, e) = axes(k);
    let (along_pitch, across_pitch) = if k.rem_euclid(2) == 0 { (plate.holes.pitch_y, plate.holes.pitch_x) } else { (plate.holes.pitch_x, plate.holes.pitch_y) };
    let o = plate.holes.locate(at).map_err(|e| format!("the fixed jaw block: {e}"))?;
    let fixed_at = hole_at(&plate.holes, o.0, o.1)?;
    footprint(plate, fixed, o, d, e)?;

    // The stock in (t along d, u across e, z) from the fixed anchor.
    let pts = corners(plate_pose, stock);
    let rel = |p: &Point3| (p.x - o.0, p.y - o.1);
    let span = |f: &dyn Fn(&Point3) -> f64| pts.iter().map(f).fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
    let (t_near, t_far) = span(&|p| {
        let (x, y) = rel(p);
        x * d.0 + y * d.1
    });
    let (u_lo, u_hi) = span(&|p| {
        let (x, y) = rel(p);
        x * e.0 + y * e.1
    });
    let (z_lo, _) = span(&|p| p.z);
    if ![t_near, t_far, u_lo, u_hi, z_lo].iter().all(|v| v.is_finite()) || t_far - t_near <= 0.0 {
        return Err("the stock has no depth to clamp".into());
    }
    let t_fixed = fixed.face_y();
    if t_near < t_fixed - TOL {
        return Err(format!(
            "the stock reaches {:.3} mm into the fixed jaw block at {fixed_at} - place it clear of the stock (its face is {:.3} mm in front of its hole)",
            t_fixed - t_near,
            t_fixed
        ));
    }
    let top = plate.top_z;
    if z_lo < top - TOL {
        return Err(format!("the stock reaches {:.3} mm below the plate top - it must sit on the plate", top - z_lo));
    }
    let grip = fixed.jaw_height.min(adjustable.jaw_height);
    if z_lo >= top + grip - TOL {
        return Err(format!("the stock sits {:.3} mm above the plate, over the jaws ({grip:.3} mm tall)", z_lo - top));
    }
    let between = |centre: f64, width: f64, what: &str| {
        let (a, b) = (centre - 0.5 * width, centre + 0.5 * width);
        if u_hi <= a + TOL || u_lo >= b - TOL {
            Err(format!("the stock is beside the {what}, not between the jaws"))
        } else {
            Ok(())
        }
    };
    between(fixed.centre_x(), fixed.jaw_width, &fixed.what())?;

    // Across: line the adjustable body up with the fixed one, to the
    // nearest grid line (it stands turned half a turn: its across axis is -e).
    let shift = ((fixed.centre_x() + adjustable.centre_x()) / across_pitch).round();
    if !shift.is_finite() || shift.abs() > 1e6 {
        return Err("the jaw blocks' bolt patterns do not fit this grid's pitch".into());
    }
    let lat = shift * across_pitch;
    between(lat - adjustable.centre_x(), adjustable.jaw_width, &adjustable.what())?;

    // Along: the nearest hole that clears the stock.
    let limit = u64::from(plate.holes.rows.max(plate.holes.cols)).saturating_add(1);
    let mut pick = None;
    for n in 1..=limit {
        let t_a = n as f64 * along_pitch;
        let gap = t_a - adjustable.face_offset - t_far;
        if gap >= -TOL {
            pick = Some((t_a, gap.max(0.0)));
            break;
        }
    }
    let Some((t_a, gap)) = pick else {
        return Err(format!("the grid ends before a hole past the stock along the line of {fixed_at} - no place for the adjustable jaw block"));
    };
    let oa = (o.0 + t_a * d.0 + lat * e.0, o.1 + t_a * d.1 + lat * e.1);
    let adjustable_at = hole_at(&plate.holes, oa.0, oa.1).map_err(|e| format!("the adjustable jaw block's place {t_a:.3} mm from {fixed_at}: {e}"))?;
    if gap > adjustable.slot_travel + TOL {
        return Err(format!(
            "the stock is out of reach: from {adjustable_at}, the nearest hole clear of it, the adjustable jaw block must slide {gap:.3} mm; its slots travel {:.3} mm",
            adjustable.slot_travel
        ));
    }
    let slide = gap.min(adjustable.slot_travel);
    footprint(plate, adjustable, oa, (-d.0, -d.1), (-e.0, -e.1))?;
    let fixed_gap = (t_near - t_fixed).max(0.0);
    Ok(PairFit {
        fixed_at,
        yaw_deg: 90.0 * k as f64,
        adjustable_at,
        slide,
        fixed_gap,
        opening: t_a - adjustable.face_offset - slide - t_fixed,
    })
}
