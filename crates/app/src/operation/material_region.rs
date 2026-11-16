//! Selection-to-region and open-chain turn logic: the cutting region an
//! operation draws and generates from (`region_of`, `region_turn_of`),
//! the picked chains it rides, and the selection-authors-region law.
//! Split from `material` along the domain seam (400-line law).
//!
//! Provenance: our own domain model - specced from public documentation of
//! commercial CAM workflows; no vendor source was consulted.

use super::*;

/// The cutting region of an operation as the world stands now - the same
/// construction `generate` runs, without the path (and without the tool,
/// which the region never consults). This is the viewport's "what gets
/// cut" outline. None when there is nothing honest to draw: a vanished
/// source or an unsliceable level shows nothing, never a stale loop
/// (Prime Directive: a wrong preview is as bad as a wrong toolpath).
#[must_use]
pub fn region_of(world: &World, op: &Operation) -> Option<Region> {
    region_turn_of(world, op).map(|r| r.region)
}

/// [`region_of`]'s full answer: the world region, the turn the area
/// controls lay Face's rows at (0 without an open chain), the resolved
/// area controls (`None` at their identity) and - under a 3+2 index -
/// the LOCAL region with the plane it stands at (M13.10), so a band
/// built in the frame projects through it instead of being laid on the
/// world shadow. What `footprint` needs to draw the band Face really
/// clears, or the open pass a Contour rides.
pub(super) struct RegionTurn {
    pub(super) region: Region,
    pub(super) turn: f64,
    pub(super) ctx: Option<AreaCtx>,
    pub(super) local: Option<(Region, f64)>,
}

pub(super) fn region_turn_of(world: &World, op: &Operation) -> Option<RegionTurn> {
    // The 3+2 index (M5.1 B): a tilted planar op draws the cutting
    // plane's own shadow - the local region through the frame's door,
    // at the plane its `region_height` names. A tilt that commands no
    // direction (only a non-finite letter could, refused at the door
    // long before here) and a picked selection under tilt (generation
    // refuses it by name) both draw nothing: a preview generate would
    // refuse is exactly the lie this function exists to avoid.
    let frame = match op.params.tilt() {
        None => None,
        Some(t) => Some(t.frame()?),
    };
    if frame.is_some() && matches!(&op.src, SrcRef::Selection(_)) {
        return None;
    }
    // The area controls (M11.5), resolved as generation resolves them; a
    // refusal draws nothing - generation names it.
    let ctx = match area_ctx_of(world, op) {
        None => None,
        Some(Ok(mut c)) => {
            if let OpParams::Contour { side, .. } = &op.params {
                let inside = match side {
                    Side::Inside => true,
                    Side::Outside => false,
                    Side::Auto => contour_auto_side(world, op.setup.as_deref(), &op.src, &op.params)
                        .unwrap_or(false),
                };
                c.contour_side(inside);
            }
            Some(c)
        }
        Some(Err(_)) => return None,
    };
    // A Contour under an open boundary rides the chain itself
    // (`contour_open`), never the area law: its region is the area that
    // picks the side, as generation returns it - shown only while the
    // pass can be built, so the preview never outlines a refused one.
    let open_contour = match (&op.params, &ctx) {
        (OpParams::Contour { stock_to_leave, .. }, Some(c)) if c.open.is_some() => Some(*stock_to_leave),
        _ => None,
    };
    let controlled = |r: Option<Region>| {
        let r = r?;
        if let (Some(stl), Some(c)) = (open_contour, ctx.as_ref()) {
            return super::contour_open::open_run(&r, c, stl).ok().map(|_| (r, 0.0));
        }
        apply_area_full(r, ctx.as_ref()).ok().map(|a| (a.region, a.turn))
    };
    let (region, turn, local) = region_turn_core(world, op, frame.as_ref(), controlled)?;
    Some(RegionTurn { region, turn, ctx, local })
}

/// The world region, the turn, and - under a tilt - the local region
/// with its plane.
type Laid = (Region, f64, Option<(Region, f64)>);

