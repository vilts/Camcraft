//! The per-kind source-built region (`region_for` / `region_core`), its
//! section height, and the hole-rim loops the drill family adds to it.
//! Split from `material` along the domain seam (400-line law).
//!
//! Provenance: our own domain model - specced from public documentation of
//! commercial CAM workflows; no vendor source was consulted.

use super::*;

/// The per-kind region build, shared by `generate` and `region_of` so the
/// preview outline cannot disagree with what the generators cut. The
/// section height resolves through the world like every other height in
/// the params: a preview with an unresolvable `top_z` shows nothing (the
/// `Err` folds to `None` in `region_of`), and a generation refuses by
/// name.
pub(super) fn region_for(
    world: &World,
    setup: Option<&str>,
    src: Option<&SrcRef>,
    material: &Material,
    params: &OpParams,
    frame: Option<&crate::cl::TiltedFrame>,
) -> Result<Region, GenError> {
    let z = section_z_of(world, setup, src, params).unwrap_or(Ok(0.0));
    region_core(material, params, frame, z)
}

/// The section height the Contour/Pocket/Trace families cut at, read
/// through the world - the ONLY part of the region build that touches
/// it. `None` for every kind whose region needs no height (the
/// silhouettes, the stadium, the rims). `region_for` resolves it on the
/// spot; the threaded job builder precomputes it on the caller's
/// thread, and `region_core` - pure - takes either. Same call, same
/// refusal, same number.
pub(super) fn section_z_of(
    world: &World,
    setup: Option<&str>,
    src: Option<&SrcRef>,
    params: &OpParams,
) -> Option<Result<f64, GenError>> {
    let top_z = match params {
        OpParams::Contour { top_z, .. }
        | OpParams::Pocket { top_z, .. }
        // The roughing twins read the pocket's section plane - the
        // same z whose ladder `region_core` cuts (M2.4 A). The
        // engrave twin sections like the trace, and the chamfer's
        // ride plane is the apex plane - the edge the cone lies
        // along (M2.4 B).
        | OpParams::Adaptive2D { top_z, .. }
        | OpParams::Offset2D { top_z, .. }
        | OpParams::Engrave { top_z, .. }
        | OpParams::Chamfer2D { top_z, .. }
        | OpParams::Trace { top_z, .. } => *top_z,
        _ => return None,
    };
    Some(
        resolve_height(world, setup, src, top_z)
            .ok_or_else(|| unresolved_err("Top of Part Z", top_z, world, setup, src)),
    )
}

/// The region build once every world read it needs is in hand: `z` is
/// `section_z_of`'s answer for the section kinds and an ignored
/// `Ok(0.0)` for the rest; `region_for` and the threaded job share this
/// one body, so a batched generation cannot disagree with a single one.
/// The outline of the material standing above the plane `z` (M13.11):
/// the silhouette of only those facets with a vertex above it. A boss
/// rising out of a faced plane shows here; a bore, or a pocket sunk
/// below the plane, does not - so a facing row can tell an island from
/// air. `None` when the slice refuses (the facer then lifts over every
/// hole, the safe reading, and says so).
pub(super) fn standing_above(material: &Material, z: f64) -> Option<Region> {
    let above: Vec<Triangle> = material
        .facets
        .iter()
        .filter(|t| t.a.z.max(t.b.z).max(t.c.z) > z + 1e-6)
        .copied()
        .collect();
    if above.is_empty() {
        return Some(Region::empty());
    }
    slice::project(&above, FOOTPRINT_SAMPLES).ok()
}

pub(super) fn region_core(
    material: &Material,
    params: &OpParams,
    frame: Option<&crate::cl::TiltedFrame>,
    z: Result<f64, GenError>,
) -> Result<Region, GenError> {
    // Every source-built region (Face's silhouette included) crosses the
    // one area-controls seam the selection-built ones cross (M11.4 A /
    // M11.5) at the caller - `run_job`'s precompute and `region_of` -
    // where the tool and the stock the controls read are in hand.
    source_region(material, params, frame, z)
}

