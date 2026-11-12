//! The Probe operation's generation and its protected-move law (#108):
//! op params to [`crate::probing`]'s cycle, then every stylus move
//! judged against the model - a ball that comes nearer than its radius
//! to the part anywhere before (or after) the expected contact is a
//! stylus driven through material, refused as a gouge. A probe removes
//! no stock: the sim never carves it (`stock_sim` skips probe paths).
//! Provenance: NIST RS274NGC G38.2 semantics and touch-probe practice;
//! the swept-ball distance is Ericson's segment-triangle test (the
//! sim's `dist_seg_tri`). #842: a modelled tool body is judged by its
//! band stack above the stylus (`stylus_cutter`). Our own design; no
//! vendor material consulted.

use crate::cl::Toolpath;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::probing::{probe_cycle, ProbeParams};
use crate::tools::{Tool, ToolGeom, ToolKind};
use crate::verify::{solid_hits, BodyBands, Cutter, SolidLaw, Warning};
use camcraft_tools::holder_profile::{HolderBand, HolderStack};

use super::types::OpParams;

/// How much nearer than its radius the ball may come to the model
/// before the law convicts: the touch itself is tangent (exactly r),
/// so this is the facet lattice's numeric slack, not a cut allowance.
pub const STYLUS_SLACK: f64 = 0.01;

/// The deepest search past the expected contact a stylus is trusted to
/// take without a word (#570). Touch-probe styli are rated for a few
/// millimetres of overtravel at most; past this, a missing feature drives
/// the stylus toward the end of its travel before the control alarms.
pub const SEARCH_LIMIT_MM: f64 = 2.0;

/// A new probe op's search limit for a `tolerance` (#570): twice the
/// tolerance, never under 1 mm - far enough that a feature inside the
/// tolerance always trips, short enough that a missing one alarms early.
#[must_use]
pub fn default_overtravel(tolerance: f64) -> f64 {
    (2.0 * tolerance).max(1.0)
}

/// The advisory for a search limit past [`SEARCH_LIMIT_MM`]: one, at the
/// first seek, naming the distance - never blocking.
#[must_use]
pub fn search_advisory(path: &Toolpath, overtravel: f64) -> Option<Warning> {
    if overtravel <= SEARCH_LIMIT_MM {
        return None;
    }
    let at = path.probes.first().map_or(0, |t| t.index);
    Some(Warning::ProbeSearch { at, mm: overtravel, limit_mm: SEARCH_LIMIT_MM })
}

/// The cycle's numbers from the op's, with the resolved safe plane.
pub(super) fn probe_params(params: &OpParams, tool: &Tool, clearance_z: f64) -> Result<ProbeParams, GenError> {
    let OpParams::Probe {
        mode,
        dir,
        dir_y,
        target,
        radius,
        approach,
        overtravel,
        backoff,
        fast_feed,
        slow_feed,
        depth,
        inset,
        tolerance,
        set_wcs,
        ..
    } = params
    else {
        return Err(GenError::Invalid("not a probe operation".into()));
    };
    if tool.kind != ToolKind::Probe {
        return Err(GenError::Invalid(format!(
            "a probe operation mounts a touch probe, not a {}",
            tool.kind
        )));
    }
    if !(tolerance.is_finite() && *tolerance > 0.0) {
        return Err(GenError::Invalid("the probe tolerance must be positive".into()));
    }
    if set_wcs.is_some_and(|s| s > 5) {
        return Err(GenError::Invalid("the work offset to set must be one of G54..G59".into()));
    }
    Ok(ProbeParams {
        mode: *mode,
        dir: *dir,
        dir_y: *dir_y,
        target: Point3::new(target[0], target[1], target[2]),
        radius: *radius,
        ball_diameter: tool.diameter,
        approach: *approach,
        overtravel: *overtravel,
        backoff: *backoff,
        fast_feed: *fast_feed,
        slow_feed: *slow_feed,
        depth: *depth,
        inset: *inset,
        clearance_z,
    })
}

/// Generate a probe op's path and judge it against `facets` (the model).
/// `stack` is the tool's modelled body (#842), `None` without one.
pub(super) fn generate_probe(
    params: &OpParams,
    tool: &Tool,
    stack: Option<&HolderStack>,
    clearance_z: f64,
    facets: &[Triangle],
) -> Result<(Toolpath, Vec<Warning>), GenError> {
    let p = probe_params(params, tool, clearance_z)?;
    let path = probe_cycle(&p)?;
    let mut warns = stylus_hits(&path, tool.diameter * 0.5, facets);
    warns.extend(stylus_reach_hits(&path, tool, stack, facets, &warns));
    warns.extend(search_advisory(&path, p.overtravel));
    Ok((path, warns))
}