/// The region and turn of [`region_turn_of`] with the area controls
/// applied by `controlled`, split out so the controls' context outlives
/// its borrow; the third is the local region and its plane under a tilt.
fn region_turn_core(
    world: &World,
    op: &Operation,
    frame: Option<&crate::cl::TiltedFrame>,
    controlled: impl Fn(Option<Region>) -> Option<(Region, f64)>,
) -> Option<Laid> {
    if let SrcRef::Selection(items) = &op.src
        && !matches!(
            op.params,
            OpParams::Slot { .. }
                | OpParams::Drill { .. }
                // The hole-list rides take their boundary from the
                // holes, never from a pick (M2.4 C).
                | OpParams::Bore { .. }
                | OpParams::Thread { .. }
                | OpParams::Pocket3D { .. }
                | OpParams::Finish3D { .. }
                // The deburr ride's edges ARE the model's own exposed
                // boundary - a pick could only disagree with what the
                // weld already found (M2.5 E B).
                | OpParams::Deburr { .. }
                // The drive-curve contour's picks are the PATH, not a
                // boundary: the 2D selection walk below would try to
                // bound an area with them (and refuse an open run),
                // where the honest preview is the material's shadow -
                // what the ball rides is the 3D chain itself, visible
                // once generated (M5.1 J).
                | OpParams::MultiAxisContour { .. }
        )
    {
        return controlled(selection_region(world, op.setup.as_deref(), items, &op.params, &op.area).ok()).map(|(r, t)| (r, t, None));
    }
    // A whole part under the pocket family: the stock around it, drawn
    // as generation cuts it (issue #12) - local under a tilt.
    if let Some(r) = whole_model_pocket(world, op.setup.as_deref(), &op.src, &op.params, frame) {
        let (r, turn) = controlled(r.ok())?;
        return match frame {
            None => Some((r, turn, None)),
            Some(f) => {
                let zl = resolve_height(world, op.setup.as_deref(), Some(&op.src), op.params.region_height())?
                    - f.origin.z;
                f.world_region(&r, zl).ok().map(|w| (w, turn, Some((r, zl))))
            }
        };
    }
    let material = material_of(world, op.setup.as_deref(), &op.src).ok()?;
    match frame {
        None => controlled(
            region_for(world, op.setup.as_deref(), Some(&op.src), &material, &op.params, None).ok(),
        )
        .map(|(r, t)| (r, t, None)),
        Some(f) => {
            let local = Material {
                excess: material.excess,
                facets: f.local_facets(&material.facets),
                top: material.top - f.origin.z,
                bodies: material.bodies.clone(),
            };
            let local = region_for(
                world,
                op.setup.as_deref(),
                Some(&op.src),
                &local,
                &op.params,
                Some(f),
            )
            .ok()?;
            let zl =
                resolve_height(world, op.setup.as_deref(), Some(&op.src), op.params.region_height())?
                    - f.origin.z;
            // The area controls run in the frame too (M11.5 owed): the
            // context was built through the same door, so the law
            // applies to the local region and the result rides the
            // wrap out - exactly `run_job`'s order.
            let (local, turn) = controlled(Some(local))?;
            f.world_region(&local, zl).ok().map(|r| (r, turn, Some((local, zl))))
        }
    }
}

/// The centerline runs `generate` would add to `region_of`'s outline for
/// this operation as the world stands now - the open chains of a Contour
/// selection, in the order they were picked. Drawn and generated through
/// this one call, so the viewport can never show a run the path does not
/// cut (or hide one it does). Unresolvable selections yield nothing, like
/// a vanished source in `region_of`.
#[must_use]
pub fn chains_of(world: &World, op: &Operation) -> Vec<Vec<Point2>> {
    let SrcRef::Selection(items) = &op.src else {
        return Vec::new();
    };
    match selection_geometry(world, op.setup.as_deref(), items, &op.params) {
        Ok((_, chains)) => chains,
        Err(_) => Vec::new(),
    }
}

/// The drive chains of one operation as the world stands now, in
/// WORLD 3D (M5.1 J): the Multi-Axis Contour's source must be a
/// selection of edge-chain picks - the picks ARE the path, machined
/// where they stand. Any other source is a named refusal, never a
/// silent whole-model guess: a Stock or Entity source spells "cut
/// the model's own pattern", which is what the mesh-driven 5-axis
/// kinds exist for. The facet geometry the ball rests ON is NOT read
/// here: it is `material_of`'s union of the picked entities, whole -
/// the curve tells the ball where to go, the model says how high it
/// sits there.
pub(super) fn drive_chains_of(world: &World, op: &Operation) -> Result<Vec<Vec<Point3>>, GenError> {
    match &op.src {
        SrcRef::Selection(items) => drive_chains(world, items),
        SrcRef::Stock => Err(GenError::Invalid(
            "Multi-Axis Contour machines picked edge chains - pick them in the Geometry section, \
             or use 5-Axis Contour to machine the stock's own pattern"
                .into(),
        )),
        SrcRef::Entity(name) => Err(GenError::Invalid(format!(
            "Multi-Axis Contour machines picked edge chains, not a whole entity ('{name}') - \
             pick edges in its Geometry section, or use 5-Axis Contour for that model's pattern"
        ))),
    }
}

