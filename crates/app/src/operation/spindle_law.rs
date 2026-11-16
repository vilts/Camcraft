//! The lathe spindle law (issue #62): a turning op's axis of revolution
//! is fitted from the part's facets, but the lathe post writes every
//! diameter about the work offset's origin and every Z as world Z - it
//! programs a spindle that IS world Z through that origin. A part whose
//! fitted axis is not that line (a few degrees of lean, or a millimetre
//! off the origin) would be turned about a spindle it is not on: a taper,
//! an eccentric or a coned face the model does not have, and a lean in
//! one meridian plane gives the post nothing to notice. So the door is
//! here, twice: before a move is computed (`collect_job`), and again at
//! the post (`posting::program_ops`) against the origin the post is
//! about to receive - a work offset moved after generation must never
//! post a program measured from the old one. Off the spindle by more
//! than the post can write, the op is refused by name with the lean and
//! the offset it measured, and the act that fixes it.
//!
//! The measure is the whole rigid displacement, not the axis line alone:
//! every part vertex is read twice - (radius, height) about the fitted
//! axis, the generator's reading, and (radius, Z) about world Z through
//! the origin, the post's - and the largest disagreement over the part
//! is what is bounded. A lean `a` moves a point at radius `R` by `R*a`
//! in Z (a face posted as a cone), on top of the axis drift over the
//! part's height; a thin wide flange is judged by its radius, a long
//! bar by its length.
//!
//! The fit is outlier-resistant (issue #71): a keyway, flat or cross
//! hole is set aside and the axis read on the revolved surface, so the
//! refusal names that share and tells a misplaced part (move the work
//! offset) from a fit the features left in doubt (moving it may not
//! help). A part that passes with features set aside generates with a
//! note naming that share: those features are not turned.
//!
//! Provenance: a lathe's programmed X is the diameter about the spindle
//! centreline and Z is measured along it (RS-274 / ISO 6983 lathe
//! convention, any turning handbook); the tolerance is the posts' own
//! output lattice. Our own design notes; no vendor material consulted.

use flecs_ecs::prelude::*;

use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

/// How far any part point may read differently about the fitted axis
/// than about the spindle, in mm, per posted word: half the last digit
/// of the diameter word in radius (X is written at 0.001 mm of
/// diameter) and half the Z word's 0.001 mm - so a part that passes
/// posts every word within one last digit of what a perfectly placed
/// part would. The fit of an honest revolve sits far below it (the
/// non-uniform, f32-stored revolve in `lathe_spindle_law` measures it).
pub const SPINDLE_TOL: f64 = 0.0005;
/// The up-component below which the turning frame itself refuses the
/// axis as lying (`turning::profile`'s LEAN_MIN): such a model keeps
/// that refusal's own wording, so this door stands aside for it.
const LYING: f64 = 0.0175;

/// What the law measured on one part: the fitted axis against world Z
/// through the work offset's origin.
#[derive(Clone, Copy, Debug)]
pub struct SpindleReading {
    /// The fitted axis' angle off world Z, degrees.
    pub lean_deg: f64,
    /// The axis line's XY distance from the origin at `mid_z`, mm.
    pub offset_mid: f64,
    /// The part's mid-height (world Z), where `offset_mid` is read.
    pub mid_z: f64,
    /// The worst radius disagreement over the part's vertices, mm.
    pub radial: f64,
    /// The worst height disagreement over the part's vertices, mm.
    pub axial: f64,
    /// The area share (0..1) of the part's surface the fit set aside
    /// as not revolved - keyways, flats, cross holes (issue #71).
    pub outlier_share: f64,
    /// False when that share was too large for the fit to refit on:
    /// the axis is then the plain fit, biased by those features.
    pub trusted: bool,
}

impl SpindleReading {
    /// The worst disagreement of either word; NaN stays NaN (refused).
    #[must_use]
    pub fn worst(&self) -> f64 {
        if self.radial.is_nan() || self.axial.is_nan() {
            return f64::NAN;
        }
        self.radial.max(self.axial)
    }
}

