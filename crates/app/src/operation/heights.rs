

use super::*;

impl Height {
    /// Resolve against the current world; None when the source is not
    /// available (no stock or model yet, no entity in the slot). The
    /// default setup's world - a caller standing in another setup uses
    /// [`Self::z_for`] (M3.2 stage A).
    #[must_use]
    pub fn z(&self, world: &flecs_ecs::prelude::World) -> Option<f64> {
        self.z_for(world, None)
    }

    /// The same resolve in a declared setup context ([`crate::setup`]).
    /// No operation source is in hand, so a Selection reference reads
    /// None here - callers holding the op use [`resolve_height`].
    #[must_use]
    pub fn z_for(
        &self,
        world: &flecs_ecs::prelude::World,
        setup: Option<&str>,
    ) -> Option<f64> {
        resolve_height(world, setup, None, *self)
    }

    /// The full resolve: setup context plus the operation's own source.
    #[must_use]
    pub fn z_in(
        &self,
        world: &flecs_ecs::prelude::World,
        setup: Option<&str>,
        src: Option<&SrcRef>,
    ) -> Option<f64> {
        resolve_height(world, setup, src, *self)
    }

}

/// Resolve one height to a world Z. `Fixed` reads through; the other
/// sources ask the world for the current geometry (model bounds are the
/// parts': no stock, WCS or fixture, #1529; a slot entity that exists
/// but was never moved reads at the origin, matching `transform_of`'s
/// own default-everything semantics).
/// `setup` is the declared setup of the asking operation (`None` = the
/// first setup, which in a document without setups is the old global
/// lookup exactly - the implicit-setup law of [`crate::setup`]). The
/// model bounds are the parts as the setup holds them (#326's part
/// alignment; as modelled, they are the document bounds).
/// `src` is the asking operation's own source, which the Selection
/// references read ([`selection_z_range`], M11.3 A); None = no op in
/// hand, and a Selection reference is then unresolved, never guessed.
#[must_use]
pub fn resolve_height(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    src: Option<&SrcRef>,
    h: Height,
) -> Option<f64> {
    let base = match h.reference {
        HeightReference::Fixed => return (h.offset.is_finite()).then_some(h.offset),
        HeightReference::TopOfStock => model::stock_world_aabb_for(world, setup)?.max.z,
        HeightReference::BottomOfStock => model::stock_world_aabb_for(world, setup)?.min.z,
        // #326: the parts' bounds in the setup's frame - a flipped
        // part's top is its former bottom. #1529: the parts only - a
        // vise, accessory or tagged fixture (of any setup) is never
        // the model the heights stand on.
        HeightReference::TopOfModel => crate::setup_align::part_aabb_in(world, setup)?.max.z,
        HeightReference::BottomOfModel => crate::setup_align::part_aabb_in(world, setup)?.min.z,
        HeightReference::WorkOffset => {
            // #329, #342: the setup's one WCS, never the height's own.
            let entity = crate::setup::wcs_entity(world, setup)?;
            model::transform_of(world, &entity).unwrap_or_default().pos.z
        }
        HeightReference::TopOfSelection | HeightReference::BottomOfSelection => {
            selection_base(world, setup, src, h.reference)?
        }
    };
    (base + h.offset).is_finite().then_some(base + h.offset)
}

/// What a reference is missing, phrased for a diagnostic: the user can
/// act on "no stock yet". None when the source stands.
#[must_use]
pub fn missing_source(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    src: Option<&SrcRef>,
    r: HeightReference,
) -> Option<&'static str> {
    match r {
        HeightReference::TopOfSelection | HeightReference::BottomOfSelection => match src {
            None => Some(SELECTION_UNKNOWN),
            Some(s) => selection_z_range(world, setup, s).err(),
        },
        HeightReference::Fixed => None,
        HeightReference::TopOfStock | HeightReference::BottomOfStock => {
            model::stock_world_aabb_for(world, setup)
                .is_none()
                .then_some("no stock yet - set it up in Setup > Stock")
        }
        HeightReference::TopOfModel | HeightReference::BottomOfModel => {
            crate::setup_align::part_aabb_in(world, setup)
                .is_none()
                .then_some("no model loaded")
        }
        HeightReference::WorkOffset => crate::setup::wcs_entity(world, setup)
            .is_none()
            .then_some("the setup's WCS has no origin yet - place it in Setup"),
    }
}

