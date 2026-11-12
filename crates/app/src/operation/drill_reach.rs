//! Drill reach (issue #84): a drill may go no deeper into material than
//! its usable flute, measured from the STOCK surface at the hole - past
//! the flutes the shank (no narrower than the drill on every catalog
//! drill) feeds into uncut stock, a collision - and its holder must
//! stay clear of the stock along every move. Both laws refuse by name,
//! before the path can reach the post.
//!
//! The tool stack read is the catalog's own: `flute_len` from the tip,
//! then `shank_len` of shank, then the holder (`holder_d` wide,
//! `holder_len` long) - so the holder's face stands `flute_len +
//! shank_len` behind the tip. A tool with no holder modelled
//! (`holder_d` or `holder_len` zero, the turret-held kinds) has nothing
//! for the holder law to sweep (a non-finite or negative holder field
//! is refused by name, never read as "no holder"). The rule is the
//! conservative one: a necked tool (shank narrower than the drill)
//! would let the shank in further, but nothing here proves its wall
//! clearance, so it is held to its flute like the rest. Tap, bore and
//! back-bore cycles run in a hole that is already there: they are held
//! not to the flute but to the shank fitting that hole.
//!
//! Issue #842: with a modelled tool body (#368 mesh, #93 drawn profile)
//! the holder laws read the body's band stack - the one the simulation
//! draws and the other stack laws read (`verify::BodyBands`) - each
//! band's face disc (axial) or swept footprint (cross) against the
//! material over that band's own radius, not the one cylinder enclosing
//! the body. No body, or a stack that does not fit the record, reads the
//! holder cylinder exactly as before.
//!
//! Issue #89: the axial laws read the stock as the setup's earlier
//! operations left it (the re-machining prior grid, read under each
//! disk at its highest covered cell; unknown = raw). The cross holes'
//! laws keep the raw stock: the column grid stands along Z and cannot
//! be read along a radial hole line.
//!
//! Provenance: usable flute length versus hole depth, and holder
//! clearance over the work, are the machine-tool handbooks' drilling
//! practice; the swept-rectangle distance is textbook planar geometry
//! (convex hull, point-to-segment). Our own design notes; no vendor
//! material consulted.

use crate::cl::{Cycle, CycleKind, Toolpath};
use crate::drilling::{CrossHole, CycleChoice, Hole, SPOT_DEPTH, THROUGH_CLEAR};
use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::tools::Tool;
use crate::verify::BodyBands;
use camcraft_tools::holder_profile::HolderBand;

#[path = "drill_station.rs"]
mod station;
#[path = "drill_top.rs"]
mod top;
#[path = "drill_hull.rs"]
mod hull;
#[cfg(test)]
use hull::hull_dist;
use hull::{holder_approach, seg_dist};
#[cfg(test)]
use top::{facet_top, meets_disk};
use top::{r_plane_clear, top_over};

const TOL: f64 = 1e-6;

fn finite_or_zero(v: f64) -> f64 {
    if v.is_finite() { v.max(0.0) } else { 0.0 }
}

/// The tip-to-holder-face distance and the holder's radius, or `None`
/// when no holder is modelled (`holder_d` or `holder_len` zero). A
/// non-finite or negative holder field is refused by name: reading a
/// NaN as "no holder" would wave the holder law through.
fn holder(tool: &Tool, name: &str) -> Result<Option<(f64, f64)>, GenError> {
    let (hd, hl) = (tool.holder_d, tool.holder_len);
    if !(hd.is_finite() && hl.is_finite() && hd >= 0.0 && hl >= 0.0) {
        return Err(GenError::Invalid(format!(
            "tool {name}'s holder (dia. {hd}, length {hl}) is not a usable size - its clearance cannot be \
             proven; set both to real sizes, or both to 0 for a tool with no holder modelled"
        )));
    }
    Ok((hd > 0.0 && hl > 0.0).then(|| (finite_or_zero(tool.flute_len) + finite_or_zero(tool.shank_len), 0.5 * hd)))
}

/// One band of the tool's stack above the shank: its face `above` the
/// tip, its radius, its length.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Band {
    above: f64,
    r: f64,
    len: f64,
}

