//! Grooving and parting (M5.2 C): the kinds where a full-width
//! blade, not a nose circle, owns the cut.
//!
//! A blade's footprint is a BAND: it removes everything outside its
//! tip radius, across the `width` of axis its kerf stands in. The
//! floor it can reach at center `c` is therefore the boxcar max of
//! the silhouette over its OPEN band - [`revolve::span_max`] - the
//! same no-gouge law the envelope scan reads, with the band's
//! edges tangent by [`revolve`]'s promise: a corner standing exactly
//! on a wall grazes it, which is how a full-width groove sits flush
//! to its walls and why centers step by exactly one width.
//!
//! The ROUGH's ladder steps each center's tip down from the window
//! shell so no single plunge eats the whole depth, and stops at
//! `B + leave` - the finish (or the next operation) owns the last
//! skin. PARTING adds the through-axis rung: a tip of 0.0 stands ON
//! the axis, the kerf's whole column is above it, and the verdict
//! reads that as the separation it is - silent inside the kerf, and
//! blind to everything right of it, where the workpiece has left
//! with the scrap.
//!
//! Rapids travel at the whole body's widest radius plus tolerance:
//! the window may be a groove in mid-bar, and a clearance plane
//! that clears only the groove would drag the blade through every
//! standing shoulder it traverses over.
//!
//! Provenance: grooving, single-groove finishing and cutoff as
//! banded revolved cuts - machine-tool handbook practice; the
//! boxcar floor and the tangency-stepped center sweep are our own,
//! on the same silhouette the profile kinds ride. No vendor
//! strategy consulted.

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geometry::Triangle;
use crate::machining2d::GenError;

use super::profile;
use super::revolve;
use super::stock::StockEnv;

pub use super::parting::{part, part_in};

/// Grooving roughing: a band window on the bar, eaten to a floor
/// with a leave of stock for the finish, rung by rung.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrooveRoughParams {
    pub tolerance: f64,
    pub clearance_z: f64,
    pub top_z: f64,
    pub bottom_z: f64,
    pub stepdown: f64,
    pub stock_to_leave: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    pub blade_width: f64,
}

/// Grooving finishing: one full-depth plunge per center down to the
/// band's floor, leave-free.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrooveFinishParams {
    pub tolerance: f64,
    pub clearance_z: f64,
    pub top_z: f64,
    pub bottom_z: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    pub blade_width: f64,
}

/// Parting: one center at the parting plane, a stepped ladder and a
/// through-axis rung that severs the bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartParams {
    pub tolerance: f64,
    pub clearance_z: f64,
    pub top_z: f64,
    pub parting_z: f64,
    pub stepdown: f64,
    pub feed: f64,
    pub plunge_feed: f64,
    pub blade_width: f64,
}

/// A plunge cap with room to spare - facing's number, the same
/// promise.
pub(crate) const MAX_RUNGS: f64 = 100_000.0;

/// What a blade kernel walks on: the frame, the silhouette samples
/// and their chord windows, the window's station pair and the bar's
/// domain ends.
type Blade = (
    profile::Frame,
    Vec<(f64, f64)>,
    revolve::Segs,
    (f64, f64),
    (f64, f64),
    f64,
);