/// The Z the operation's region lives at, resolved: the section/stadium
/// plane for the depth kinds, the skim (or first stage) plane for Face.
/// None when that height's reference has no source.
#[must_use]
pub fn region_z(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    src: Option<&SrcRef>,
    params: &OpParams,
) -> Option<f64> {
    match params {
        OpParams::Face { top_z, cut_z, .. } => top_z
            .map(|t| resolve_height(world, setup, src, t))
            .unwrap_or_else(|| resolve_height(world, setup, src, *cut_z)),
        OpParams::Contour { top_z, .. }
        | OpParams::Pocket { top_z, .. }
        // The roughing twins name the pocket's high station (M2.4 A),
        // and so do the circular ladder and the engrave ride; the
        // chamfer's top plane is where its cone's apex stands - the
        // ride's only height (M2.4 B).
        | OpParams::Adaptive2D { top_z, .. }
        | OpParams::Offset2D { top_z, .. }
        | OpParams::Engrave { top_z, .. }
        | OpParams::Chamfer2D { top_z, .. }
        | OpParams::Circular { top_z, .. }
        | OpParams::Slot { top_z, .. }
        | OpParams::TrochoidalSlot { top_z, .. }
        | OpParams::Trace { top_z, .. }
        | OpParams::Drill { top_z, .. }
        | OpParams::Pocket3D { top_z, .. }
        | OpParams::Finish3D { top_z, .. }
        | OpParams::Contour5D { top_z, .. }
        | OpParams::Parallel5D { top_z, .. }
        | OpParams::Swarf { top_z, .. }
        | OpParams::AdvancedSwarf { top_z, .. }
        | OpParams::RotaryContour { top_z, .. }
        | OpParams::RotaryParallel { top_z, .. }
        | OpParams::RotaryOffset { top_z, .. }
        | OpParams::MultiAxisRough { top_z, .. }
        | OpParams::Flow5D { top_z, .. }
        | OpParams::MultiAxisContour { top_z, .. }
        | OpParams::Facing { top_z, .. }
        | OpParams::ProfileRough { top_z, .. }
        | OpParams::ProfileFinish { top_z, .. }
        | OpParams::GrooveRough { top_z, .. }
        | OpParams::GrooveFinish { top_z, .. }
        | OpParams::Part { top_z, .. }
        | OpParams::Threading { top_z, .. }
        // The contour windows name the same high station (M5.2 E).
        | OpParams::Chamfering { top_z, .. }
        | OpParams::TaperTurning { top_z, .. }
        // The form window names the same high station (M5.2 E2), and
        // so does the bore's (M5.2 F).
        | OpParams::MultiPassTurning { top_z, .. }
        | OpParams::FormTurning { top_z, .. }
        | OpParams::BoringTurning { top_z, .. }
        // The holes' entry plane names the same station (M2.4 C).
        | OpParams::Bore { top_z, .. }
        | OpParams::Thread { top_z, .. }
        // The deburr window's ceiling names the same high station:
        // the ride's tips stand at or below it (M2.5 E B).
        | OpParams::Deburr { top_z, .. } => resolve_height(world, setup, src, *top_z),
        // The probe's station is its expected contact (#108).
        OpParams::Probe { target, .. } => Some(target[2]),
        // The plasma cut rides the plate's top (#670).
        OpParams::PlasmaProfile(p) => resolve_height(world, setup, src, p.top_z),
        // The wire's profile is read on the workpiece's top (#532).
        OpParams::WireContour(p) => resolve_height(world, setup, src, p.top_z),
        // The electrode enters at the work top (#541).
        OpParams::SinkerBurn(p) | OpParams::SinkerSideBurn(crate::operation::SinkerSideBurnOp { burn: p, .. }) => resolve_height(world, setup, src, p.top_z),
    }
}

