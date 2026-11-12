//! The material an operation machines: the world-transformed source
//! facets and their top (`material_of`), the hole lists read from them,
//! the section/stadium builders and the shared height refusals. The
//! region builds live in `material_region` and `material_source`, split
//! along the domain seam (400-line law) and re-exported here.
//!
//! Provenance: our own domain model - specced from public documentation of
//! commercial CAM workflows; no vendor source was consulted.

use super::*;

pub use super::material_region::*;
pub(super) use super::material_source::*;

/// The height expressions of a params set (the `for_each_height` soup
/// collected once, for scanning).
pub(super) fn collect_heights(params: &OpParams) -> Vec<Height> {
    let mut out = Vec::new();
    params.for_each_height(|h| out.push(h));
    out
}

/// Gouge law's radial slack: half the geo2d round-join chord error plus
/// room for float chains; below this, a "bite" is an artifact, above it a
/// real overrun of the finish surface (see `tests/verify.rs` rationale).
pub(crate) const GOUGE_TOL: f64 = 0.01;
/// "Below the clearance plane" / "moved in XY" margin for the rapid law.
pub(super) const Z_TOL: f64 = 1e-6;
/// Z samples for a silhouette footprint; convergence needs far fewer.
pub(super) const FOOTPRINT_SAMPLES: usize = 16;
/// One micron under a grazing section plane, for the slice retry.
pub(super) const SLICE_NUDGE: f64 = 1e-3;
/// How far a slot cap's polygon may stand OUTSIDE the true arc, mm
/// (#510). The verifier's slack for a real over-cut is this plus
/// `GOUGE_TOL`, so it is held a decade under the tolerance.
pub(super) const CAP_SAG_BUDGET: f64 = 0.001;
/// Floor and ceiling of the chords per semicircular slot cap: the floor
/// keeps a tiny cap a real polygon, the ceiling keeps a huge one cheap.
pub(super) const CAP_MIN_SEGMENTS: usize = 16;
pub(super) const CAP_MAX_SEGMENTS: usize = 2048;

/// The source geometry in world coordinates, plus its top.
pub(super) struct Material {
    pub(super) facets: Vec<Triangle>,
    pub(super) top: f64,
    /// The per-entity index ranges of `facets` when the source is a
    /// selection across several models (M13.11): the depth law and the
    /// verify read a plane as the 2D union of the bodies' own sections,
    /// so two models butted face to face are one solid, not a
    /// non-manifold refusal. Empty for one body - the plain read.
    pub(super) bodies: Vec<std::ops::Range<usize>>,
    /// How much coarser than the importer's promise the source mesh is,
    /// mm (#864, `model_fidelity`): the worst of the entities read. 0 for
    /// every model that met it - the gouge laws then read as always.
    pub(super) excess: f64,
}

/// The world facets an operation's generator stands in - exactly
/// `material_of`'s read, for a consumer outside the generator that
/// must re-derive what the generator derived from them (the stock
/// sim's lathe frame, #19: the turning kernels fit their axis here).
pub(crate) fn source_facets(world: &World, setup: Option<&str>, src: &SrcRef) -> Result<Vec<Triangle>, String> {
    material_of(world, setup, src).map(|m| m.facets).map_err(|e| e.to_string())
}