/// What the holder laws sweep above the shank (#842), and the word a
/// refusal names it by: a modelled body's band stack (`body`, the bands
/// the simulation draws and the other stack laws read - each never
/// smaller than the body over its span), or the one holder cylinder.
/// No holder modelled sweeps nothing. A carried stack that is empty,
/// non-finite or reaches past the record's enclosing holder is not
/// trusted: the cylinder stands in, which can only over-report.
fn stack_bands(tool: &Tool, body: Option<&BodyBands>, name: &str) -> Result<(Vec<Band>, &'static str), GenError> {
    let Some((reach, hr)) = holder(tool, name)? else { return Ok((Vec::new(), "holder")) };
    let cyl = Band { above: reach, r: hr, len: finite_or_zero(tool.holder_len) };
    let wide = hr.max(0.5 * finite_or_zero(tool.shank_d)) + TOL;
    let ok = |b: &HolderBand| {
        [b.above, b.r, b.len].iter().all(|v| v.is_finite() && *v >= 0.0) && b.above >= reach - TOL && b.r <= wide
    };
    match body.map(BodyBands::bands) {
        Some(bs) if !bs.is_empty() && bs.iter().all(ok) => {
            Ok((bs.iter().map(|b| Band { above: b.above, r: b.r, len: b.len }).collect(), "body band"))
        }
        _ => Ok((vec![cyl], "holder")),
    }
}

/// `at` is the hole's mouth, the spot the refusal marks (#677).
fn too_deep(what: &str, at: Point3, depth: f64, name: &str, flute: f64, fix: &str) -> GenError {
    GenError::invalid_at(
        format!(
            "{what} is {depth:.3} mm deep from the stock surface; tool {name}'s flute reaches {flute:.3} mm - \
             past it the shank feeds into uncut stock (a collision); {fix}"
        ),
        at,
    )
}

/// The cross holes' reach (spindle frame, mouths already on the raw
/// surface when the stock is wider than the part): the depth the kernel
/// will feed - the floor, plus the through-exit allowance, or the spot
/// dimple - against the flute. `world` names each hole as modelled.
/// `extra` is, per hole, how much further out than the mouth a shank
/// wider than the drill first meets the raw stock (`to_raw_mouths`):
/// the reach is the wider body's, the mouth stays the drill's.
pub(super) fn cross_reach(
    local: &[CrossHole],
    extra: &[f64],
    world: &[CrossHole],
    cycle: CycleChoice,
    tool: &Tool,
    name: &str,
) -> Result<(), GenError> {
    let flute = finite_or_zero(tool.flute_len);
    for (i, (h, w)) in local.iter().zip(world).enumerate() {
        let floor = h.floor_d - if h.through { THROUGH_CLEAR } else { 0.0 };
        let body = extra.get(i).copied().unwrap_or(0.0);
        let depth = body + if matches!(cycle, CycleChoice::Spot) { SPOT_DEPTH.min(-h.floor_d) } else { -floor };
        if depth.is_nan() || depth > flute + TOL {
            let o = w.origin;
            return Err(too_deep(
                &format!("cross hole {i} at ({:.3}, {:.3}, {:.3})", o.x, o.y, o.z),
                o,
                depth,
                name,
                flute,
                "drill it with a longer drill, or turn the bar to size first",
            ));
        }
    }
    Ok(())
}

/// The axial (Z) holes' reach and holder, read off the cycle records the
/// kernel emitted: from the material top over the drill's (or a wider
/// shank's) disk down to the cycle's bottom against the flute, and the
/// holder's face - `flute_len + shank_len` above the bottom - against
/// the material top over the holder's disk. A tap, bore or back-bore
/// cycle runs in an existing hole (`holes`, the list the kernel cut):
/// past its flute only the shank must fit that hole's diameter.
///
/// `prior` (issue #89) is the grid the setup's earlier operations left
/// standing: over each disk the stock and the hole's mouth stand no
/// higher than its highest covered cell ([`prior_surface`]); `None`,
/// or a disk it cannot read, is the raw stock; a lathe's axial hole
/// reads it revolved about the axis ([`super::drill_prior::PriorRead`]).
/// `body` is the tool's modelled band stack (#842, `Cutter::body`), the
/// holder face law's reading where it is given.
pub(super) fn axial_reach(
    t: &Toolpath,
    holes: &[Hole],
    part: &[Triangle],
    stock: Option<&[Triangle]>,
    prior: Option<super::drill_prior::PriorRead<'_>>,
    (tool, body): (&Tool, Option<&BodyBands>),
    name: &str,
) -> Result<(), GenError> {
    // No stock (or the stock is the source): the part alone is the
    // material - every law below reads the part too (#91 review).
    let stock = stock.unwrap_or(&[]);
    let flute = finite_or_zero(tool.flute_len);
    let (bands, what) = stack_bands(tool, body, name)?;
    let body = 0.5 * finite_or_zero(tool.diameter).max(finite_or_zero(tool.shank_d));
    for c in &t.cycles {
        let Cycle { xy, r_z, bottom_z, .. } = *c;
        let q = Point2::new(xy.x, xy.y);
        let prior_at = |r: f64| prior.and_then(|p| p.surface(q, r));
        let pb = prior_at(body);
        r_plane_clear(part, stock, c, body, pb)?;
        let entry = top::cut_by(Some(c.entry_z), pb).unwrap_or(c.entry_z);
        let top = top_over(part, stock, xy.x, xy.y, body, pb).unwrap_or(entry).max(entry.min(r_z));
        let depth = top - bottom_z;
        if matches!(c.kind, CycleKind::Tap | CycleKind::FloatingTap | CycleKind::Bore | CycleKind::BackBore) {
            shank_fits(c, holes, top, depth, flute, tool, name)?;
        } else if depth.is_nan() || depth > flute + TOL {
            return Err(too_deep(
                &format!("hole at ({:.3}, {:.3})", xy.x, xy.y),
                Point3::new(xy.x, xy.y, top),
                depth,
                name,
                flute,
                "drill it with a longer drill, or in stages from both sides",
            ));
        }
        // #842: each band's face disc against the material over that
        // band's own radius - a wide flange high on the body is judged
        // where it stands, not at the shank's top.
        for &Band { above: reach, r: hr, .. } in &bands {
            let face = bottom_z + reach;
            if let Some(ht) = top_over(part, stock, xy.x, xy.y, hr, prior_at(hr)).filter(|&z| face < z - TOL) {
                return Err(GenError::invalid_at(
                    format!(
                        "tool {name}'s {what} (dia. {:.3}, {reach:.3} mm above the tip) sinks to Z {face:.3} at the \
                         hole at ({:.3}, {:.3}), below the material top {ht:.3} around it - the holder feeds into \
                         the stock (a collision); use a longer tool or a slimmer holder",
                        2.0 * hr,
                        xy.x,
                        xy.y
                    ),
                    Point3::new(xy.x, xy.y, top),
                ));
            }
        }
    }
    Ok(())
}