/// The shared door: the numbers are numbers, the blade has width,
/// the model revolves, and the height window holds bar. Blades
/// carry no nose promise, so this stands on its own beside
/// [`super::prof`]'s setup. `tool_word` names the steel the refusal
/// asks for - the threading kernel reads bar through the same door
/// a blade does (M5.2 D), and its insert is not a blade.
#[allow(clippy::too_many_arguments)] // the whole door is flat reads, like prof::setup
pub(crate) fn setup(
    facets: &[Triangle],
    clearance_z: f64,
    top_z: f64,
    bottom_z: f64,
    width: f64,
    tol: f64,
    what: &str,
    tool_word: &str,
    stock: Option<StockEnv>,
) -> Result<Blade, GenError> {
    if facets.is_empty() {
        return Err(GenError::Invalid(format!("no facets to {what}")));
    }
    if ![clearance_z, top_z, bottom_z, width, tol]
        .iter()
        .all(|v| v.is_finite())
    {
        return Err(GenError::Invalid(format!(
            "{what} was asked with a number that is not one"
        )));
    }
    if width <= 0.0 {
        return Err(GenError::Invalid(format!(
            "a blade of width {width:.3} cuts no band - the kerf's width IS the tool; \
             mount a {tool_word} with a real width"
        )));
    }
    if tol <= 0.0 {
        return Err(GenError::Invalid(format!(
            "the tolerance must be a positive size, not {tol}"
        )));
    }
    let f = profile::Frame::about(facets)?;
    let pts = profile::silhouette(facets, &f, tol * 0.25)?;
    let prof = profile::profile_of(facets, &f)?;
    let (d_lo, d_hi) = (pts[0].0, pts[pts.len() - 1].0);
    let s_lo = f.station_of(bottom_z).max(d_lo);
    let s_hi = f.station_of(top_z).min(d_hi);
    if s_hi <= s_lo {
        return Err(GenError::Empty(format!(
            "nothing to {what} - the height window holds no length of the bar"
        )));
    }
    // A blade's move carries its tip, its body outward of it: the tip
    // clear of the bar's OD clears the blade (#130).
    let rho_clear = StockEnv::rapid_radius(stock, prof.r_max, prof.r_max + tol, 0.0);
    let sg = revolve::segs(&pts);
    Ok((f, pts, sg, (s_lo, s_hi), (d_lo, d_hi), rho_clear))
}

/// The centers that tile the window: spaced one blade width apart,
/// the first and last standing so the open band reaches the window
/// ends - spilling half a width into the bar's own end air when an
/// end coincides with one. The last center lands on `hi_c` even
/// when the step does not divide the run: tangency tiles, the tail
/// closes.
fn centers(
    (s_lo, s_hi): (f64, f64),
    (d_lo, d_hi): (f64, f64),
    half: f64,
) -> Result<Vec<f64>, GenError> {
    let lo_c = if s_lo <= d_lo { s_lo } else { s_lo + half };
    let hi_c = if s_hi >= d_hi { s_hi } else { s_hi - half };
    if hi_c < lo_c {
        return Err(GenError::Invalid(format!(
            "the window is {:.3} long and the blade {:.3} wide - narrower than the \
             blade: a groove cannot fit where it was asked",
            s_hi - s_lo,
            2.0 * half
        )));
    }
    if (hi_c - lo_c) / (2.0 * half) > MAX_RUNGS {
        return Err(GenError::Invalid(format!(
            "the center sweep would run more than {:.0} passes - the blade is far too \
             narrow for the window, or the window is far too long for one operation",
            MAX_RUNGS
        )));
    }
    let mut v: Vec<f64> = (0..)
        .map_while(|i| {
            let c = lo_c + (i as f64) * 2.0 * half;
            (c < hi_c).then_some(c)
        })
        .collect();
    v.push(hi_c);
    Ok(v)
}

/// One center's plunge: rapid in over it, feed down to `tips`
/// rung by rung, rapid clear again.
pub(crate) fn plunge(
    b: &mut PathBuilder,
    f: &profile::Frame,
    s: f64,
    rho_clear: f64,
    tips: &[f64],
    plunge_feed: f64,
) {
    b.push(MoveKind::Rapid, f.point(rho_clear, s), None);
    for &t in tips {
        b.push(MoveKind::Feed, f.point(t, s), Some(plunge_feed));
        b.push(MoveKind::Rapid, f.point(rho_clear, s), None);
    }
}

/// A rung floored: a rung the projection noise left within a
/// [`revolve::HAIR`] of its floor IS the floor rung (and one below
/// it, of course - that is a `max` the clamp still does). Without
/// the snap the ladder's `ceil` counts a rung the noise pushed a
/// hair off the floor, and the feed count flips on the same
/// coincidence the meridian law refuses to judge by (M5.2 C).
pub(crate) fn rung(v: f64, floor: f64) -> f64 {
    if v <= floor + revolve::HAIR {
        floor
    } else {
        v
    }
}

