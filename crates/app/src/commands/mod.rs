//! The command pattern: every persistent world mutation is an invertible
//! command, and the history is the single source of truth for undo/redo
//! (AGENTS.md). Parsing and validation happen at command *construction*, so
//! malformed input never enters the history ("validate early").

// M13.2: workpiece material + the feeds-and-speeds suggestion.
pub mod accessory;
pub mod builtin_vise;
pub mod cutting_data;
pub mod delete_entity;
pub mod fixture;
pub mod fixture_move;
pub mod fixture_plate;
pub mod mod_vise;
// #832: edit and re-fit a placed mod vise pair.
pub mod mod_vise_edit;
pub mod import_fixture;
pub mod import_model;
// #339 slice 6: a fixture library entry placed into a setup.
pub mod insert_fixture;
pub mod load_machine;
// #561: locate the part in a fixture (or the fixture on the part) by mates.
pub mod locate;
pub mod move_model;
pub mod operation;
pub mod post;
pub mod post_options;
// #155: Pause / Home entries of the operation list.
pub mod program_entry;
// M13.11: mesh repair (weld + fill holes) as one undo unit over UpdateModel.
pub mod repair_model;
// Issue #143: ribbon pins are saved with the project, so they are a command.
pub mod ribbon_pins;
pub mod select_entity;
pub mod set_stock;
pub mod stock_source_only;
pub mod setup;
pub mod setup_delete;
// #196: a setup's picked program offset.
pub mod setup_align;
pub mod setup_wcs;
// #343: a setup's own machine and post.
pub mod setup_mount;
pub mod setup_post_options;
pub mod setup_workholding;
// #316: a setup placed relative to another (flip / turn presets).
pub mod setup_relative;
// #320: a setup's stock source.
pub mod setup_stock;
pub mod set_units;
// M6.1 C / #877: the theme, a user preference (not undoable, not saved
// with the project - `app::prefs` keeps it).
pub mod theme;
// M13.4 / #877: the viewer's colour-vision remap, the theme's twin.
pub mod color_vision;
pub mod set_wcs;
pub mod tool;
// #368: a tool's modelled body.
pub mod tool_body;
// #93 slice 2: a milling tool's drawn cutter outline.
pub mod tool_cutter;
// #300: a tool library file imported as one undo unit.
pub mod tool_import;
// #221: a tool's controller number.
pub mod tool_number;
pub mod update_model;

use flecs_ecs::prelude::World;

pub use accessory::{AccessoryPlacement, InsertAccessory, SetAccessory};
pub use builtin_vise::{InsertVise, SetVise, VisePlacement};
pub use cutting_data::{ApplyCuttingData, SetWorkMaterial};
pub use delete_entity::DeleteEntity;
pub use fixture::{
    AddFixtureModel, DeleteFixture, MoveFixture, SetFixtureFixed, TagFixture, UntagFixture,
};
pub use fixture_plate::{InsertPlate, PlaceOnPlate, PlatePlacement, SetPlate, SetPlateGrid};
pub use mod_vise::PlaceModVisePair;
pub use mod_vise_edit::{RefitModVise, SetModJaw};
pub use import_fixture::ImportFixture;
pub use import_model::ImportModel;
pub use insert_fixture::InsertFixture;
pub use load_machine::LoadMachine;
pub use locate::{Locate, LocateFixture, LocatePart};
pub use move_model::MoveModel;
pub use operation::{
    ChangeOperationKind, ClearToolpath, CreateOperation, DeleteOperation, EditOperationLinks,
    EditOperationParams,
    GenerateOperation, PasteOperation, RegenerateAll, RenameOperation, ReorderOperation, SetOperationGeometry,
};
pub use post::SelectPost;
pub use post_options::SetPostOptions;
pub use program_entry::{AddProgramEntry, EditProgramEntry};
pub use repair_model::RepairModel;
pub use ribbon_pins::SetRibbonPins;
pub use select_entity::SelectEntity;
pub use set_stock::SetStock;
pub use stock_source_only::SetStockSourceOnly;
pub use setup::{ActivateSetup, CreateSetup, DeleteSetup, SetOperationSetup};
pub use setup_align::AlignPartToSetup;
pub use setup_wcs::SetSetupWorkOffset;
pub use setup_mount::{SetSetupMachine, SetSetupPost};
pub use setup_post_options::SetSetupPostOptions;
pub use setup_workholding::SetSetupWorkholdingRpm;
pub use setup_relative::SetSetupRelative;
pub use setup_stock::SetSetupStockSource;
pub use set_units::SetUnits;
pub use theme::{check_theme, theme_of, theme_set};
pub use color_vision::{color_vision_of, color_vision_set};
pub use set_wcs::SetWcsOrigin;
pub use tool::{AddTool, ChangeTool, DeleteTool, HolderPick};
pub use tool_body::{LatheBodySpec, SetToolBody, ToolBodySpec};
pub use tool_cutter::{ReshapeTool, SetToolCutter};
pub use tool_import::{ImportMode, ImportReport, ImportTools};
pub use tool_number::SetToolNumber;
pub use update_model::UpdateModel;

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("no entity named '{0}'")]
    NotFound(String),
    #[error("an entity named '{0}' already exists")]
    NameConflict(String),
    #[error("invalid parameters: {0}")]
    Invalid(String),
    /// A precondition the user can fix, said in plain English with what to
    /// do next (#1105): shown as is, with no "invalid parameters:" prefix.
    #[error("{0}")]
    Needs(String),
    #[error(transparent)]
    Import(#[from] crate::geometry::ImportError),
    #[error("'{0}' import is not supported yet; import a STEP, STL, or OBJ file instead")]
    UnsupportedType(String),
    #[error("could not generate the toolpath: {0}")]
    Machining(String),
    /// A [`Machining`](CommandError::Machining) refusal about one spot
    /// of the part (#261 part 2): `at` is world coordinates. Reads as
    /// `Machining` does.
    #[error("could not generate the toolpath: {msg}")]
    MachiningAt { msg: String, at: crate::geometry::Point3 },
    /// A [`Machining`](CommandError::Machining) refusal that handed back
    /// the moves it had built (#261 part 3), world coordinates, with the
    /// spot it names if any. Display-only: the moves are never cached,
    /// so the operation still has no toolpath. Reads as `Machining` does.
    #[error("could not generate the toolpath: {msg}")]
    MachiningGhost { msg: String, at: Option<crate::geometry::Point3>, ghost: crate::machining2d::Ghost },
}