/// The protected-move law: every move of the stylus, the touches
/// included, keeps the ball at least its radius (less the slack) off
/// every facet. The moves state the TIP (`probing::probe_cycle`), so
/// the swept ball's centre is each point raised by the radius. One
/// warning per offending move.
#[must_use]
pub fn stylus_hits(path: &Toolpath, ball_r: f64, facets: &[Triangle]) -> Vec<Warning> {
    let floor = ball_r - STYLUS_SLACK;
    let centre = |q: Point3| Point3::new(q.x, q.y, q.z + ball_r);
    path.moves
        .iter()
        .enumerate()
        .filter(|(_, m)| {
            facets
                .iter()
                .any(|t| crate::stock_sim::dist_seg_tri(centre(m.from), centre(m.to), t) < floor)
        })
        .map(|(at, _)| Warning::Gouge { at })
        .collect()
}

/// The probe stack as the solid law reads it (#172): the ball is the
/// whole cutting atom (`flute_len` = its radius), the stylus stem above
/// it the shank band (the ball's radius - `Tool` states no thinner stem,
/// so never smaller than the real one) up to the stylus length, and the
/// probe body with whatever holder rides above it one band, at the wider
/// of the two radii over both lengths - never smaller than the real pair.
///
/// #842: with a modelled body (`stack`, `tool_body::stack` of the tool,
/// `Some` only when the tool carries one) that one band is rebuilt as the
/// probe body (the record's shank band) and the body's own bands above
/// it - the shape the simulation draws. A stack that does not fit the
/// stylus ([`BodyBands::of`]) leaves the one enclosing band, which can
/// only over-report; `None` reads exactly as before.
#[must_use]
pub fn stylus_cutter(tool: &Tool, stack: Option<&HolderStack>) -> Cutter {
    let r = tool.diameter * 0.5;
    let g = tool.geom();
    let body_len = g.shank_len.max(0.0) + g.holder_len.max(0.0);
    let body_r = if g.holder_len > 0.0 { g.shank_radius.max(g.holder_radius) } else { g.shank_radius };
    let c = Cutter {
        radius: r,
        corner_r: r,
        tip_angle: 0.0,
        flute_len: r,
        shank_radius: r,
        shank_len: (g.flute_len - r).max(0.0),
        holder_radius: body_r,
        holder_len: body_len,
        body: None,
        neck: None,
    };
    Cutter { body: stack.and_then(|s| stylus_body(&g, &c, s)), ..c }
}

/// The stylus's bands above its stem (#842): the probe body over the
/// record's shank, then the modelled body's bands - checked against the
/// stylus stack `c` describes ([`BodyBands::of`]), or `None` (the one
/// enclosing band stands). The tool's own holder cylinder is no body.
fn stylus_body(g: &ToolGeom, c: &Cutter, stack: &HolderStack) -> Option<BodyBands> {
    let cyl = HolderStack::cylinder(g);
    if stack.bands == cyl.bands && stack.top == cyl.top {
        return None;
    }
    let sg = ToolGeom {
        radius: c.radius,
        nose_radius: 0.0,
        flute_len: c.flute_len,
        shank_radius: c.shank_radius,
        shank_len: c.shank_len,
        holder_radius: c.holder_radius,
        holder_len: c.holder_len,
    };
    let mut bands = Vec::with_capacity(stack.bands.len() + 1);
    if g.shank_len > 0.0 {
        bands.push(HolderBand { above: g.flute_len, len: g.shank_len, r: g.shank_radius });
    }
    bands.extend_from_slice(&stack.bands);
    BodyBands::of(&sg, &HolderStack { bands, top: stack.top, neck: Vec::new() })
}

/// The reach half of the protected-move law (#172): every move swept
/// with the stem and body bands against the model's solid
/// ([`solid_hits`]); `stack` is a modelled body's, as
/// [`stylus_cutter`] reads it. The ball's own verdict is [`stylus_hits`]'s
/// (`ball` holds it), so a move it convicted is not reported twice;
/// what the bands meet is a [`Warning::ShankHit`] on the part, a rapid
/// that meets anything a [`Warning::RapidInStock`]. The expected
/// contact excuses nothing here: only the ball ever touches, so a stem
/// or body meeting the part at any point of any move is refused. A
/// ball conviction the distance law missed (a ball wholly inside the
/// solid) is kept as the gouge it is.
#[must_use]
pub fn stylus_reach_hits(
    path: &Toolpath,
    tool: &Tool,
    stack: Option<&HolderStack>,
    facets: &[Triangle],
    ball: &[Warning],
) -> Vec<Warning> {
    let law = SolidLaw { step: 0.5, allow: STYLUS_SLACK, hole_walls: false };
    let convicted = |at: usize| ball.iter().any(|w| matches!(*w, Warning::Gouge { at: b } if b == at));
    solid_hits(path, facets, &stylus_cutter(tool, stack), &law)
        .into_iter()
        .filter(|w| match *w {
            Warning::Gouge { at } | Warning::RapidInStock { at } | Warning::ShankHit { at, .. } => !convicted(at),
            _ => true,
        })
        .collect()
}
