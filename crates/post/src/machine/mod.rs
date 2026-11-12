//! The machine-definition seam: `.mch` grammar as data (`def`) and the
//! file's reader (`parse`) - the same facts `post::machine::mirror`
//! restates for scripts. Pure serde; no world, no units glue (that is
//! the app's `machine` module, which re-exports all of this).

pub mod def;
mod direction;
pub mod edm;
mod edm_conditions;
mod kind;
mod no_spindle;
mod parse;
mod plasma;
mod rows;
mod swing;
mod tooling;

pub use def::*;
pub use direction::{inverted_axis_line, inverted_motion, inverted_phrase, untranslated_inversion, AxisDirection};
pub use edm::{edm_refusals, Edm, OrbitWord, ORBIT_KINDS, SINKER_KEYS, WIRE_KEYS};
pub use edm_conditions::{Condition, ConditionField, CONDITION_KEYS, CONDITION_ROLES, MAX_WIRE_MM, WIRE_MATCH_MM};
pub use parse::{parse_mch, parse_mch_with};
pub use plasma::plasma_refusals;
pub use tooling::{mount_check, tooling_refusals, HolderInfo, HolderLookup, SPINDLE_MOUNTS};
