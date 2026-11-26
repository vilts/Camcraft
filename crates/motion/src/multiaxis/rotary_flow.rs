//! Rotary Flow (issue #2824, the first slice of #2804): the
//! 4-axis cylindrical flow-line finishing kind. The caller PICKS
//! the guide chains - the edges of a helical band on a rotor - and
//! states the rotation axis (the #2772 seam: the axis is a
//! PARAMETER here, never fitted from the mesh, so a helical model
//! no fit could answer still machines). The kernel unwraps each
//! chain to a continuous cylindrical track about the axis
//! (`rotary_flow_unwrap`), aligns each adjacent pair by the nearest
//! whole turn, develops the band `(s = R_ref * theta, v)`, chooses
//! the pass count from the stepover or the scallop law, blends
//! iso-parametric flow curves between the pair, pushes every
//! station's ball center out along its radial until the
//! closest-point oracle clears the whole mesh (`rotary_flow_pass`),
//! and poses and links the passes radially on the retract cylinders
//! (`rotary_flow_emit`) - ball-center ladders, tool tips out.
//!
//! This first slice needs at least two chains and a ball-nose tool;
//! empty, off-model, on-axis, reversed and degenerate picks are
//! refused by name, never guessed past.
//!
//! Provenance: cylindrical unwrapping and iso-parametric flow-line
//! CAM on developed rotary surfaces from published 4-axis machining
//! literature; the scallop, lean and pivot laws are this codebase's
//! own. No vendor material consulted.

use crate::cl::Toolpath;
use crate::finishing3d::scallop_stepover;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::{GenError, ARC_FIT_TOL};
use crate::tools::{Tool, ToolKind};

use super::axis::Axis;
use super::retip::radial_at;
use super::rotary_flow_emit::Emitter;
use super::rotary_flow_guard::prove_path;
use super::rotary_flow_pass::{boxes_of, flow_pass};
use super::rotary_flow_unwrap::{self, Basis, Unwrapped};
use super::tilt::AxisControl;

#[cfg(test)]
mod rotary_flow_tests;

/// The corner-lattice step for chain points (the drive-curve walk's
/// own DUST law, curve.rs): two reads of one CAD vertex differ by
/// float dust, never by a facet's breadth.
const DUST: f64 = 1e-6;
/// Matched-place samples that read a band's width and alignment.
const BAND_SAMPLES: usize = 33;
/// Most flow passes one band may take: a band that needs more is a
/// wrong pick or a wrong stepover, refused by name.
pub const MAX_PASSES: usize = 4096;

/// The pass's numbers. Heights read as BALL-CENTER radii about the
/// stated axis - the retract cylinders the links ride - never as
/// world Z: a rotor may stand or lie, and only a radius about the
/// axis names a safe place the same way for both.
pub struct RotaryFlowParams<'a> {
    /// The cutting tool. Rotary Flow is a ball law: the pass spacing,
    /// the push-out and the tip all ride a ball of `tool.diameter`;
    /// a tool of any other kind is refused by name.
    pub tool: &'a Tool,
    /// Developed stepover between flow passes, mm, read at the
    /// band's outermost radius (ground inside it gets finer, never
    /// coarser). `None` uses the scallop law
    /// [`scallop_stepover`]`(ball_r, tolerance)`.
    pub stepover: Option<f64>,
    /// Scallop height and chord budget, mm.
    pub tolerance: f64,
    /// Ball-center radius of the clearance cylinder: the program
    /// opens, hops bands and exits there.
    pub clearance: f64,
    /// Ball-center radius of the pass-link cylinder; `None` links at
    /// clearance.
    pub retract: Option<f64>,
    pub feed: f64,
    pub plunge_feed: f64,
    /// `true` walks each band as picked; `false` reverses each
    /// pass's travel (the drive-curve walk's own climb law).
    pub climb: bool,
}