impl CommandError {
    /// The spot a refused generation names, in world coordinates.
    #[must_use]
    pub fn location(&self) -> Option<crate::geometry::Point3> {
        match self {
            CommandError::MachiningAt { at, .. } => Some(*at),
            CommandError::MachiningGhost { at, .. } => *at,
            _ => None,
        }
    }

    /// The moves a refused generation had built (#261 part 3), for the
    /// viewport's diagnostic ghost - never a toolpath.
    #[must_use]
    pub fn ghost(&self) -> Option<&crate::machining2d::Ghost> {
        match self {
            CommandError::MachiningGhost { ghost, .. } => Some(ghost),
            _ => None,
        }
    }
}

pub trait Command {
    /// Forward effect.
    fn apply(&self, world: &World);
    /// Exact inverse of `apply`.
    fn invert(&self, world: &World);
}

/// Linear undo/redo over executed commands. Executing a fresh command clears
/// the redo stack (standard history semantics).
///
/// The history also knows whether the document has unsaved changes (#107):
/// `saved` is the undo depth at the last save or load. The document is
/// clean exactly when the current depth equals it - so undoing back to the
/// saved point is clean again, and redoing past it is dirty again. A fresh
/// command executed while the saved point sits on the redo stack discards
/// that state for good, so the saved point becomes unreachable (`None`).
pub struct History {
    done: Vec<Box<dyn Command>>,
    redo: Vec<Box<dyn Command>>,
    saved: Option<usize>,
}

impl Default for History {
    fn default() -> Self {
        Self { done: Vec::new(), redo: Vec::new(), saved: Some(0) }
    }
}

impl History {
    pub fn execute(&mut self, world: &World, command: impl Command + 'static) {
        tracing::debug!(command = command_name(&command), "execute");
        if self.saved.is_some_and(|depth| depth > self.done.len()) {
            self.saved = None;
        }
        command.apply(world);
        self.done.push(Box::new(command));
        self.redo.clear();
    }

    pub fn undo(&mut self, world: &World) -> bool {
        match self.done.pop() {
            Some(command) => {
                tracing::debug!("undo");
                command.invert(world);
                self.redo.push(command);
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self, world: &World) -> bool {
        match self.redo.pop() {
            Some(command) => {
                command.apply(world);
                self.done.push(command);
                true
            }
            None => false,
        }
    }

    /// Run a command without touching either stack. Selection is a command
    /// (AGENTS.md) but is deliberately not on the undo timeline.
    pub fn execute_ephemeral(world: &World, command: impl Command) {
        command.apply(world);
    }

    /// Drop all history; used when a document replaces the current one.
    /// The replacing document (blank or loaded) starts clean.
    pub fn clear(&mut self) {
        self.done.clear();
        self.redo.clear();
        self.saved = Some(0);
    }

    /// Record the current position as the saved state (after a save or a
    /// load succeeded).
    pub fn mark_clean(&mut self) {
        self.saved = Some(self.done.len());
    }

    /// True when the document differs from the last save or load: the
    /// history position is not the recorded saved position.
    pub fn is_dirty(&self) -> bool {
        self.saved != Some(self.done.len())
    }

    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn is_empty(&self) -> bool {
        self.done.is_empty() && self.redo.is_empty()
    }
}

/// A command's short type name for the dispatch log (#163):
/// `camcraft_app::commands::set_wcs::SetWcsOrigin` reads `SetWcsOrigin`.
fn command_name<T>(_: &T) -> &'static str {
    let full = std::any::type_name::<T>();
    full.rsplit("::").next().unwrap_or(full)
}
