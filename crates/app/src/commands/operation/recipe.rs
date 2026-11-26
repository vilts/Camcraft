//! ApplyCutRecipe (#3073): make a new operation from a saved cut recipe
//! on new geometry, as one undoable step. The recipe's tool must still
//! be in the document and its material must still be in the table and
//! agree with the material in force; else it is refused with the
//! recipe's name. Every creation gate then applies through
//! `CreateOperation`, so a recipe can never make an operation the
//! property panel could not.
//!
//! Provenance: our own command, a thin composite over CreateOperation.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::cut_recipe::CutRecipe;
use crate::model;
use crate::operation::{OpParams, SrcRef};

use super::CreateOperation;

/// One undo step: the operation the recipe made.
pub struct ApplyCutRecipe(CreateOperation);

impl ApplyCutRecipe {
    /// Operation `name` from `recipe` on `src`, in the active setup.
    pub fn new(world: &World, recipe: &CutRecipe, name: &str, src: SrcRef) -> Result<Self, CommandError> {
        let r = &recipe.name;
        if !recipe.tool.is_empty() {
            let found = model::find_by_name(world, &recipe.tool).is_some_and(|e| model::is_tool(world, &e));
            if !found {
                return Err(CommandError::Needs(format!(
                    "cut recipe '{r}': its tool '{}' is not in the document - add it, or save the recipe again with a tool that is",
                    recipe.tool
                )));
            }
        }
        let wanted = recipe.material().map_err(CommandError::Needs)?;
        let setup = crate::setup::active(world);
        if let Some(wanted) = wanted {
            let here = crate::stock_material::in_force(world, setup.as_deref());
            if here != Some(wanted) {
                return Err(CommandError::Needs(format!(
                    "cut recipe '{r}' is for {} ('{}'), but this setup has {} - set a matching material before applying it",
                    wanted.label(),
                    wanted.word(),
                    here.map_or("no material", |m| m.label())
                )));
            }
        }
        if has_saved_geometry(&recipe.params) {
            return Err(CommandError::Needs(format!(
                "cut recipe '{r}' contains saved geometry - supply geometry for the new source in a new operation"
            )));
        }
        CreateOperation::with_named_entry(world, name, &recipe.tool, src, recipe.params.clone(), true).map(Self)
    }
}

impl Command for ApplyCutRecipe {
    fn apply(&self, world: &World) {
        self.0.apply(world);
    }

    fn invert(&self, world: &World) {
        self.0.invert(world);
    }
}

/// Coordinates and entity picks belong to the source operation, not the
/// destination. The recipe API has no replacement geometry input yet.
fn has_saved_geometry(params: &OpParams) -> bool {
    if params.tilt().is_some() {
        return true;
    }
    // A picked start or exit point (#1448, #1473) is a source coordinate.
    if !params.path_picks().is_empty() {
        return true;
    }
    match params {
        OpParams::Drill { holes, cross, .. } => !holes.is_empty() || !cross.is_empty(),
        OpParams::Bore { holes, .. } | OpParams::Thread { holes, .. } => !holes.is_empty(),
        OpParams::Contour { chain_overrides, .. } => !chain_overrides.is_empty(),
        OpParams::Deburr { exclude_edges, .. } | OpParams::MultiAxisDeburr { exclude_edges, .. } => !exclude_edges.is_empty(),
        OpParams::Circular { .. }
        | OpParams::Slot { .. }
        | OpParams::TrochoidalSlot { .. }
        | OpParams::Probe { .. }
        | OpParams::SlotUndercut(_)
        | OpParams::WireContour(_)
        | OpParams::SinkerBurn(_)
        | OpParams::SinkerSideBurn(_) => true,
        // A per-corner relief override is matched by a source corner XY.
        // The global relief kind and T-bone rule carry no coordinate.
        OpParams::Pocket { corner_relief, .. }
        | OpParams::Adaptive2D { corner_relief, .. }
        | OpParams::Offset2D { corner_relief, .. } => !corner_relief.overrides.is_empty(),
        // A tab position is a world X, Y of the source contour (#1883).
        // The other waterjet fields carry no coordinate.
        OpParams::WaterjetProfile(op) => op.tabs.as_ref().is_some_and(|t| !t.at.is_empty()),
        OpParams::Face { .. }
        | OpParams::Engrave { .. }
        | OpParams::VCarve { .. }
        | OpParams::Chamfer2D { .. }
        | OpParams::Trace { .. }
        | OpParams::Pocket3D { .. }
        | OpParams::Finish3D { .. }
        | OpParams::Contour5D { .. }
        | OpParams::Parallel5D { .. }
        | OpParams::Swarf { .. }
        | OpParams::AdvancedSwarf { .. }
        | OpParams::RotaryContour { .. }
        | OpParams::RotaryParallel { .. }
        | OpParams::RotaryOffset { .. }
        | OpParams::MultiAxisRough { .. }
        | OpParams::Flow5D { .. }
        | OpParams::MultiAxisContour { .. }
        // The flow band's guide chains are entity picks on the op's own
        // selection law (op.src), not params coordinates (#2826).
        | OpParams::RotaryFlow { .. }
        | OpParams::Facing { .. }
        | OpParams::ProfileRough { .. }
        | OpParams::ProfileFinish { .. }
        | OpParams::GrooveRough { .. }
        | OpParams::GrooveFinish { .. }
        | OpParams::Part { .. }
        | OpParams::Threading { .. }
        | OpParams::Chamfering { .. }
        | OpParams::TaperTurning { .. }
        | OpParams::MultiPassTurning { .. }
        | OpParams::FormTurning { .. }
        | OpParams::BoringTurning { .. }
        | OpParams::PlasmaProfile(_) => false,
    }
}