/// A tap, bore or back-bore cycle `c`, `depth` deep: within the flute
/// only the cutting end is in the hole; past it the shank (the only
/// body between flutes and holder) must fit the hole's diameter, or it
/// is refused by name.
fn shank_fits(c: &Cycle, holes: &[Hole], top: f64, depth: f64, flute: f64, tool: &Tool, name: &str) -> Result<(), GenError> {
    if depth.is_finite() && depth <= flute + TOL {
        return Ok(());
    }
    let at = |h: &&Hole| (h.xy.x - c.xy.x).abs() <= TOL && (h.xy.y - c.xy.y).abs() <= TOL;
    let hole_d = holes.iter().find(at).map(|h| 2.0 * h.radius);
    let shank_d = tool.shank_d;
    match hole_d {
        Some(hd) if depth.is_finite() && shank_d.is_finite() && shank_d <= hd + TOL => Ok(()),
        _ => Err(GenError::invalid_at(
            format!(
                "{:?} cycle at ({:.3}, {:.3}) is {depth:.3} mm deep from the stock surface, past tool {name}'s \
                 {flute:.3} mm flute, and its shank dia. {shank_d:.3} does not fit the dia. {} hole - the shank \
                 feeds into the wall (a collision); use a tool with a longer flute or a slimmer shank",
                c.kind,
                c.xy.x,
                c.xy.y,
                hole_d.map_or_else(|| "(unknown)".to_string(), |d| format!("{d:.3}"))
            ),
            Point3::new(c.xy.x, c.xy.y, top),
        )),
    }
}

/// The widest material radius about the spindle origin `o` within the
/// Z band `z0..=z1`: each facet clipped to the band, read at the
/// clipped polygon's corners (the radius is convex, so no point inside
/// a facet stands further out than its corners). `None` when nothing
/// is there.
fn band_r(facets: &[Triangle], o: (f64, f64), z0: f64, z1: f64) -> Option<f64> {
    let rad = |x: f64, y: f64| (x - o.0).hypot(y - o.1);
    let mut best: Option<f64> = None;
    for t in facets {
        let v = [t.a, t.b, t.c];
        if v.iter().any(|p| !p.is_finite()) {
            continue;
        }
        for i in 0..3 {
            let (p, q) = (v[i], v[(i + 1) % 3]);
            let mut take = |r: f64| best = Some(best.map_or(r, |b| b.max(r)));
            if p.z >= z0 && p.z <= z1 {
                take(rad(p.x, p.y));
            }
            for zp in [z0, z1] {
                if (p.z - zp) * (q.z - zp) < 0.0 {
                    let s = (zp - p.z) / (q.z - p.z);
                    take(rad(p.x + s * (q.x - p.x), p.y + s * (q.y - p.y)));
                }
            }
        }
    }
    best
}

