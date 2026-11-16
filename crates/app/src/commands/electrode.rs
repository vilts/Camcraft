//! `ElectrodeFromFaces` (#547): a sinker EDM electrode generated from
//! the picked faces of a part's cavity, added to the document as a tool
//! (#541's electrode kind) in one undo step.
//!
//! Provenance: our own command model - validation at construction,
//! `apply` a pure world write, the inverse `AddTool`'s own - over the
//! derivation in `operation::sinker_faces` (whose sources its module
//! head and the motion crate's cite). No vendor material.

use flecs_ecs::prelude::World;

use crate::commands::{AddTool, Command, CommandError};
use crate::operation::sinker_faces::electrode_from_faces;
use crate::operation::GeoSel;
use crate::sinker_electrode::{ElectrodeBox, ElectrodeSpec};

/// Generate the electrode a picked cavity calls for and add it as the
/// tool `name` - a rectangular electrode for an axis-aligned rectangular
/// cavity, a body electrode (#1152) for any other outline. Every
/// refusal - a face set that is no single cavity, a garbled undersize
/// or stub, a taken name - is
/// made here, by name, before anything enters the history.
pub struct ElectrodeFromFaces {
    add: AddTool,
    electrode: ElectrodeBox,
}

impl ElectrodeFromFaces {
    /// # Errors
    /// The derivation's refusals (`CommandError::Invalid`, in its own
    /// words) and `AddTool`'s (a reserved or taken name).
    pub fn new(world: &World, name: &str, picks: &[GeoSel], spec: &ElectrodeSpec) -> Result<Self, CommandError> {
        let electrode = electrode_from_faces(world, picks, spec).map_err(CommandError::Invalid)?;
        // A sinker electrode rides no milling holder: it lands bare.
        let add = AddTool::new(world, name, electrode.tool())?;
        Ok(Self { add, electrode })
    }

    /// What was derived: the electrode's face and length, and the
    /// cavity's centre, size and heights - the numbers a Sinker Burn of
    /// this cavity states.
    #[must_use]
    pub fn electrode(&self) -> &ElectrodeBox {
        &self.electrode
    }
}

impl Command for ElectrodeFromFaces {
    fn apply(&self, world: &World) {
        self.add.apply(world);
    }

    fn invert(&self, world: &World) {
        self.add.invert(world);
    }
}