pub(super) fn material_of(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
) -> Result<Material, GenError> {
    match src {
        SrcRef::Stock => {
            let name = crate::setup::stock_entity_name(world, setup);
            named_material(world, setup, &name, "no stock: set it up in Setup first")
        }
        SrcRef::Entity(name) => named_material(world, setup, name, &format!("no entity named '{name}'")),
        SrcRef::Selection(items) => {
            // Union of the distinct selected entities (stock is a legal
            // pick): what the generators stand in and what verification
            // sweeps. The region itself comes from the features.
            let mut facets = Vec::new();
            let mut top = f64::NEG_INFINITY;
            let mut seen: Vec<&str> = Vec::new();
            let mut bodies = Vec::new();
            let mut excess = 0.0f64;
            for g in items {
                if seen.contains(&g.entity.as_str()) {
                    continue;
                }
                seen.push(g.entity.as_str());
                let m = named_material(
                    world,
                    setup,
                    &g.entity,
                    &format!("the selection names '{}', which is gone", g.entity),
                )?;
                excess = excess.max(m.excess);
                let start = facets.len();
                facets.extend(m.facets);
                if facets.len() > start {
                    bodies.push(start..facets.len());
                }
                top = top.max(m.top);
            }
            let mut m = usable(facets, top, "the selection")?;
            m.excess = excess;
            // Two or more bodies: each is read as its own solid and the
            // planes are united; one body keeps the plain read.
            if bodies.len() > 1 {
                m.bodies = bodies;
            }
            Ok(m)
        }
    }
}

/// [`material_of`] as generation judges it (#904): the excess also
/// covers the setup's stock when the source is not the stock itself.
/// The collision laws (rapids in stock, the rotary bands, the turning
/// margin) read clearance against that stock, so a stock copied from a
/// coarser model than the part must tighten them too. One number for
/// every law - the worst of the two - so it only ever tightens, and the
/// program gate reads the same pair (`model_fidelity::op_problem`).
pub(super) fn judged_material(world: &World, setup: Option<&str>, src: &SrcRef) -> Result<Material, GenError> {
    let mut m = material_of(world, setup, src)?;
    if !matches!(src, SrcRef::Stock) {
        let stock = crate::setup::stock_entity_name(world, setup);
        m.excess = m.excess.max(crate::model_fidelity::excess_of(world, &stock));
    }
    Ok(m)
}

/// The hole list of one source, for the panel's Detect holes button:
/// the SAME world-transformed facets generation machines, so what is
/// detected is what will be drilled - a local-frame list would drift
/// from a moved part. A pure read; the caller (the UI, through a
/// command) decides what to do with the list.
pub fn holes_of(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
) -> Result<Vec<crate::drilling::Hole>, GenError> {
    let m = material_of(world, setup, src)?;
    Ok(crate::drilling::recognize(&m.facets))
}

/// The radial hole list of one source, for Detect holes on a document
/// that holds a turret (M5.2 F2): the same world-transformed facets,
/// read a second way - closed cylindrical walls whose AXIS runs at
/// the spindle, found by the same weld law the Z pass runs.
pub fn cross_holes_of(
    world: &World,
    setup: Option<&str>,
    src: &SrcRef,
) -> Result<Vec<crate::drilling::CrossHole>, GenError> {
    let m = material_of(world, setup, src)?;
    Ok(crate::drilling::recognize_cross(&m.facets))
}

/// Detect holes (M2.4 C; shared with the MCP server by #312): `params`
/// with its source's holes, read NOW, written into whichever hole list
/// the kind has - the drill, the boring ride and the thread mill store
/// the same `holes` shape - plus the drill's radial list when the
/// document's machine holds a turret (M5.2 F2; a plain mill asks nothing
/// of the mesh's sides). `name` only words the refusals. The caller
/// commits the result as one undoable `EditOperationParams`, so the
/// app's Detect holes button and the MCP `detect_holes` tool store the
/// same list.
pub fn detect_holes(
    world: &World,
    name: &str,
    params: &OpParams,
    setup: Option<&str>,
    src: &SrcRef,
) -> Result<OpParams, String> {
    let mut params = params.clone();
    let is_drill = matches!(params, OpParams::Drill { .. });
    let holes: &mut Vec<_> = match &mut params {
        OpParams::Drill { holes, .. } | OpParams::Bore { holes, .. } | OpParams::Thread { holes, .. } => holes,
        _ => return Err(format!("'{name}' is not a drilling operation")),
    };
    let axial = holes_of(world, setup, src).map_err(|e| e.to_string())?;
    // #388: the op's own setup's machine, not the active one.
    let radial = if !is_drill || crate::machine::kind_of_setup(world, setup) == crate::machine::Kind::Mill {
        Vec::new()
    } else {
        cross_holes_of(world, setup, src).map_err(|e| e.to_string())?
    };
    if axial.is_empty() && radial.is_empty() {
        return Err(if is_drill {
            "no drilled holes found in the source model - Z or radial".to_string()
        } else {
            "no Z-axis holes found in the source model - this ride reads only those".to_string()
        });
    }
    *holes = axial;
    if let OpParams::Drill { cross, .. } = &mut params {
        *cross = radial;
    }
    Ok(params)
}