/// The source-built region per kind, before the area controls.
fn source_region(
    material: &Material,
    params: &OpParams,
    frame: Option<&crate::cl::TiltedFrame>,
    z: Result<f64, GenError>,
) -> Result<Region, GenError> {
    // The section plane as the material's coordinates read it: the
    // world height minus the anchor's z is the frame's own distance
    // law (M5.1 B); None frame leaves every number untouched.
    let oz = frame.map_or(0.0, |f| f.origin.z);
    match params {
        OpParams::Face { .. } => {
            slice::project(&material.facets, FOOTPRINT_SAMPLES).map_err(from_slice)
        }
        // The probe bounds no area (#108): its generation never asks.
        OpParams::Probe { .. } => Err(GenError::Invalid("a probe has no machining region".into())),
        // The plasma cut goes THROUGH the plate (#670): what it can cut
        // is the through-silhouette - a blind pocket is no torch's work.
        OpParams::PlasmaProfile(_) => {
            slice::project(&material.facets, FOOTPRINT_SAMPLES).map_err(from_slice)
        }
        // The 3D pocket's machine area is a silhouette, not a section:
        // columns whose surface sits below the first level are still
        // cut, down to that surface - a section would miss them. The
        // per-level islands come from the mesh, in the generator and in
        // the verify pass, never from this region.
        OpParams::Pocket3D { .. } => {
            slice::project(&material.facets, FOOTPRINT_SAMPLES).map_err(from_slice)
        }
        // Same silhouette law for the finish - and for its 5-axis
        // twins, whose paths ARE the finish's paths with a stamped
        // pose: the rings chase the mesh at every plane, the region
        // only says where the model shadows.
        // The rotary twins chase the mesh around the fitted axis, so
        // the same shadow answers for them (stage E).
        OpParams::Finish3D { .. }
        // The deburr ride's machine area is the model's own shadow:
        // its loci are welded edges in the material, and the region
        // only says where the model stands (M2.5 E B).
        | OpParams::Deburr { .. }
        | OpParams::Contour5D { .. }
        | OpParams::Parallel5D { .. }
        | OpParams::Swarf { .. }
        | OpParams::AdvancedSwarf { .. }
        // The wrap ladder is mesh-driven the same way: its columns are
        // the shadow's, unrolled (stage E2).
        | OpParams::RotaryContour { .. }
        | OpParams::RotaryParallel { .. }
        | OpParams::RotaryOffset { .. }
        // The stamped ladder shadows the same silhouette: leaning the
        // spindle moves no position, so no footprint moves either -
        // and flow turns the lines inside the same shadow (M5.1 I).
        | OpParams::MultiAxisRough { .. }
        | OpParams::Flow5D { .. }
        // The drive-curve contour's ball rests on this same shadow's
        // solid; the chains it rides are resolved separately, in 3D,
        // at generation (M5.1 J).
        | OpParams::MultiAxisContour { .. }
        // Facing rides the same shadow for the panels: the station
        // limits are the meridian profile's own, computed in the
        // axis frame by the kernel (M5.2 A) - and so are the
        // profile kinds' rungs, the same silhouette read as skin
        // instead of as a floor (M5.2 B).
        | OpParams::Facing { .. }
        | OpParams::ProfileRough { .. }
        | OpParams::ProfileFinish { .. }
        // The blade's shadow is the bar's own disc - the kerf's
        // width lives in the meridian, not the footprint (M5.2 C).
        | OpParams::GrooveRough { .. }
        | OpParams::GrooveFinish { .. }
        // Threading's shadow is the bar's own disc too - the wave
        // lives in the meridian, not the footprint (M5.2 D). The
        // contour rides stand on the same disc: a face of
        // revolution casts the turned part's shadow (M5.2 E).
        | OpParams::Part { .. }
        | OpParams::Threading { .. }
        | OpParams::Chamfering { .. }
        | OpParams::TaperTurning { .. }
        // The crown gauge stands on the same disc: a face of
        // revolution casts the turned part's shadow (M5.2 E2). The
        // boring bar too - conservative by the bar's OD, which is
        // exactly the law the family keeps, and never a hide.
        | OpParams::MultiPassTurning { .. }
        | OpParams::FormTurning { .. }
        | OpParams::BoringTurning { .. } => {
            slice::project(&material.facets, FOOTPRINT_SAMPLES).map_err(from_slice)
        }
        // The twins hollow the region's inside exactly like the pocket
        // does (M2.4 A) - same section, same machine area. The
        // engrave twin sections the same plane (M2.4 B).
        OpParams::Contour { .. }
        | OpParams::Pocket { .. }
        | OpParams::Adaptive2D { .. }
        | OpParams::Offset2D { .. }
        | OpParams::Engrave { .. }
        | OpParams::Trace { .. } => {
            let z = z?;
            section(&material.facets, z - oz)
        }
        // The chamfer ride's machine area is the edge the cone lies
        // along: the section at the apex plane - a boundary ride the
        // contour's way (M2.4 B).
        OpParams::Chamfer2D { .. } => {
            let z = z?;
            section(&material.facets, z - oz)
        }
        OpParams::Slot { a, b, width, .. } | OpParams::TrochoidalSlot { a, b, width, .. } => {
            stadium(*a, *b, width * 0.5)
        }
        // The circular ladder's machine area is the AUTHORED disk -
        // the slot's authored-geometry law with a circle for a
        // stadium; the generator refuses a disk that meets no
        // material, by name (M2.4 B).
        OpParams::Circular { center, radius, .. } => {
            Ok(Region::new(vec![disk_loop(*center, *radius)?])?)
        }
        OpParams::Drill { holes, cross, .. } => {
            // A radial list asks for the silhouette only (M5.2 F2):
            // a cross hole opens on the bar's SKIN, so its mouth is
            // already in the projection, and its wall runs along X or
            // Y - a "rim" in the XY shadow would be a lie. The law
            // that proves a posed plunge is the 3-D envelope judge
            // the turning and multiaxis families ride, not this
            // circle-in-the-footprint tangency test.
            if !cross.is_empty() {
                return slice::project(&material.facets, FOOTPRINT_SAMPLES).map_err(from_slice);
            }
            // The silhouette, plus a rim loop for every hole (a blind
            // hole's mouth sits inside the material's shadow and the
            // projection never sees it; a through wall does show up,
            // but as the mesh's own coarse polygon). The rims are what
            // make the gouge law's tangency test mean something at a
            // plunge: d(center, rim) = hole radius, so a matched drill
            // passes silent and an oversized tool is honestly flagged.
            // A silhouette loop tracing a hole is REPLACED, not kept:
            // a 16-facet wall's apothem sits a chord-sag inside its own
            // radius - far more than the gouge law's slack at drill
            // diameters - and the law would read that tessellation as
            // tool bite. The hole's parameters are the truth here.
            let base = slice::project(&material.facets, FOOTPRINT_SAMPLES).map_err(from_slice)?;
            if holes.is_empty() {
                return Ok(base);
            }
            let mut loops: Vec<Loop> = base
                .loops()
                .iter()
                .filter(|l| !holes.iter().any(|h| rim_is(l, *h)))
                .cloned()
                .collect();
            for hole in holes {
                loops.push(rim_loop(*hole)?);
            }
            Ok(Region::new(loops)?)
        }
        // The boring ride and the thread mill take the drill's rim
        // law verbatim (M2.4 C): the wall the model shows IS the
        // region, so the gouge test's tangency stands at the circle
        // the ring rides. No cross list - these are mill kinds.
        OpParams::Bore { holes, .. } | OpParams::Thread { holes, .. } => {
            let base = slice::project(&material.facets, FOOTPRINT_SAMPLES).map_err(from_slice)?;
            if holes.is_empty() {
                return Ok(base);
            }
            let mut loops: Vec<Loop> = base
                .loops()
                .iter()
                .filter(|l| !holes.iter().any(|h| rim_is(l, *h)))
                .cloned()
                .collect();
            for hole in holes {
                loops.push(rim_loop(*hole)?);
            }
            Ok(Region::new(loops)?)
        }
    }
}