/// Read the part about its fitted axis and about the spindle through
/// `origin`. `None` when the fit cannot read the model as revolved or
/// the axis lies near the table: the turning generators refuse those by
/// their own names, in their own order (the lathe's axial drill at its
/// `axial_doors`, its cross drill at `cross_spindle::lathe_reads` -
/// issue #67; a mill-turn's cross drill takes a prismatic bar, measured
/// about the spindle as it stands).
#[must_use]
pub fn spindle_reading(facets: &[Triangle], origin: [f64; 3]) -> Option<SpindleReading> {
    let fit = crate::multiaxis::axis::fit_report(facets).ok()?;
    let ax = fit.axis;
    let d = ax.dir;
    let n = d.dot(d).sqrt();
    if !n.is_finite() || n <= 0.0 {
        return None;
    }
    let mut u = Point3::new(d.x / n, d.y / n, d.z / n);
    if u.z < 0.0 {
        u = Point3::new(-u.x, -u.y, -u.z);
    }
    if u.z < LYING {
        return None;
    }
    let c = ax.origin;
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut radial, mut axial, mut finite) = (0.0f64, 0.0f64, true);
    for p in facets.iter().flat_map(|t| [t.a, t.b, t.c]) {
        let q = p - c;
        let s = q.dot(u);
        // The generator's reading: radius off the fitted axis, and the
        // world height of the axis point at that station (stations are
        // world Z planes read where they meet the axis).
        let (fx, fy, fz) = (q.x - u.x * s, q.y - u.y * s, q.z - u.z * s);
        let rf = (fx * fx + fy * fy + fz * fz).sqrt();
        let zf = c.z + s * u.z;
        // The post's: radius about world Z through the origin, world Z.
        let (dx, dy) = (p.x - origin[0], p.y - origin[1]);
        let rs = (dx * dx + dy * dy).sqrt();
        let (er, ez) = ((rs - rf).abs(), (p.z - zf).abs());
        if !er.is_finite() || !ez.is_finite() {
            finite = false;
            continue;
        }
        radial = radial.max(er);
        axial = axial.max(ez);
        lo = lo.min(p.z);
        hi = hi.max(p.z);
    }
    if !finite || !lo.is_finite() || !hi.is_finite() {
        radial = f64::NAN;
    }
    let mid_z = if lo.is_finite() && hi.is_finite() { 0.5 * (lo + hi) } else { c.z };
    let t = (mid_z - c.z) / u.z;
    let (ox, oy) = (c.x + u.x * t - origin[0], c.y + u.y * t - origin[1]);
    Some(SpindleReading {
        lean_deg: u.z.clamp(-1.0, 1.0).acos().to_degrees(),
        offset_mid: (ox * ox + oy * oy).sqrt(),
        mid_z,
        radial,
        axial,
        outlier_share: fit.outlier_share,
        trusted: fit.trusted,
    })
}

/// Below this set-aside share (0..1) the fit's outliers are read as
/// noise and the generation note stays silent: a plain revolve sets
/// nothing aside, and 0.05 % of the turned surface is no feature.
const NOTE_FLOOR: f64 = 0.0005;

/// The set-aside share as a percentage for the messages: one decimal,
/// and never "0.0%" for a share the fit did set aside.
fn shown_pct(share: f64) -> f64 {
    (100.0 * share).max(0.1)
}

/// The non-blocking note a trusted fit carries when it set features
/// aside (issue #71 review): the op turns the revolved surface only, so
/// an eccentric pin is turned as a concentric cylinder about the axis
/// (material left on) and a keyway is left standing - said by name,
/// never in silence. Not a warning: nothing unsafe is cut, so it does
/// not hold the post. `None` below [`NOTE_FLOOR`] or for an untrusted
/// reading (which [`check`] refuses).
#[must_use]
pub fn set_aside_note(r: &SpindleReading) -> Option<String> {
    if !r.trusted || !r.outlier_share.is_finite() || r.outlier_share <= NOTE_FLOOR {
        return None;
    }
    let pct = shown_pct(r.outlier_share);
    Some(format!(
        "the lathe axis was read on the revolved surface; {pct:.1}% of the turned (non-end-face) \
         surface (keyways, flats, holes, eccentric sections) was set aside and is not turned - \
         machine those features in a later setup"
    ))
}

