//! The file mirror of `machining2d::PassStart` (#3316): where each level's
//! passes start. One key, `pass_start`, for every kind that grows the
//! choice. Provenance: our own project-file format; no vendor material
//! consulted.

use serde::{Deserialize, Serialize};

use crate::machining2d::PassStart;

/// `auto` is the default and is never written, so every file before the
/// key reads the same and every file without a choice stays byte-exact
/// (no version step).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PassStartDto {
    #[default]
    Auto,
    Inside,
    Outside,
}

impl PassStartDto {
    /// The serde skip test: the default stays off the wire.
    #[must_use]
    pub fn is_auto(&self) -> bool {
        *self == PassStartDto::Auto
    }
}

impl From<PassStart> for PassStartDto {
    fn from(s: PassStart) -> Self {
        match s {
            PassStart::Auto => PassStartDto::Auto,
            PassStart::Inside => PassStartDto::Inside,
            PassStart::Outside => PassStartDto::Outside,
        }
    }
}

impl From<PassStartDto> for PassStart {
    fn from(s: PassStartDto) -> Self {
        match s {
            PassStartDto::Auto => PassStart::Auto,
            PassStartDto::Inside => PassStart::Inside,
            PassStartDto::Outside => PassStart::Outside,
        }
    }
}