/// Rim loop segments: enough that the chord sag below the circle,
/// r·(1−cos(π/n)), stays under RIM_SAG_BUDGET - half the tolerance the
/// gouge law grants, so a matched tool clears its own rim by the other
/// half, never by an accident of the segment count. That is 48 sides at
/// drill diameters and ~110 at a 12 mm bore radius; the floor keeps
/// tiny holes from degenerating and the ceiling keeps huge ones cheap.
pub(super) const RIM_SAG_BUDGET: f64 = 0.005;
pub(super) const DRILL_RIM_SEGMENTS: usize = 48;
pub(super) const DRILL_RIM_MAX_SEGMENTS: usize = 4096;

pub(super) fn rim_segments(r: f64) -> usize {
    let x = (RIM_SAG_BUDGET / r).clamp(0.0, 2.0);
    let n = (std::f64::consts::PI / (1.0 - x).acos()).ceil() as usize;
    n.clamp(DRILL_RIM_SEGMENTS, DRILL_RIM_MAX_SEGMENTS)
}

/// A polygonal circle at a hole's XY - the loop the rim of `h` adds to a
/// drill region. `Region::new` orients by nesting, so the winding here
/// is free.
pub(super) fn rim_loop(h: Hole) -> Result<Loop, GenError> {
    disk_loop(h.xy, h.radius)
}

