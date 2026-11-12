//! A milling tool's stored drawn cutter (#93 slice 2): the one reader
//! and the one writer of the `ToolCutter` component, and the outline the
//! editor and the side view show - the drawn one while it still agrees
//! with the tool's numbers, else the one the numbers draw. No generator
//! reads it (the numbers stay the truth); the stack laws and the
//! simulation read only its neck (#852, [`neck_of`]) - where it is
//! narrower than the stated shank, never wider. Our own design over
//! `camcraft_tools::cutter_profile` and `camcraft_tools::neck`.

use flecs_ecs::core::EntityViewGet; // try_get is not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::{Entity, World};

use camcraft_tools::cutter_profile;
use camcraft_tools::holder_profile::HolderBand;
use camcraft_tools::neck;
use camcraft_tools::spun::SpunProfile;
use camcraft_tools::tools::Tool;

use crate::schema::ToolCutter;

/// The drawn cutter on a tool entity, if it has one.
#[must_use]
pub fn cutter_of(world: &World, entity: &Entity) -> Option<SpunProfile> {
    entity.entity_view(world).try_get::<&ToolCutter>(|c| c.spun.clone())
}

/// Land (`Some`) or clear (`None`) a tool's drawn cutter - the one
/// writer, called by `SetToolCutter`, the tool snapshots and the loader.
pub fn set_cutter(world: &World, entity: &Entity, spun: Option<&SpunProfile>) {
    let view = entity.entity_view(world);
    match spun {
        Some(s) => {
            view.set(ToolCutter { spun: s.clone() });
        }
        None => {
            view.remove(world.component::<ToolCutter>());
        }
    }
}

/// What the side view draws for `tool`: its drawn cutter when there is
/// one that agrees with these numbers, else the outline the numbers
/// draw. `None` for a turning tool.
#[must_use]
pub fn shown(tool: &Tool, drawn: Option<&SpunProfile>) -> Option<SpunProfile> {
    drawn
        .filter(|s| cutter_profile::agrees(tool, s).is_ok())
        .cloned()
        .or_else(|| SpunProfile::cutter_of(tool))
}

/// The drawn neck the stack laws read for the tool on `entity` (#852):
/// its drawn cutter's bands over the shank's span where narrower than
/// the stated shank, judged against `stored` - the tool record as stored,
/// the numbers the outline was drawn to. Empty (the stated shank) with
/// no drawn cutter, one that no longer agrees, or no neck.
#[must_use]
pub fn neck_of(world: &World, entity: &Entity, stored: &Tool) -> Vec<HolderBand> {
    cutter_of(world, entity).map(|s| neck::neck_bands(stored, &s)).unwrap_or_default()
}

/// The neck's term in an operation's stamp: nothing without one (no
/// digest moves), its bands with one - the only numbers a law reads.
pub fn digest(world: &World, entity: &Entity, stored: &Tool, h: &mut impl std::hash::Hasher) {
    let bands = neck_of(world, entity, stored);
    if bands.is_empty() {
        return;
    }
    h.write(b"tool-neck");
    for b in bands {
        for v in [b.above, b.len, b.r] {
            h.write(&v.to_bits().to_le_bytes());
        }
    }
}