/// Whether a `Selection` source is allowed to author the region (the
/// guard the generation selection arm used to spell inline, lifted
/// so the threaded job builder - which precomputes that arm's answer on
/// the caller's thread - shares this one law). The kinds NOT listed
/// (Contour, Pocket, Face's picked loops) take the region from the
/// picks; every kind listed here is mesh-driven, and a picked loop
/// could only disagree with the path the model authors.
pub(crate) fn selection_drives_region(params: &OpParams) -> bool {
    !matches!(
        params,
        OpParams::Slot { .. }
            | OpParams::Drill { .. }
            // The hole-list rides: the recognized circles author the
            // boundary; a pick could only disagree with them
            // (M2.4 C).
            | OpParams::Bore { .. }
            | OpParams::Thread { .. }
            | OpParams::Pocket3D { .. }
            | OpParams::Finish3D { .. }
            // The deburr loci are the model's own exposed edges - the
            // weld finds them, and a pick could only disagree (E B).
            | OpParams::Deburr { .. }
            // The 5-axis kinds are mesh-driven like the finish: the
            // rings and raster lines chase the model at every plane,
            // so a picked loop could only disagree with the path
            // (M5.1 C).
            | OpParams::Contour5D { .. }
            | OpParams::Parallel5D { .. }
            // The swarf family is mesh-driven the same way, and more
            // strictly so: its pass lines ARE the ruling the picked
            // loop could only contradict (M5.1 D).
            | OpParams::Swarf { .. }
            | OpParams::AdvancedSwarf { .. }
            // The rotary twins ride the fitted axis, not a pick: the
            // turn's own geometry authors the path (M5.1 E).
            | OpParams::RotaryContour { .. }
            | OpParams::RotaryParallel { .. }
            // The rough ladder wraps the same axis - its islands come
            // from the mesh in wrap space, never a pick.
            | OpParams::RotaryOffset { .. }
            // The stamped ladder is the 3-D pocket's islands verbatim
            // - mesh sections at every plane, and the stamp only ever
            // writes poses onto them. Flow chases the mesh's own
            // direction field; a picked loop could only contradict it
            // (M5.1 I).
            | OpParams::MultiAxisRough { .. }
            | OpParams::Flow5D { .. }
            // The drive-curve contour is the one kind whose picks ARE
            // the path: the region is still the material's shadow - a
            // picked loop could not disagree with the pattern because
            // the picks ARE it (M5.1 J).
            | OpParams::MultiAxisContour { .. }
            // The facing ladder's limit is the turned profile's own
            // silhouette at every station: the meridian projection
            // authors it from the mesh, and a pick could only
            // disagree with the turn (M5.2 A). The profile kinds ride
            // that very silhouette - same answer (M5.2 B).
            | OpParams::Facing { .. }
            | OpParams::ProfileRough { .. }
            | OpParams::ProfileFinish { .. }
            // The blade stands in the turned part's own silhouette -
            // the band is model-bounded, not pick-bounded (M5.2 C).
            | OpParams::GrooveRough { .. }
            | OpParams::GrooveFinish { .. }
            | OpParams::Part { .. }
            // The lattice is the modeled thread's own - a pick could
            // only disagree with the helix (D). So is a face: the
            // window says which cone the ride walks, and the model
            // says it too (E).
            | OpParams::Threading { .. }
            | OpParams::Chamfering { .. }
            | OpParams::TaperTurning { .. }
            // So is a crown plunge: the window says which valleys the
            // gauge stamps, and the model says it too (M5.2 E2). So
            // is a bore: the hole the bar rides is in the model, and
            // a pick could only disagree with it (M5.2 F).
            | OpParams::MultiPassTurning { .. }
            | OpParams::FormTurning { .. }
            | OpParams::BoringTurning { .. }
    )
}