/// The Rotary Flow program: the picked guide chains of one helical
/// band (or several bands, one per adjacent chain pair), machined
/// by flow lines between each pair.
///
/// The chains are 3D polylines ON the model - the picked edge
/// chains of the selection, each running ALONG the band (a helix of
/// the rotor). Two chains span one band; three or more machine one
/// band per adjacent pair (a shared chain is walked once per band
/// that uses it).
pub fn rotary_flow(
    chains: &[Vec<Point3>],
    facets: &[Triangle],
    ax: &Axis,
    p: &RotaryFlowParams<'_>,
    c: &AxisControl,
) -> Result<Toolpath, GenError> {
    let r = validate_numbers(p)?;
    validate_axis(ax)?;
    if facets.is_empty() {
        return Err(GenError::Empty(
            "Rotary Flow found no model to read the band from".into(),
        ));
    }
    validate_facets(facets)?;
    if chains.len() < 2 {
        return Err(GenError::Invalid(format!(
            "Rotary Flow needs at least two picked guide chains to span a band; got {} - pick \
             the two edges of the helical band",
            chains.len()
        )));
    }
    let basis: Basis = rotary_flow_unwrap::basis(ax)?;
    let boxes = boxes_of(facets);
    let rho_mesh = rotary_flow_unwrap::mesh_radius(ax, facets);
    let hop = p.retract.unwrap_or(p.clearance);
    for (name, v) in [("clearance", p.clearance), ("retract", hop)] {
        if !(v.is_finite() && v > rho_mesh + r + ARC_FIT_TOL) {
            // #677's own words, a radius this time: no spot is named.
            return Err(GenError::Invalid(format!(
                "Rotary Flow reads its {name} height as a ball-center radius about the stated \
                 axis: {v:.3} mm does not clear the work's {rho_mesh:.3} mm radius plus the \
                 {r:.3} mm ball - raise it"
            )));
        }
    }
    let diag = rotary_flow_unwrap::extent(facets);
    let cell = (diag * DUST).max(1e-9);
    let eps2 = (4.0 * cell) * (4.0 * cell);
    let mut tracks: Vec<Unwrapped> = Vec::with_capacity(chains.len());
    for (ci, ch) in chains.iter().enumerate() {
        let pts = rotary_flow_unwrap::clean_chain(ch, ci, eps2)?;
        for (j, q) in pts.iter().enumerate() {
            if !rotary_flow_unwrap::on_model(*q, facets, &boxes, eps2) {
                return Err(GenError::invalid_at(
                    format!(
                        "Rotary Flow: guide chain {} point {} stands off the model surface - \
                         re-pick it, or project it onto the part",
                        ci + 1,
                        j + 1
                    ),
                    *q,
                ));
            }
            if radial_at(ax, *q).is_none() {
                return Err(GenError::invalid_at(
                    format!(
                        "Rotary Flow: guide chain {} point {} stands on the rotation axis line \
                         - it has no azimuth to unwrap",
                        ci + 1,
                        j + 1
                    ),
                    *q,
                ));
            }
        }
        tracks.push(rotary_flow_unwrap::unwrap(ax, &basis, &pts));
    }
    // The passes: one band per adjacent chain pair, hop-linked inside
    // a band, clearance-linked between bands.
    let step = match p.stepover {
        Some(s) => s,
        None => scallop_stepover(r, p.tolerance),
    };
    let tol_chord = ARC_FIT_TOL.min(p.tolerance);
    let mut emit = Emitter::new(ax, &basis, c, r, p.clearance, p.feed, p.plunge_feed);
    let mut notes: Vec<String> = Vec::new();
    let mut opened = false;
    let mut prev_last: Option<Point3> = None;
    for k in 0..tracks.len() - 1 {
        let a = &tracks[k];
        let mut b = tracks[k + 1].clone();
        if rotary_flow_unwrap::opposite_sense(a, &b) {
            return Err(GenError::Invalid(format!(
                "Rotary Flow: guide chains {} and {} advance about the axis in opposite senses \
                 - reverse one pick so both run the same way along the band",
                k + 1,
                k + 2
            )));
        }
        let shift = rotary_flow_unwrap::align_pair(a, &mut b, BAND_SAMPLES);
        let r_ref = a
            .rho
            .iter()
            .chain(b.rho.iter())
            .fold(0.0f64, |m, &x| m.max(x));
        let width = rotary_flow_unwrap::cross_width(a, &b, r_ref, BAND_SAMPLES);
        if !(width.is_finite() && width > 0.0) {
            return Err(GenError::Invalid(format!(
                "Rotary Flow: guide chains {} and {} lie on top of each other - the band has no \
                 width to machine",
                k + 1,
                k + 2
            )));
        }
        let count = pass_count(width, step)?;
        let mut band: Vec<Vec<Point3>> = Vec::with_capacity(count);
        let mut capped = false;
        for i in 0..count {
            let t = i as f64 / (count - 1) as f64;
            let out = flow_pass(ax, &basis, facets, &boxes, a, &b, t, r, tol_chord)?;
            capped |= out.depth_capped;
            let mut centers = out.centers;
            if !p.climb {
                centers.reverse();
            }
            band.push(centers);
        }
        if capped {
            notes.push(format!(
                "Rotary Flow: a pass of band {} reached the chord depth cap at tolerance \
                 {tol_chord} mm - check the picks",
                k + 1
            ));
        }
        for (i, centers) in band.iter().enumerate() {
            let Some(first) = centers.first().copied() else { continue };
            if !opened {
                emit.open(first)?;
                opened = true;
            } else if i == 0 {
                // Between bands the hop is the clearance cylinder.
                let last = prev_last.ok_or_else(|| {
                    GenError::Invalid("Rotary Flow lost the last pass's end".into())
                })?;
                emit.exit(last)?;
                emit.link(last, first, p.clearance)?;
            } else {
                let last = prev_last.ok_or_else(|| {
                    GenError::Invalid("Rotary Flow lost the last pass's end".into())
                })?;
                emit.retract(last, hop)?;
                emit.link(last, first, hop)?;
            }
            emit.pass(centers)?;
            prev_last = centers.last().copied();
        }
        let span = if shift.abs() > 1e-9 {
            format!("{shift:.2} rad of whole-turn alignment")
        } else {
            "no whole-turn alignment".into()
        };
        notes.push(format!(
            "Rotary Flow: band {} spans {width:.3} mm developed at R {r_ref:.3} mm; {count} \
             flow passes at a {step:.3} mm stepover, {span}",
            k + 1
        ));
    }
    if let Some(last) = prev_last {
        emit.exit(last)?;
    }
    let path = emit.finish(notes)?;
    // The proof over every emitted move - cuts, plunges, retracts,
    // links and the exit: safe stations do not prove a safe sweep.
    prove_path(&path, facets, &boxes, r)?;
    Ok(path)
}