/// Every height of one params set, resolved at once for a generation or
/// a panel pass. Err carries the field name and the source that is
/// missing, so the refusal is actionable by name - never a guessed z.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedHeights {
    /// Rapids plane, every kind.
    pub clearance: f64,
    /// Depth kinds: section top. Face: None (it has [`Self::face_top`]).
    pub top: Option<f64>,
    /// Depth kinds: last level. Face: the finished cut plane.
    pub bottom: Option<f64>,
    /// Face: first stage when stepped, None for one-level facing.
    pub face_top: Option<f64>,
    /// Contour resolves Some; pocket-family None hops at clearance.
    pub retract: Option<f64>,
}

impl ResolvedHeights {
    /// The height-plane Zs of one kind: (top, bottom/cut) as the panels
    /// and verify want them, from the resolved bag.
    #[must_use]
    pub fn depth(&self) -> (Option<f64>, Option<f64>) {
        if self.face_top.is_some() || self.top.is_some() {
            (self.top.or(self.face_top), self.bottom)
        } else {
            (self.face_top, self.bottom)
        }
    }
}

/// Resolve every height in `params`; Err is "<field> (From: <ref> <offset>):
/// <why>" - each refusal names the height and its source.
pub fn resolve_params(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    src: Option<&SrcRef>,
    params: &OpParams,
) -> Result<ResolvedHeights, String> {
    let req = |what: &str, h: Height| -> Result<f64, String> {
        resolve_height(world, setup, src, h)
            .ok_or_else(|| unresolved_msg(what, h, world, setup, src))
    };
    let opt = |what: &str, h: Option<Height>| -> Result<Option<f64>, String> {
        h.map(|v| req(what, v)).transpose()
    };
    let (clearance, top, bottom, face_top, retract) = match params {
        OpParams::Face { clearance_z, cut_z, top_z, .. } => (
            req("Clearance Z", *clearance_z)?,
            None,
            Some(req("Cut Z", *cut_z)?),
            opt("Top Z", *top_z)?,
            None,
        ),
        OpParams::Contour { clearance_z, retract_z, top_z, bottom_z, .. } => (
            req("Clearance Z", *clearance_z)?,
            Some(req("Top of Part Z", *top_z)?),
            Some(req("Bottom of Part Z", *bottom_z)?),
            None,
            Some(req("Retract Z", *retract_z)?),
        ),
        OpParams::Pocket { clearance_z, retract_z, top_z, bottom_z, .. }
        // The roughing twins carry the pocket's four heights verbatim
        // (M2.4 A) - same ladder shape, same hop. The engrave twin
        // and the circular ladder do too (M2.4 B).
        | OpParams::Adaptive2D { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::Offset2D { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::Engrave { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::Circular { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::Slot { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::TrochoidalSlot { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::Trace { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::Pocket3D { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::Finish3D { clearance_z, retract_z, top_z, bottom_z, .. }
        // The 5-axis kinds are the finish's height shape exactly: the
        // same four named heights, the same optional hop.
        | OpParams::Contour5D { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::Parallel5D { clearance_z, retract_z, top_z, bottom_z, .. }
        // The swarf family's too - stage D adds no height to the shape,
        // nor does rotary: the generators read these world planes into
        // axial stations themselves (stage E).
        | OpParams::Swarf { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::AdvancedSwarf { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::RotaryContour { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::RotaryParallel { clearance_z, retract_z, top_z, bottom_z, .. }
        // The stamped ladder's four are the pocket family's, the
        // optional hop among them (M5.1 stage E3), and flow's are
        // the finish's four the same way (M5.1 I).
        | OpParams::MultiAxisRough { clearance_z, retract_z, top_z, bottom_z, .. }
        // The drive-curve contour names the same four, and its
        // top/bottom are the window the picked chain must stand in
        // (M5.1 J).
        | OpParams::Flow5D { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::MultiAxisContour { clearance_z, retract_z, top_z, bottom_z, .. } => (
            req("Clearance Z", *clearance_z)?,
            Some(req("Top of Part Z", *top_z)?),
            Some(req("Bottom of Part Z", *bottom_z)?),
            None,
            opt("Retract Z", *retract_z)?,
        ),
        // The 2-D chamfer resolves TWO heights: the clearance and the
        // plane the cone's apex stands at. Its floor is derived - the
        // wall's own width · tan(angle) below the apex - so `bottom`
        // stays None, Face's skipped-field the other way (M2.4 B).
        OpParams::Chamfer2D { clearance_z, retract_z, top_z, .. } => (
            req("Clearance Z", *clearance_z)?,
            Some(req("Top of Part Z", *top_z)?),
            None,
            None,
            opt("Retract Z", *retract_z)?,
        ),
        // The wrap ladder names its own rapids plane - one infeed and
        // a tool radius outside the skin - so there is no hop height
        // to resolve; the four-number shape minus the retract is all
        // it has (M5.1 stage E2).
        OpParams::RotaryOffset { clearance_z, top_z, bottom_z, .. } => (
            req("Clearance Z", *clearance_z)?,
            Some(req("Top of Part Z", *top_z)?),
            Some(req("Bottom of Part Z", *bottom_z)?),
            None,
            None,
        ),
        // The facing ladder is the wrap ladder's height shape spoken
        // on the axis instead of the skin: the same three, no hop -
        // the station rapid names itself one infeed clear of the
        // profile (M5.2 stage A).
        OpParams::Facing { clearance_z, top_z, bottom_z, .. } => (
            req("Clearance Z", *clearance_z)?,
            Some(req("Top of Part Z", *top_z)?),
            Some(req("Facing Plane Z", *bottom_z)?),
            None,
            None,
        ),
        // The profile window stands the same way on the axis - three
        // heights, no hop; the rapids plane of the ride names itself
        // a tolerance clear of the rungs' reach (M5.2 stage B).
        OpParams::ProfileRough { clearance_z, top_z, bottom_z, .. }
        | OpParams::ProfileFinish { clearance_z, top_z, bottom_z, .. }
        // The blade windows state the same trio: three heights, no
        // hop - the rapids ride the whole bar's radius, a law and not
        // a height (M5.2 stage C).
        | OpParams::GrooveRough { clearance_z, top_z, bottom_z, .. }
        | OpParams::GrooveFinish { clearance_z, top_z, bottom_z, .. }
        // Threading states the same trio - the window of turns the
        // lattice of V's fills, rapids clear of every crest (D) -
        // and the contour rides state it across a face: the same
        // three heights, the promise in the kernel's door (E).
        | OpParams::Threading { clearance_z, top_z, bottom_z, .. }
        | OpParams::Chamfering { clearance_z, top_z, bottom_z, .. }
        | OpParams::TaperTurning { clearance_z, top_z, bottom_z, .. }
        // The crown gauge states the same trio across the window
        // its formed surfaces stand in (M5.2 E2), and the boring
        // bar across the window of its wall (M5.2 F).
        | OpParams::MultiPassTurning { clearance_z, top_z, bottom_z, .. }
        | OpParams::FormTurning { clearance_z, top_z, bottom_z, .. }
        | OpParams::BoringTurning { clearance_z, top_z, bottom_z, .. } => (
            req("Clearance Z", *clearance_z)?,
            Some(req("Top of Part Z", *top_z)?),
            Some(req("Bottom of Part Z", *bottom_z)?),
            None,
            None,
        ),
        // The deburr ride states the trio too - but its two window
        // heights are named for what they bound: the TIP's ceiling
        // and floor, since the kernel stands the tip `drop` below
        // each edge it breaks (M2.5 E B). The rapids plane hops every
        // run itself; no hop height exists to resolve.
        OpParams::Deburr { clearance_z, top_z, bottom_z, .. } => (
            req("Clearance Z", *clearance_z)?,
            Some(req("Window Top Z", *top_z)?),
            Some(req("Window Bottom Z", *bottom_z)?),
            None,
            None,
        ),
        // Parting names its bottom for what it is - the plane the
        // blade sinks through, facing's law read as a sever (M5.2 C).
        OpParams::Part {
            clearance_z,
            top_z,
            bottom_z,
            ..
        } => (
            req("Clearance Z", *clearance_z)?,
            Some(req("Top of Part Z", *top_z)?),
            Some(req("Parting Plane Z", *bottom_z)?),
            None,
            None,
        ),
        // The drill bottom is an override of the whole pattern, not a
        // requirement: each hole's own floor is the default depth, so an
        // unresolved (None) bottom is the normal shape, not a failure.
        OpParams::Drill { clearance_z, retract_z, top_z, bottom_z, .. }
        // The boring ride and the thread mill carry the drill's four
        // heights verbatim (M2.4 C) - the floors ride the holes, the
        // override rides the bag.
        | OpParams::Bore { clearance_z, retract_z, top_z, bottom_z, .. }
        | OpParams::Thread { clearance_z, retract_z, top_z, bottom_z, .. } => (
            req("Clearance Z", *clearance_z)?,
            Some(req("Top of Part Z", *top_z)?),
            opt("Bottom Z", *bottom_z)?,
            None,
            opt("Retract Z", *retract_z)?,
        ),
        // The probe states one height: the safe plane it travels at
        // between touches (#108); the touches ride the target.
        OpParams::Probe { clearance_z, .. } => (req("Clearance Z", *clearance_z)?, None, None, None, None),
        // The plasma cut (#670): the torch-off plane, the plate's top
        // (the cut plane) and bottom (its thickness) - no hop plane.
        OpParams::PlasmaProfile(p) => (
            req("Clearance Z", p.clearance_z)?,
            Some(req("Top of Plate Z", p.top_z)?),
            Some(req("Bottom of Plate Z", p.bottom_z)?),
            None,
            None,
        ),
        // The wire cut (#532): the workpiece's top and bottom - the two
        // programmed planes. A wire never lifts out of the work, so no
        // rapids plane is stated; the slot reads the top.
        OpParams::WireContour(p) => {
            let top = req("Top of Workpiece Z", p.top_z)?;
            (top, Some(top), Some(req("Bottom of Workpiece Z", p.bottom_z)?), None, None)
        }
        // The sinker burn (#541): the approach plane, the work top and
        // the cavity floor datum - no hop plane.
        OpParams::SinkerBurn(p) | OpParams::SinkerSideBurn(crate::operation::SinkerSideBurnOp { burn: p, .. }) => (
            req("Clearance Z", p.clearance_z)?,
            Some(req("Work Top Z", p.top_z)?),
            Some(req("Cavity Floor Z", p.floor_z)?),
            None,
            None,
        ),
    };
    Ok(ResolvedHeights { clearance, top, bottom, face_top, retract })
}

/// The resolved depth pair for the verify context and the level list:
/// (top, bottom), Face folded to (cut, cut-or-top) - None parts stay
/// None, never guessed.
#[must_use]
pub fn resolve_map(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    src: Option<&SrcRef>,
    params: &OpParams,
) -> (Option<f64>, Option<f64>) {
    match resolve_params(world, setup, src, params) {
        Ok(r) => {
            if matches!(params, OpParams::Face { .. }) {
                let cut = r.bottom;
                (cut, cut)
            } else {
                (r.top, r.bottom)
            }
        }
        Err(_) => (None, None),
    }
}