/// A named entity's facets in its setup's frame (#326: a part of an
/// aligned setup reads turned; the stock and an unaligned setup read
/// exactly the world facets they always did).
pub(super) fn named_material(world: &World, setup: Option<&str>, name: &str, gone: &str) -> Result<Material, GenError> {
    let Some(entity) = model::find_by_name(world, name) else {
        return Err(GenError::Invalid(gone.to_string()));
    };
    let facets = crate::setup_align::facets_in(world, &entity, setup).unwrap_or_default();
    let top = crate::setup_align::entity_aabb_in(world, &entity, setup)
        .map(|b| b.max.z)
        .unwrap_or(f64::NAN);
    let mut m = usable(facets, top, name)?;
    m.excess = crate::model_fidelity::excess_of(world, name);
    Ok(m)
}

/// The shared sanity gate: every vertex finite, a real top.
pub(super) fn usable(facets: Vec<Triangle>, top: f64, label: &str) -> Result<Material, GenError> {
    if !facets
        .iter()
        .all(|t| t.a.is_finite() && t.b.is_finite() && t.c.is_finite())
        || !top.is_finite()
    {
        return Err(GenError::Invalid(format!(
            "'{label}' has no usable geometry to machine"
        )));
    }
    Ok(Material { facets, top, bodies: Vec::new(), excess: 0.0 })
}

pub(super) fn from_slice(e: SliceError) -> GenError {
    match e {
        SliceError::Geo(g) => GenError::Geo(g),
        other => GenError::Invalid(other.to_string()),
    }
}

/// The section at `z`, retried a micron below when the plane grazes a
/// vertex or edge (a tangential slice is an error by contract, and a
/// 1 um shift cannot change what a cutter sees) - or when it comes back
/// empty: a plane at the very top of an imported mesh whose top corners
/// scatter by float noise (#241: 3e-13 mm) meets only the highest few
/// and reads "no material" over a part that is plainly there. The
/// section is only ever taken at Top of Part Z (`section_z_of`), so an
/// empty answer names that height and the span that would answer.
pub(super) fn section(facets: &[Triangle], z: f64) -> Result<Region, GenError> {
    let first = match slice::slice(facets, z) {
        Ok(r) if !r.is_empty() => return Ok(r),
        other => other,
    };
    match (first, slice::slice(facets, z - SLICE_NUDGE)) {
        (_, Ok(r)) if !r.is_empty() => Ok(r),
        // The asked plane was clean, or the nudged one was: nothing is
        // there - say where something is.
        (Ok(_), _) | (Err(_), Ok(_)) => Err(empty_at(facets, z)),
        // Both the plane and the nudged plane are unusable: the
        // original error describes the level the user asked for.
        (Err(e), Err(_)) => Err(from_slice(e)),
    }
}

/// The empty-section refusal, with the fix: the Z span the source does
/// occupy, so the user can move Top of Part Z into it.
fn empty_at(facets: &[Triangle], z: f64) -> GenError {
    let (lo, hi) = facets
        .iter()
        .flat_map(|t| [t.a.z, t.b.z, t.c.z])
        .filter(|v| v.is_finite())
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)));
    if lo <= hi {
        GenError::Empty(format!(
            "no material at z = {z:.3} - the source spans Z{lo:.3} to Z{hi:.3}; set Top of Part Z inside that span, or pick the face or edges to machine"
        ))
    } else {
        GenError::Empty(format!("no material at z = {z:.3}"))
    }
}

