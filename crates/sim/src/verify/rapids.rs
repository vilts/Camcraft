//! Law (d), lateral rapids under the clearance plane over rest material - the shop practice of traversing only above the stock (machine-tool handbooks) on our own conservative rest model (split from `engine.rs` by M13.7; structure only).
//!
//! Issue #106: the column is read under the tool's whole footprint -
//! the disc of radius `r - gouge_tol` the mill law's other disc
//! readings use ([`Rest::hidden_along`], [`super::rapids_radial`]) -
//! never under its centre alone: a dive whose centre stands just off
//! the stock outline still meets the stock with its rim, and a dive
//! whose centre an earlier cut cleared can still land its far side on
//! uncut stock. Each sample is judged at the height the tool's own
//! tip stands at its radius - the tip plane plus the ball's, bull
//! corner's or point's rise there ([`Cutter::lift_at`]), so a ball
//! diving beside a wall whose rim is lifted clear of it is air (the
//! judged cutter: square when the context names no tip, which can
//! only over-report).
//!
//! Sampled (PR #138 review): [`RINGS`] rings at radii `d k / RINGS`,
//! `k = 1..=RINGS` about the centre, ring k holding `2 k` points (the
//! rim's [`RIM`] = 16 density scaled by radius). A sample on ring k is
//! judged at the lift of ring k - 1, the lowest the tool surface
//! stands anywhere over the band between the two rings (the lift only
//! rises outward), so stock the band holds is never judged against a
//! tip lifted higher than the tool really is there. Bounded rather
//! than exact: stock reaching into the footprint by less than one
//! ring spacing `d / RINGS` without touching a ring sample, or poking
//! between two samples of a ring by less than the chord bulge
//! `rho (1 - cos(pi / (2 k)))` (about `0.019 d` at the rim, the bound
//! [`super::rapids_radial`] states), can pass.
//!
//! Marched (issue #140): a lateral rapid is judged along its whole
//! length, never at its start, midpoint and end alone - a link at floor
//! depth whose three samples all stand in cut air can still drive
//! through a narrow uncut wall between two pockets. The footprint is
//! read at stations every `s = d / RINGS` (the ring spacing; floored at
//! [`MIN_STEP`] for a tool no wider than the gouge tolerance), both ends
//! included, so the step count is `ceil(len / s)`. The bound: the
//! footprint reaches `d` ahead of and behind each station, so the swept
//! stations overlap and a wall across the rapid can slip between them
//! only by being thinner than the step minus that reach - a non-positive
//! gap here (`s <= d`); what remains is the footprint's own sampling
//! bound above, the same at every station (stock between two samples
//! of one footprint by less than `d / RINGS` or the chord bulge). The
//! centre sample alone, read at the tip plane, already catches any wall
//! crossing the centre line thicker than `s`. This reading runs with or
//! without a model: [`Rest::hidden_along`] returns early on a 2.5-D
//! path, the column reading never does. Cost: a `RINGS`-ring footprint
//! (73 samples) per station; a rapid longer than [`MAX_STATIONS`]
//! steps (about 98 m for a 6 mm tool, 2.6 m at the floor step) is not
//! thinned to fit - it fails closed and is convicted, a refusal with a
//! name rather than a silent pass.
use super::laws::xy;
use super::midpose;
use super::probe::Cutter;
use super::rapids_posed::{self, PartIndex};
use super::rapids_radial;
use super::rest::Rest;
use super::types::{Mode, VerifyContext, Warning};

use crate::cl::{MoveKind, Toolpath};
use crate::geo2d::Point2;

