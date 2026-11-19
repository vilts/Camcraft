//! Operation commands: create, edit, delete, generate, and clear the generated path.
//!
//! `GenerateOperation` is where this layer's central design choice lives:
//! generation runs in the constructor, while the world is being read
//! anyway and errors can still refuse the command (validate early). The
//! resulting path, warnings and stamp become plain fields, so `apply` and
//! `invert` are pure component writes - an undo of an unrelated edit and
//! a redo of the generation replay the identical bytes, no re-run.
//!
//! Provenance: our own domain model, following the same command pattern
//! as the import/stock siblings.

mod clear;
mod create;
mod edit;
mod gates;
mod generate;
mod geometry;
mod meta;

pub use clear::ClearToolpath;
pub use create::{CreateOperation, DeleteOperation, PasteOperation};
pub use edit::{ChangeOperationKind, EditOperationLinks, EditOperationParams};
pub use generate::{GenerateOperation, RegenerateAll, StagedGeneration};
pub use geometry::SetOperationGeometry;
pub use meta::{MarkOperationReviewed, RenameOperation, ReorderOperation};
