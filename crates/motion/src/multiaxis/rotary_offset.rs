//! Rotary offset roughing (M5.1 stage E2): the pocket ladder run in
//! WRAP SPACE - the developed surface of the fitted axis [`super::axis`]
//! - so a turned work is roughed by the machinery that already knows
//!   how to erode regions, dodge islands and enter levels, applied to
//!   a cylinder instead of a table.
//!
//! The map: every vertex keeps its axial station `v = (p - o)·û` and
//! its radius `ρ = |(p - o) ⊥ û|`, and its azimuth becomes the
//! developed coordinate `s = R_ref·θ` (θ measured about the `e1/e2`
//! basis [`axis`] fits; R_ref the work's largest radius, so `s` is a
//! true arc length on the skin). A facet with one vertex ON the axis
//! closes the lathe onto it (a pole, or the fan across a flat cap)
//! and sweeps a full developed RECTANGLE between its two rim
//! vertices' columns; every other facet develops as itself, and an
//! axis vertex never lands on an affine blend of a meaningless
//! azimuth - which is what lets a fan of wedges tile one closed
//! chain of chords per level instead of opening on every spoke.
//! Each facet also picks its own lap: a plain one when its columns
//! fit within half a turn, a seam one (one column riding exactly
//! 2π low) when they straddle the ±π branch, so the same mesh vertex
//! keeps one developed coordinate in every facet that shares it.
//! The mesh is then duplicated one full period (+2πR_ref in `s`) so
//! every chord chain runs into its own copy at the seam, and each
//! free seam edge left at the cover's two ends hangs a CURTAIN down
//! to the floor radius ([`super::rotary_seam`]: the meridian wall on
//! a revolve, the ragged seam column on a flatted bar), closing the
//! chain's two far ends. An open mesh is refused by name. A seam that
//! crosses a feature which is not turned (a D-flat, a keyway) is moved:
//! the azimuth basis turns to a seam angle where the bar is turned
//! ([`super::rotary_seam_angle`], #226), and only a feature all the way
//! round is refused. The wrap is a
//! closed solid exactly when the model was: a revolve's ring slices
//! to a closed contour at every level the way block work expects,
//! and a pass running most of a lap rewraps to a closed ring
//! instead of leaving a rest ridge at the seam. Levels in AIR - above
//! the part's skin, over a fatter bar - meet no facet and need no
//! copy: they clear one lap plus the seam pad (M13.8).
//!
//! With ρ as height, `pocket3d`'s island law reads exactly right:
//! part material stands above the current radius wherever the skin
//! reaches further out than the level, so a level's target is
//! precisely the columns already turned inside that radius, the
//! ladder peels ρ_max → ρ_min (ρ_max the STOCK's outer radius when
//! the caller hands one over a fatter bar - M13.8, where the levels
//! above the part's skin are the whole lap), and the ledge under a column's last
//! legal level is the finishing kinds' job - the same staircase law,
//! one seam over. End caps ride the wedge law too: their rectangle
//! stands at the cap's station between the axis and the rim, a real
//! chord closing every slice it spans.
//!
//! The world exit is [`wrap_exit`]: ball centers back at ρ plus one
//! tool radius with the tip posted under them (M13 pivot law), arcs
//! chorded, s-sweeps resampled, rapids and cut moves standing
//! RADIAL with the lean of [`super::tilt`], as in [`super::rotary`].
//! Unlike the twins there is no LEAN refusal: the developed cylinder
//! is neutral to which way the axis points - a bar may stand or lie
//! and the rough ladder runs the same. The caller's world
//! `top_z`/`bottom_z` window the STATIONS (the contour kind's own
//! `(h - o.z)/u.z` law) and are silently inert on a bar lying flat,
//! where no world plane names a station.
//!
//! Provenance: cylindrical/unwrapped toolpath generation and radial
//! infeed roughing from public CAM literature and turning practice;
//! composed on this codebase's own pocket, axis-fit and lean laws.
//! Nothing of any vendor's consulted.

use crate::cl::Toolpath;
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::machining3d::{self, Pocket3DParams};