/// Every move of the cross-drill chain (spindle frame) proven to keep
/// the holder clear of the material: the holder is a cylinder of
/// radius `hr` whose face stands `reach` behind the tip along the
/// move's tool axis. Per move, the Z band the holder occupies (both
/// ends of the move, both ends of the holder, its radius across the
/// axis) is measured against the widest part or stock (world facets,
/// spindle origin `origin`) within that band - so a holder riding
/// above the end face meets nothing, and a flange far away in Z does
/// not refuse a hole in a slim section. The XY rectangle the holder's
/// footprint sweeps must stay outside that radius. Anything else is
/// refused by name - the swing is not raised to make room (a holder
/// that wide wants a clearance the user sets, not one the kernel
/// invents). Issue #119: that corner-radius reading is kept for the
/// swings round the bar; a move on one station (along its hole line,
/// across it, or in Z) reads the material in the tube its holder sweeps
/// (`station`), so a box is met at its face, not its corner circle.
pub(super) fn cross_holder(
    t: &Toolpath,
    part: &[Triangle],
    stock: Option<&[Triangle]>,
    origin: (f64, f64),
    tool: &Tool,
    body: Option<&BodyBands>,
    name: &str,
) -> Result<(), GenError> {
    // #842: a body's bands each swept at their own face and radius (the
    // stack the other laws read); no body, the one holder cylinder.
    let (bands, what) = stack_bands(tool, body, name)?;
    if bands.is_empty() {
        return Ok(());
    }
    let mat = station::material(part, stock, origin);
    let cx = Swept { part, stock, origin, mat: &mat, name, what };
    for (i, m) in t.moves.iter().enumerate() {
        for b in &bands {
            band_clear(&cx, i, m, *b)?;
        }
    }
    Ok(())
}

/// What one cross-drill band is swept against, and how it is named.
struct Swept<'a> {
    part: &'a [Triangle],
    stock: Option<&'a [Triangle]>,
    origin: (f64, f64),
    mat: &'a station::Material,
    name: &'a str,
    what: &'a str,
}

/// [`cross_holder`]'s law for move `i` and one band `b` of the stack.
fn band_clear(cx: &Swept<'_>, i: usize, m: &crate::cl::Move, b: Band) -> Result<(), GenError> {
    let Band { above: reach, r: hr, len: hl } = b;
    let (name, what) = (cx.name, cx.what);
    let axis = m.axis.unwrap_or([0.0, 0.0, 1.0]);
    let az = axis[2].clamp(-1.0, 1.0);
    let across = hr * (1.0 - az * az).max(0.0).sqrt();
    let zs = [m.from.z, m.to.z].into_iter().flat_map(|z| [z + reach * az, z + (reach + hl) * az]);
    let (lo, hi) = zs.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), z| (lo.min(z), hi.max(z)));
    let (z0, z1) = (lo - across, hi + across);
    if !(z0.is_finite() && z1.is_finite()) {
        return Err(GenError::Invalid(format!(
            "cross-drill move {i}: tool {name}'s {what} position is not finite - its clearance cannot be proven"
        )));
    }
    if let Some((tube, face)) = station::on_station(m.from, m.to, axis, reach, hr) {
        let Some(need) = station::reach_in(cx.mat, tube) else { return Ok(()) };
        if face.is_nan() || need.is_nan() || face < need - TOL {
            let (zl, zh) = (tube.line.z - tube.hz, tube.line.z + tube.hz);
            return Err(GenError::Invalid(format!(
                "cross-drill move {i}: tool {name}'s {what} (dia. {:.3}, {reach:.3} mm behind the tip) stands \
                 {face:.3} mm out along its hole's line, inside material {need:.3} mm out on that line at Z \
                 {zl:.3} to {zh:.3} - the holder feeds into the stock or part (a collision); use a tool with a \
                 longer reach, or turn or face the material back first",
                2.0 * hr
            )));
        }
        return Ok(());
    }
    let facets = [Some(cx.part), cx.stock].into_iter().flatten();
    let Some(wide) = facets.filter_map(|f| band_r(f, cx.origin, z0, z1)).reduce(f64::max) else { return Ok(()) };
    let got = holder_approach(m.from, m.to, axis, reach, hl, hr);
    if got.is_nan() || got < wide - TOL {
        return Err(GenError::Invalid(format!(
            "cross-drill move {i}: tool {name}'s {what} (dia. {:.3}, {reach:.3} mm behind the tip) passes \
             {got:.3} mm from the spindle, inside the stock's radius {wide:.3} at Z {z0:.3} to {z1:.3} - \
             the holder sweeps the stock (a collision); use a longer tool, raise the clearance above the \
             end face, or turn the bar to size first",
            2.0 * hr
        )));
    }
    Ok(())
}

#[cfg(test)]
#[path = "drill_reach_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "drill_reach_body_tests.rs"]
mod body_tests;