/// The model, or the refusal that names its first non-finite facet:
/// the bounds and the distance oracle would read past a NaN.
fn validate_facets(facets: &[Triangle]) -> Result<(), GenError> {
    match facets
        .iter()
        .position(|f| !(f.a.is_finite() && f.b.is_finite() && f.c.is_finite()))
    {
        Some(i) => Err(GenError::Invalid(format!(
            "Rotary Flow: model facet {} carries a non-finite vertex - repair the model before \
             it is machined",
            i + 1
        ))),
        None => Ok(()),
    }
}

/// The tool and the numbers, or the refusal that names them.
fn validate_numbers(p: &RotaryFlowParams) -> Result<f64, GenError> {
    if p.tool.kind != ToolKind::BallNose {
        return Err(GenError::Invalid(format!(
            "Rotary Flow cuts with a ball-nose tool; the setup's tool is a {} - mount a \
             ball-nose tool, or finish the band with a kind that takes it",
            p.tool.kind
        )));
    }
    let r = p.tool.diameter * 0.5;
    if !(r.is_finite() && r > 0.0) {
        return Err(GenError::Invalid(format!(
            "Rotary Flow needs a finite positive ball diameter; the tool states {} mm",
            p.tool.diameter
        )));
    }
    for (name, v) in [("feed", p.feed), ("plunge feed", p.plunge_feed)] {
        if !v.is_finite() {
            return Err(GenError::Invalid(format!(
                "Rotary Flow needs a finite {name}, not {v}"
            )));
        }
    }
    if !(p.clearance.is_finite() && p.retract.is_none_or(f64::is_finite)) {
        return Err(GenError::Invalid(
            "Rotary Flow needs finite clearance and retract radii".into(),
        ));
    }
    if let Some(s) = p.stepover
        && !(s.is_finite() && s > 0.0)
    {
        return Err(GenError::Invalid(format!(
            "Rotary Flow needs a positive finite stepover, not {s} mm"
        )));
    }
    if !(p.tolerance.is_finite() && p.tolerance > 0.0) {
        return Err(GenError::Invalid(format!(
            "Rotary Flow needs a positive finite tolerance, not {} mm",
            p.tolerance
        )));
    }
    // The scallop ceiling, the finishing kinds' own law (params.rs): a
    // stepover a ball diameter apart leaves uncut ground, and the auto
    // law reaches a full diameter at half the ball radius.
    if let Some(s) = p.stepover
        && s >= p.tool.diameter
    {
        return Err(GenError::Invalid(format!(
            "Rotary Flow stepover {s} mm reaches the scallop ceiling: passes a ball diameter \
             ({} mm) apart leave uncut ground, not a cusp",
            p.tool.diameter
        )));
    }
    if p.tolerance >= r / 2.0 {
        return Err(GenError::Invalid(format!(
            "Rotary Flow tolerance {} mm reaches the scallop ceiling: at half the ball radius \
             ({:.3} mm) the scallop stepover is a full ball diameter",
            p.tolerance,
            r / 2.0
        )));
    }
    Ok(r)
}