/// Refuse by name a part that reads differently about its fitted axis
/// than about world Z through `origin` (the work offset's world
/// position) by more than [`SPINDLE_TOL`] anywhere on it. A pass
/// returns the fit's [`set_aside_note`], if any, for the op to carry.
pub(super) fn check(facets: &[Triangle], origin: [f64; 3], slot: &str) -> Result<Option<String>, GenError> {
    let Some(r) = spindle_reading(facets, origin) else {
        return Ok(None);
    };
    let worst = r.worst();
    if worst.is_finite() && worst <= SPINDLE_TOL {
        return Ok(set_aside_note(&r));
    }
    Err(refusal(&r, slot))
}

/// The refusal of a reading off the spindle by more than
/// [`SPINDLE_TOL`]: an untrusted fit says the reading is biased, a
/// trusted one names the misplacement and the act that fixes it.
fn refusal(r: &SpindleReading, slot: &str) -> GenError {
    let worst = r.worst();
    let pct = shown_pct(r.outlier_share);
    if !r.trusted {
        // The fit itself is in doubt: moving the work offset onto a
        // biased axis may only move the error (issue #71).
        return GenError::Invalid(format!(
            "the part's axis of revolution cannot be read to the lathe spindle's {SPINDLE_TOL} mm: \
             {pct:.1}% of the part's turned (non-end-face) surface does not revolve about any one \
             axis (off-centre features - keyways, flats, cross holes), and the axis read through \
             them stands {:.3} mm off the {slot} origin, leaning {:.3} degrees - the reading is \
             biased by off-centre features; moving the work offset may not fix it; turn the \
             revolved stock first and machine those features in a later setup, or model them \
             out of the turned part",
            r.offset_mid, r.lean_deg
        ));
    }
    let aside = if r.outlier_share > 0.0 {
        format!(
            " (read on the revolved surface; {pct:.1}% of the part's turned (non-end-face) surface - \
             keyways, flats, holes - set aside)"
        )
    } else {
        String::new()
    };
    GenError::Invalid(format!(
        "the part's axis of revolution is not the lathe spindle: it leans {:.3} degrees \
         off world Z and stands {:.3} mm off the {slot} origin at Z{:.3}{aside}, moving a part \
         point {worst:.4} mm (the lathe programs diameters about world Z through the work \
         offset, {SPINDLE_TOL} mm allowed) - turned as posted it would cut a taper, an \
         eccentric or a coned face; align the part's axis with world Z and move the work \
         offset onto it",
        r.lean_deg, r.offset_mid, r.mid_z
    ))
}

/// Whether `params` works about the spindle on the machine its `setup`
/// context mounts (#388 - the op's own, active or not):
/// the turning family, the lathe's Drill (axial or cross), and the
/// mill-turn's cross drill (issue #67) - whose standoffs are measured
/// about this same spindle, so a revolved part off it is refused here
/// by name rather than drilled about a line it is not on.
pub(crate) fn turns(world: &World, setup: Option<&str>, params: &super::OpParams) -> bool {
    let kind = crate::machine::kind_of_setup(world, setup);
    params.kind().info().family() == super::Family::Turn
        || match params {
            super::OpParams::Drill { cross, .. } => {
                kind == crate::machine::Kind::Lathe
                    || (kind == crate::machine::Kind::MillTurn && !cross.is_empty())
            }
            _ => false,
        }
}

/// The law again at the post, for operation entity `e` named `name`:
/// the work offset may have moved since generation, and the post is
/// about to measure every diameter from where it is NOW. `None` for an
/// op that does not turn, or whose part still sits on the spindle.
pub(crate) fn post_problem(world: &World, name: &str, e: &Entity) -> Option<String> {
    let op = crate::model::operation_of(world, e)?;
    let setup = op.setup.as_deref();
    if !turns(world, setup, &op.params) {
        return None;
    }
    let (_, origin) = crate::posting::wcs_context(world, setup);
    let slot = crate::setup::wcs_name(world, setup);
    // A material that cannot be read is a source that moved or went:
    // the freshness check right after this names that, as it always has.
    let m = super::material_of(world, setup, &op.src).ok()?;
    let why = check(&m.facets, origin, &slot).err()?;
    Some(format!("{name}: {why} - fix it and regenerate before posting"))
}

#[cfg(test)]
#[path = "spindle_law_tests.rs"]
mod tests;