use super::axis;
use super::rotary_seam;
use super::rotary_seam_angle;
use super::rotary_sheet::{self, Basis, Sheet};
use super::tilt::AxisControl;
use super::wrap_exit;

/// World up, spelled once (Point3::new is not a const fn).
const UP: Point3 = Point3 {
    x: 0.0,
    y: 0.0,
    z: 1.0,
};
/// The axis' up-component below which world heights cannot name axial
/// stations (a plane nearly parallel to the axis meets it, if at all,
/// a million stations away): the twins' own sin-1 degree boundary.
const LEAN_MIN: f64 = 0.0175;
/// One full lap of development, in radians.
const TAU: f64 = std::f64::consts::TAU;

/// The rotary offset rough: axis fit, wrap map, the pocket ladder in
/// developed space, the radial-pose world exit.
///
/// `p` carries the pocket's own numbers (diameter, stepover, leave,
/// stepdown as the RADIAL infeed, feeds, strategy, entry law); its
/// height fields are read differently here: `top_z`/`bottom_z` window
/// the stations (above), the ρ ladder - `top_z = rho_max`,
/// `bottom_z = rho_min` - is computed from the mesh, and the rapids
/// plane sits one infeed and one tool radius outside the skin.
/// With no stock to read the part's own skin is the top: see
/// [`rotary_offset_in`].
pub fn rotary_offset(
    facets: &[Triangle],
    p: &Pocket3DParams,
    c: &AxisControl,
) -> Result<Toolpath, GenError> {
    rotary_offset_in(facets, None, p, c)
}

/// The rotary offset rough over its setup's `stock` (M13.8): the ρ
/// ladder's top, its rapids cylinder and the development's arc-length
/// radius are the STOCK's outer radius about the fitted axis (its
/// farthest vertex - a round bar's skin, a block's corner), never less
/// than the part's - a ladder peeling from the part's skin inside a
/// fatter bar hops through skin it never cut. The stepover then holds
/// as an arc length on the outermost level and only tightens inward.
pub fn rotary_offset_in(
    facets: &[Triangle],
    stock: Option<&[Triangle]>,
    p: &Pocket3DParams,
    c: &AxisControl,
) -> Result<Toolpath, GenError> {
    let (base, d) = develop(facets, stock, p)?;
    // Adaptive's in-cut kink law holds on the posted chords too; the
    // other strategies keep the sagitta-only chords they always had.
    let cap = (p.strategy == crate::machining2d::PocketStrategy::Adaptive)
        .then(|| wrap_exit::ADAPTIVE_TURN_DEG.to_radians());
    wrap_exit::wrap_exit(&base, &d.ax, &d.e1, &d.e2, d.rho_ref, d.r, c, cap)
}

/// [`rotary_offset_in`] about an axis the caller STATES and has judged
/// (#2772 parts 1-2, `axis_stated::judged_any`) - the machine's rotary
/// line or the operation's own direction, anchored through the work
/// offset's origin. The same development the plain entry wraps about
/// its fit; the fit stays inside the plain entry.
pub fn rotary_offset_stated(
    facets: &[Triangle],
    stock: Option<&[Triangle]>,
    ax: axis::Axis,
    p: &Pocket3DParams,
    c: &AxisControl,
) -> Result<Toolpath, GenError> {
    let (base, d) = develop_about(facets, stock, ax, p)?;
    let cap = (p.strategy == crate::machining2d::PocketStrategy::Adaptive)
        .then(|| wrap_exit::ADAPTIVE_TURN_DEG.to_radians());
    wrap_exit::wrap_exit(&base, &d.ax, &d.e1, &d.e2, d.rho_ref, d.r, c, cap)
}

/// The DEVELOPED program [`rotary_offset_in`] wraps (issue #9): the
/// ladder's own path in wrap space - s the arc length at the stock's
/// radius, v the station, height the radius - before the world exit
/// poses it. The plane the planner's load and kink laws are read on.
pub fn rotary_offset_developed(
    facets: &[Triangle],
    stock: Option<&[Triangle]>,
    p: &Pocket3DParams,
) -> Result<Toolpath, GenError> {
    develop(facets, stock, p).map(|(base, _)| base)
}