/// Chords per semicircular cap of half-width `h` such that the
/// circumscribed polygon's corners stand at most `CAP_SAG_BUDGET`
/// outside the arc: a step of `phi` puts the corner at `h / cos(phi/2)`.
pub(super) fn cap_segments(h: f64) -> usize {
    if !(h.is_finite() && h > 0.0) {
        return CAP_MIN_SEGMENTS;
    }
    let half = (h / (h + CAP_SAG_BUDGET)).clamp(-1.0, 1.0).acos();
    if !(half.is_finite() && half > 0.0) {
        return CAP_MAX_SEGMENTS;
    }
    let n = (std::f64::consts::PI / (2.0 * half)).ceil();
    if n.is_finite() && n >= 1.0 {
        (n as usize).clamp(CAP_MIN_SEGMENTS, CAP_MAX_SEGMENTS)
    } else {
        CAP_MAX_SEGMENTS
    }
}

/// Closed-form stadium: `a`/`b` centers, half-width `h`. The region only
/// feeds verification and the drawn outline (the slot generator computes
/// the path in closed form), and any finite inputs build a simple closed
/// ring.
///
/// Each round end is the polygon CIRCUMSCRIBED about its semicircle
/// (#510): `n` tangent chords of step `phi = pi / n`, whose corners sit
/// at radius `h / cos(phi/2)` midway between tangent points. Every chord
/// is tangent to the true arc, so a cutter tangent to the cap is never
/// nearer to the polygon than to the arc - an inscribed cap put its
/// chords up to `h (1 - cos(phi/2))` INSIDE the arc (16 chords, 6 mm
/// cutter: 0.0145 mm, past `GOUGE_TOL`) and convicted every pass that
/// reached the end. The corners give a real over-cut at most
/// `CAP_SAG_BUDGET` of slack. The first and last corner of each cap lie
/// on the straight sides' lines, so the sides stay exact.
pub(super) fn stadium(a: Point2, b: Point2, h: f64) -> Result<Region, GenError> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt();
    let (ux, uy) = if len > 1e-9 {
        (dx / len, dy / len)
    } else {
        (1.0, 0.0)
    };
    let (nx, ny) = (-uy, ux);
    let n = cap_segments(h);
    let phi = std::f64::consts::PI / n as f64;
    let rc = h / (phi * 0.5).cos();
    let mut pts = Vec::with_capacity(2 * n);
    for (c, t0) in [(b, -std::f64::consts::FRAC_PI_2), (a, std::f64::consts::FRAC_PI_2)] {
        for i in 0..n {
            let th = t0 + phi * (i as f64 + 0.5);
            let (ca, sa) = (th.cos(), th.sin());
            pts.push(Point2::new(
                c.x + rc * (ca * ux + sa * nx),
                c.y + rc * (ca * uy + sa * ny),
            ));
        }
    }
    let loop_ = Loop::new(pts)?;
    Ok(Region::new(vec![loop_])?)
}

/// The shared refusal phrasing for an unresolved height, named by field.
/// One source so `resolve_params`, the region builders and every command
/// error read identically.
pub fn unresolved_msg(
    what: &str,
    h: Height,
    world: &World,
    setup: Option<&str>,
    src: Option<&SrcRef>,
) -> String {
    format!(
        "{what} (From: {ref}, offset {off}) is unresolved: {why}",
        ref = h.reference,
        off = h.offset,
        why = missing_source(world, setup, src, h.reference)
            .unwrap_or("the source has no finite height"),
    )
}

pub(super) fn unresolved_err(
    what: &str,
    h: Height,
    world: &World,
    setup: Option<&str>,
    src: Option<&SrcRef>,
) -> GenError {
    GenError::Invalid(unresolved_msg(what, h, world, setup, src))
}

/// A required height slot of an already-`Ok` `ResolvedHeights` is `Some`
/// by that function's contract (per kind); the error arm exists so no
/// call site needs an unwrap.
pub(super) fn req_z(v: Option<f64>, what: &str) -> Result<f64, GenError> {
    v.ok_or_else(|| GenError::Invalid(format!("{what} is unresolved")))
}