/// The roughing rungs at one center: from the window shell inward,
/// stopped by `B + leave` - the floor the blade may reach without
/// eating what the model keeps, plus the skin it was told to leave.
pub(crate) fn rungs(
    b: f64,
    ref_top: f64,
    stepdown: f64,
    leave: f64,
) -> Result<Vec<f64>, GenError> {
    let floor = b + leave;
    if ref_top > floor + stepdown * MAX_RUNGS {
        return Err(GenError::Invalid(format!(
            "the ladder would run more than {:.0} rungs down to {:.3} - the stepdown is \
             far too fine for the depth to eat",
            MAX_RUNGS, floor
        )));
    }
    let n = (((ref_top - floor) / stepdown).ceil().max(0.0)) as usize;
    let mut v: Vec<f64> = (1..=n).map(|j| rung(ref_top - (j as f64) * stepdown, floor)).collect();
    if v.last().is_none_or(|&t| t > floor) {
        v.push(floor);
    }
    v.dedup();
    Ok(v)
}

/// Groove roughing: tile the window, ladder each center from the
/// window shell down to the leave line.
pub fn groove_rough(
    facets: &[Triangle],
    p: &GrooveRoughParams,
) -> Result<Toolpath, GenError> {
    groove_rough_in(facets, p, None)
}

/// [`groove_rough`] inside a setup stock (#130): each center's ladder
/// steps down from the bar's OD, every rapid clears it.
pub fn groove_rough_in(
    facets: &[Triangle],
    p: &GrooveRoughParams,
    stock: Option<StockEnv>,
) -> Result<Toolpath, GenError> {
    let (f, _, sg, win, dom, rho_clear) = setup(
        facets,
        p.clearance_z,
        p.top_z,
        p.bottom_z,
        p.blade_width,
        p.tolerance,
        "groove",
        "grooving blade",
        stock,
    )?;
    if p.stepdown <= 0.0 {
        return Err(GenError::Invalid(
            "grooving needs a positive stepdown".into(),
        ));
    }
    if p.stock_to_leave < 0.0 {
        return Err(GenError::Invalid(format!(
            "stock to leave of {:.3} is stock to remove - the floor is the floor; \
             negative leave is not a thing",
            p.stock_to_leave
        )));
    }
    let cs = centers(win, dom, p.blade_width * 0.5)?;
    let part_r = profile::profile_of(facets, &f)?.r_max;
    let ref_top = StockEnv::top(stock, part_r, revolve::span_max(&sg, win.0, win.1));
    let mut b = PathBuilder::new(f.point(rho_clear, f.station_of(p.clearance_z)));
    for &s in &cs {
        let base = revolve::span_max(&sg, s - p.blade_width * 0.5, s + p.blade_width * 0.5);
        // #871: a part-way refusal hands back the stations so far.
        let tips = rungs(base, ref_top, p.stepdown, p.stock_to_leave).map_err(|e| e.with_ghost_moves(b.moves()))?;
        plunge(&mut b, &f, s, rho_clear, &tips, p.plunge_feed);
    }
    b.push(
        MoveKind::Rapid,
        f.point(rho_clear, f.station_of(p.clearance_z)),
        None,
    );
    Ok(b.finish())
}

/// Groove finishing: one full-depth plunge per center to the band's
/// floor, leave-free - the model's own surface, tangentially, at
/// every center.
pub fn groove_finish(
    facets: &[Triangle],
    p: &GrooveFinishParams,
) -> Result<Toolpath, GenError> {
    groove_finish_in(facets, p, None)
}

/// [`groove_finish`] inside a setup stock (#130): every rapid clears
/// the bar's OD.
pub fn groove_finish_in(
    facets: &[Triangle],
    p: &GrooveFinishParams,
    stock: Option<StockEnv>,
) -> Result<Toolpath, GenError> {
    let (f, _, sg, win, dom, rho_clear) = setup(
        facets,
        p.clearance_z,
        p.top_z,
        p.bottom_z,
        p.blade_width,
        p.tolerance,
        "groove",
        "grooving blade",
        stock,
    )?;
    let cs = centers(win, dom, p.blade_width * 0.5)?;
    let mut b = PathBuilder::new(f.point(rho_clear, f.station_of(p.clearance_z)));
    for &s in &cs {
        let base = revolve::span_max(&sg, s - p.blade_width * 0.5, s + p.blade_width * 0.5);
        plunge(&mut b, &f, s, rho_clear, &[base], p.plunge_feed);
    }
    b.push(
        MoveKind::Rapid,
        f.point(rho_clear, f.station_of(p.clearance_z)),
        None,
    );
    Ok(b.finish())
}