/// A polygonal circle about `center` - the drill rim's builder with an
/// authored center and radius, which is exactly what the circular
/// ladder's disk is (M2.4 B).
pub(super) fn disk_loop(center: Point2, radius: f64) -> Result<Loop, GenError> {
    let n = rim_segments(radius);
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let th = std::f64::consts::TAU * i as f64 / n as f64;
        pts.push(Point2::new(
            center.x + radius * th.cos(),
            center.y + radius * th.sin(),
        ));
    }
    Ok(Loop::new(pts)?)
}

/// Whether `l` is `h`'s rim already: enough vertices, and every one of
/// them on the hole's circle within a band that forgives tessellation
/// (the fitted radius and the sliced rim share the mesh's own vertices
/// for a through wall; only quantization separates them). A square, a
/// slot end, or a different concentric circle fails.
pub(super) fn rim_is(l: &Loop, h: Hole) -> bool {
    let pts = l.points();
    if pts.len() < 8 {
        return false;
    }
    let tol = (0.02 * h.radius).max(0.1);
    pts.iter()
        .all(|p| ((p.x - h.xy.x).hypot(p.y - h.xy.y) - h.radius).abs() <= tol)
}

/// The stock the verify's column law reads as material (`VerifyContext::
/// stock`): the setup's stock when the source is a model, and - #1066 -
/// the source itself when the source IS the stock and the kind's region
/// punches the holes' rims out of it (Bore, Thread). That region says
/// where the walls stand, not where material is: the hole's column above
/// its mouth is uncut stock until something removes it, and read through
/// the punched region the ladder's entry rapid down to the mouth's plunge
/// height under a taller block went unjudged and posted. Every other
/// stock-sourced kind's region IS the stock's footprint, so it keeps
/// `None` (the region stands in) byte for byte.
pub(super) fn judged_stock<'a>(
    stock_facets: Option<&'a [Triangle]>,
    src: &SrcRef,
    params: &OpParams,
    material: &'a Material,
) -> Option<&'a [Triangle]> {
    stock_facets.or_else(|| {
        (matches!(src, SrcRef::Stock) && matches!(params, OpParams::Bore { .. } | OpParams::Thread { .. }))
            .then_some(&material.facets[..])
    })
}