/// (d) Rapids: lateral travel under the clearance plane inside stock.
/// Mode::Revolve stands this law down (M5.2 D): the XY column is the
/// mill's geometry - a turning insert is a POINT in XY and its whole
/// form lives in the meridian plane. The pull-out from a thread's
/// last V slides along flanks that coincide with the cut's own
/// surfaces, yet the column above the tip circle is solid core
/// forever - so this law convicts every full-depth threading retract
/// for a collision the meridian scan knows cannot happen. The
/// turning rapids' safety promise is the kernel's own: radial exits
/// from cut stations and traverses at rho_clear, asserted by each
/// family's tests. Fixture steel (d2) still stands - it is a box.
/// Mode::Radial leaves the column on the same reading (M5.2 F2): the
/// bore is a tunnel a height field cannot see, so the retract
/// sliding out along its own kerf reads as lateral travel under
/// solid stock. It is judged instead by the solid reading of
/// [`super::rapids_radial`] (#79): the flute body against the stock
/// prism minus the kerf of the cuts before it - the kernel's
/// traverse promise is proved again at the post gate, never trusted.
///
/// A POSED rapid (held or stated pose off the upright) that the
/// column reading convicts is re-read against the solid before the
/// conviction stands ([`super::rapids_posed`]): the height field
/// cannot see the air under an overhang, and a swarf link retracting
/// down a contact normal under a tube's equator is air by the mesh.
/// Upright rapids keep the column verdict byte-identical.
pub(super) fn rapids_in_stock(
    path: &Toolpath,
    cx: &VerifyContext,
    rest: &Rest,
    cutter: &Cutter,
    warns: &mut Vec<Warning>,
) {
    if cx.mode == Mode::Radial {
        rapids_radial::radial_rapids(path, cx, rest, warns);
        return;
    }
    if !matches!(cx.mode, Mode::Revolve | Mode::Rotary) {
        let poses = midpose::poses(path);
        let mut part = PartIndex::new();
        for (i, m) in path.moves.iter().enumerate() {
            if m.kind != MoveKind::Rapid {
                continue;
            }
            let (a, b) = (xy(&m.from), xy(&m.to));
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            // #904: the footprint gives a coarse model's excess back.
            let d = cx.tool.radius - cx.tightened(cx.gouge_tol);
            if dx * dx + dy * dy <= cx.z_tol * cx.z_tol {
                // Issue #96: a vertical retract rides the tool's own
                // kerf, but a vertical rapid DOWN is judged by its
                // column - below the surface the cuts before it left
                // (a drill's R plane under unfaced stock) it is a
                // rapid in stock. A peck's re-entry stays above its
                // own earlier plunge, so it is acquitted by the cuts.
                let down = m.to.z < m.from.z - cx.z_tol;
                let under = |(q, rho): (Point2, f64)| {
                    rest.in_stock(q) && m.to.z + cutter.lift_at(rho) < rest.surface_before(q, i) - cx.z_tol
                };
                if down && footprint(a, d).any(under) {
                    warns.push(Warning::RapidInStock { at: i });
                }
                continue;
            }
            let low = m.from.z.min(m.to.z);
            if low >= cx.clearance_z - cx.z_tol {
                continue;
            }
            // The overhang backstop (M13.7 B, [`super::hidden`]): the
            // tool disc swept through stock hidden under a ledge, which
            // no column surface shows - asked only when the column law
            // is silent, so it adds convictions, never removes one.
            // Cut order (M13.7): the rapid meets the stock as the cuts
            // BEFORE it left it - a later feed's clearing is no alibi.
            // Issue #140: every station along the rapid, not three.
            // A rapid at or above the stock top cannot meet the column -
            // `surface_before` is capped at the stock top and the lift is
            // never negative - so the march is skipped (PR #160 review):
            // same verdict, far fewer samples. A NaN height still marches.
            let above = low >= cx.stock_top_z;
            let marched = !above
                && stations(a, b, d).is_none_or(|mut it| {
                    it.any(|c| {
                        footprint(c, d)
                            .any(|(q, rho)| rest.in_stock(q) && low + cutter.lift_at(rho) < rest.surface_before(q, i))
                    })
                });
            let bad = marched || rest.hidden_along(a, b, low, d, i);
            if !bad {
                continue;
            }
            let pose = poses.get(i).copied().unwrap_or((m.axis, m.axis));
            if rapids_posed::leaning(pose) && !rapids_posed::in_rest(m, i, pose, cx, rest, cutter, &mut part) {
                continue; // the solid reading cleared what the column misread
            }
            warns.push(Warning::RapidInStock { at: i });
        }
    }
}

/// Rim samples of the footprint disc (#106) - [`super::rapids_radial`]'s count.
const RIM: usize = 16;

/// Rings of the footprint disc out to its rim (PR #138 review).
const RINGS: usize = 8;

/// The tool footprint's sample points about `c`, each with the radius
/// its lift is read at - the next ring INWARD's (see the module doc):
/// the centre (radius 0), then ring k = 1..=[`RINGS`] at `d k / RINGS`
/// with `RIM k / RINGS` points, read at `d (k - 1) / RINGS`. A `d`
/// that is not a finite positive radius (a tool no wider than the
/// gouge tolerance) is the centre alone - the column reading the law
/// had before #106.
fn footprint(c: Point2, d: f64) -> impl Iterator<Item = (Point2, f64)> {
    let d = if d.is_finite() && d > 0.0 { d } else { 0.0 };
    let rings = if d > 0.0 { RINGS } else { 0 };
    std::iter::once((c, 0.0)).chain((1..=rings).flat_map(move |k| {
        let rad = d * k as f64 / RINGS as f64;
        let inner = d * (k - 1) as f64 / RINGS as f64;
        let n = (RIM * k).div_ceil(RINGS).max(1);
        (0..n).map(move |j| {
            let t = std::f64::consts::TAU * j as f64 / n as f64;
            (Point2::new(c.x + rad * t.cos(), c.y + rad * t.sin()), inner)
        })
    }))
}

/// The smallest march step, mm - a tool no wider than the gouge
/// tolerance still marches at a finite count.
const MIN_STEP: f64 = 0.01;

/// The most stations one rapid is marched at (issue #140); past it the
/// rapid fails closed rather than marching at a thinned step.
const MAX_STATIONS: usize = 1 << 18;

/// The footprint centres along `a -> b` every `d / RINGS` at most
/// (see the module doc), both ends included. `None` is a rapid that
/// cannot be marched at that step - a non-finite length or more than
/// [`MAX_STATIONS`] steps - and the caller convicts it (fail closed).
fn stations(a: Point2, b: Point2, d: f64) -> Option<impl Iterator<Item = Point2>> {
    let d = if d.is_finite() && d > 0.0 { d } else { 0.0 };
    let step = (d / RINGS as f64).max(MIN_STEP);
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt();
    if !len.is_finite() {
        return None;
    }
    let n = (len / step).ceil();
    if n > MAX_STATIONS as f64 {
        return None;
    }
    let n = (n as usize).max(1);
    Some((0..=n).map(move |k| {
        let t = k as f64 / n as f64;
        Point2::new(a.x + dx * t, a.y + dy * t)
    }))
}