/// The stated axis, or the refusal that names what is wrong with it.
fn validate_axis(ax: &Axis) -> Result<(), GenError> {
    if !ax.origin.is_finite() || !ax.dir.is_finite() {
        return Err(GenError::Invalid(
            "the stated rotation axis carries a non-finite point or direction".into(),
        ));
    }
    let l = ax.dir.dot(ax.dir).sqrt();
    if (l - 1.0).abs() > 1e-6 {
        return Err(GenError::Invalid(format!(
            "the stated rotation axis direction is not unit length ({l:.6}) - the stated-axis \
             resolver or the caller must normalize it"
        )));
    }
    Ok(())
}

/// The pass count a band's developed width needs at `step`: one pass
/// at each edge, one per `step` in between; the ceiling is the
/// runaway guard, refused by name.
fn pass_count(width: f64, step: f64) -> Result<usize, GenError> {
    if !(step.is_finite() && step > 0.0) {
        return Err(GenError::Invalid(
            "Rotary Flow: the scallop law collapsed to a zero stepover - check the tolerance"
                .into(),
        ));
    }
    // CEILING: the last span may be short, never long - a pass a
    // stepover and a half wide is a broken promise.
    let spans = (width / step).ceil();
    if !spans.is_finite() || spans < 0.0 {
        return Err(GenError::Invalid(format!(
            "Rotary Flow: the band's developed width {width:.3} mm does not divide at a \
             {step:.3} mm stepover"
        )));
    }
    if spans + 1.0 > MAX_PASSES as f64 {
        return Err(GenError::Invalid(format!(
            "Rotary Flow: the band needs {} passes at a {step:.3} mm stepover - more than the \
             {MAX_PASSES} pass ceiling; widen the stepover, or pick a narrower band",
            spans + 1.0
        )));
    }
    Ok((spans as usize + 1).max(2))
}