/// What the world exit needs of the development.
struct Developed {
    ax: axis::Axis,
    e1: Point3,
    e2: Point3,
    rho_ref: f64,
    r: f64,
}

fn develop(
    facets: &[Triangle],
    stock: Option<&[Triangle]>,
    p: &Pocket3DParams,
) -> Result<(Toolpath, Developed), GenError> {
    if facets.is_empty() {
        return Err(GenError::Invalid("no facets to wrap".into()));
    }
    rotary_seam::refuse_open(facets)?;
    let ax = axis::fit(facets)?;
    develop_about(facets, stock, ax, p)
}

/// The development about a stated axis: the fit's own body, the axis
/// a caller that judged it hands in (#2772 parts 1-2).
fn develop_about(
    facets: &[Triangle],
    stock: Option<&[Triangle]>,
    ax: axis::Axis,
    p: &Pocket3DParams,
) -> Result<(Toolpath, Developed), GenError> {
    if facets.is_empty() {
        return Err(GenError::Invalid("no facets to wrap".into()));
    }
    rotary_seam::refuse_open(facets)?;
    let u = ax.dir;
    // The axis.rs basis law, verbatim: any vector off the axis, then
    // the right-handed pair it completes.
    let any = if u.z.abs() < 0.9 {
        UP
    } else {
        Point3::new(1.0, 0.0, 0.0)
    };
    let e1 = match normalize(&u.cross(any)) {
        Some(v) => v,
        None => {
            return Err(GenError::Invalid(
                "the fitted axis is not a direction - the mesh is corrupt".into(),
            ))
        }
    };
    let e2 = u.cross(e1);

    // The mesh's own radial and station extents.
    let (mut rho_max, mut rho_min) = (0.0f64, f64::INFINITY);
    let (mut v_lo, mut v_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for t in facets {
        for q in [t.a, t.b, t.c] {
            let d = q - ax.origin;
            let v = d.dot(u);
            let w = Point3::new(d.x - u.x * v, d.y - u.y * v, d.z - u.z * v);
            let rho = (w.dot(w)).sqrt();
            rho_max = rho_max.max(rho);
            rho_min = rho_min.min(rho);
            v_lo = v_lo.min(v);
            v_hi = v_hi.max(v);
        }
    }
    if rho_max <= 0.0 || !rho_max.is_finite() {
        return Err(GenError::Invalid(
            "the work has no radius to wrap - every facet sits on the axis".into(),
        ));
    }
    let rho_stock = stock.map_or(0.0, |s| {
        s.iter()
            .flat_map(|t| [t.a, t.b, t.c])
            .map(|q| {
                let d = q - ax.origin;
                let v = d.dot(u);
                let w = Point3::new(d.x - u.x * v, d.y - u.y * v, d.z - u.z * v);
                w.dot(w).sqrt()
            })
            .filter(|x| x.is_finite())
            .fold(0.0f64, f64::max)
    });
    let rho_ref = rho_max.max(rho_stock);

    // The world stations window, on standing axes only (doc header).
    let uz = u.dot(UP);
    if uz.abs() >= LEAN_MIN {
        let s_top = (p.top_z - ax.origin.z) / uz;
        let s_bot = (p.bottom_z - ax.origin.z) / uz;
        v_lo = v_lo.max(s_bot.min(s_top));
        v_hi = v_hi.min(s_top.max(s_bot));
        if v_lo >= v_hi {
            return Err(GenError::Empty(
                "the height window leaves no station of the work to rough".into(),
            ));
        }
    }

    // The wrap mesh and its developed extent (`rotary_sheet`), split
    // at the basis' ±π branch. Only a turned seam is wrapped (#54;
    // `rotary_seam` says why): when the default seam crosses a feature
    // the basis turns to an azimuth where it does not (#226,
    // `rotary_seam_angle`), and the turned sheet is judged again. The
    // world exit maps back through the same basis, so the seam's place
    // changes where the development splits, never where metal is cut.
    let r = p.tool_diameter * 0.5;
    let period = TAU * rho_ref;
    let rho_eps = 1e-12 * rho_max;
    let mut basis = Basis { e1, e2 };
    let mut sheet = rotary_sheet::lay(facets, &ax, basis, rho_ref, rho_eps);
    if rotary_seam::seam_columns(&sheet.seam_cols, rho_max).is_some() {
        let Some(delta) = rotary_seam_angle::turned_seam(facets, &ax, basis, rho_eps, rho_max)? else {
            let n = rotary_seam::seam_columns(&sheet.seam_cols, rho_max).unwrap_or(0);
            return Err(rotary_seam::seam_refusal(n, true));
        };
        basis = basis.turned(u, delta);
        sheet = rotary_sheet::lay(facets, &ax, basis, rho_ref, rho_eps);
        if let Some(n) = rotary_seam::seam_columns(&sheet.seam_cols, rho_max) {
            return Err(rotary_seam::seam_refusal(n, false));
        }
    }
    let Sheet { mut wrap, edges, s_lo, s_hi, .. } = sheet;
    let Basis { e1, e2 } = basis;

    // The cover's two ends are the chain's loose ends: a curtain hangs
    // from each free seam edge down to the floor radius, so every
    // level's chord chain closes at each end of the cover into a ring,
    // not a loose thread - on a revolve the meridian wall at the end
    // column, on a flatted bar the ragged column its seam really is.
    wrap.extend(rotary_seam::curtains(&edges, period, rho_min.max(0.0)));

    // The ladder in wrap space, run by pocket3d's own entry guards.
    let mut q = *p;
    q.top_z = rho_ref;
    q.bottom_z = rho_min.max(0.0);
    q.clearance_z = rho_ref + p.stepdown + r;
    q.retract_z = None;
    if q.top_z < q.bottom_z {
        return Err(GenError::Empty(
            "the wrapped work has no radial extent to rough".into(),
        ));
    }
    // The pad is the SEAM's margin only: in v the rect stops at the
    // work's own (windowed) stations - erosion already keeps a
    // centerline r inside a boundary while its tool edge reaches the
    // boundary itself, and padding past an end face would invent
    // phantom air-cut bands where the slice sees no material.
    let pad = r + q.stock_to_leave;
    let rect = wrap_rect(s_lo - pad, s_hi + pad, v_lo, v_hi);
    // Air levels (M13.8): a plane above the part's skin plus its
    // leave meets no wrapped facet, so its target is the whole cover -
    // and the cover is two laps, the seam copy included. One lap plus
    // the seam pad each side clears every azimuth once (the eroded
    // centres reach both pads, the tool edge one radius past them),
    // so those planes sweep one lap; every plane that can meet the
    // part keeps both, the seam copy closing its rings.
    let one_lap = wrap_rect(s_lo - pad, (s_lo + period + pad).min(s_hi + pad), v_lo, v_hi);
    let air_above = rho_max + q.stock_to_leave.max(0.0);
    // The seam (issue #9): a level whose area a cover end cuts short of
    // a whole lap is refused Adaptive by name (`machining3d::hsm::Seam`).
    // #261 part 3: a ghost built in the development is not where the
    // part is - the refusal keeps its words and drops the moves.
    let base = machining3d::pocket3d_air_split(&wrap, &one_lap, &rect, air_above, Some((period, 2.0 * pad)), &q)
        .map_err(GenError::without_ghost)?;
    Ok((base, Developed { ax, e1, e2, rho_ref, r }))
}

/// The wrap's footprint: one rectangle over the covered development
/// and the (windowed) station range - every wrapped facet is inside,
/// and the period copies keep the seam's slices honest.
fn wrap_rect(s0: f64, s1: f64, v0: f64, v1: f64) -> Region {
    let p = |x: f64, y: f64| Point2::new(x, y);
    match Loop::new(vec![p(s0, v0), p(s1, v0), p(s1, v1), p(s0, v1)]) {
        Ok(l) => match Region::new(vec![l]) {
            Ok(rg) => rg,
            Err(_) => Region::empty(),
        },
        Err(_) => Region::empty(),
    }
}

fn normalize(v: &Point3) -> Option<Point3> {
    let l = (v.dot(*v)).sqrt();
    if !l.is_finite() || l <= f64::EPSILON {
        None
    } else {
        Some(Point3::new(v.x / l, v.y / l, v.z / l))
    }
}
