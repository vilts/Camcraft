//! The project-file word of the Deburr unreachable-edge policy (#1889).
//!
//! Provenance: our own file mirror of `finishing3d::UnreachableEdges`
//! (the project file is this module's concern); no vendor material.

use serde::{Deserialize, Serialize};

use crate::finishing3d::UnreachableEdges;

/// `skip_with_note` (the default, and each file before #1889) or
/// `refuse`. A word that is not one of the two is refused at load.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UnreachableDto {
    #[default]
    SkipWithNote,
    Refuse,
}

impl From<UnreachableEdges> for UnreachableDto {
    fn from(p: UnreachableEdges) -> Self {
        match p {
            UnreachableEdges::SkipWithNote => UnreachableDto::SkipWithNote,
            UnreachableEdges::Refuse => UnreachableDto::Refuse,
        }
    }
}

impl From<UnreachableDto> for UnreachableEdges {
    fn from(d: UnreachableDto) -> Self {
        match d {
            UnreachableDto::SkipWithNote => UnreachableEdges::SkipWithNote,
            UnreachableDto::Refuse => UnreachableEdges::Refuse,
        }
    }
}
