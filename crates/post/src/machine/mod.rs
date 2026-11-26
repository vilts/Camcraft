//! The machine-definition seam: `.mch` grammar as data (`def`) and the
//! file's reader (`parse`) - the same facts `post::machine::mirror`
//! restates for scripts. Pure serde; no world, no units glue (that is
//! the app's `machine` module, which re-exports all of this).
//! [`MachineCheck`] judges program values without a document or ECS world.

pub mod def;
mod direction;
pub mod edm;
pub mod edm_codes;
mod edm_conditions;
pub mod edm_dielectric;
mod edm_named;
pub mod edm_taper;
pub mod envelope;
mod operator_side;
pub use operator_side::OperatorSide;
mod kind;
pub(crate) mod installation;
pub use installation::{ControllerIdentity, ToolChangeMode};
mod no_spindle;
mod parse;
mod plasma;
mod post_name;
mod waterjet;
pub use waterjet::waterjet_refusals;
mod rows;
mod swing;
mod tooling;
pub mod reservations;
mod tree;
mod trunnion;

pub use def::*;
pub use direction::{inverted_axis_line, inverted_motion, inverted_phrase, untranslated_inversion, AxisDirection};
pub use edm::{edm_refusals, Edm, OrbitWord, ORBIT_KINDS, SINKER_KEYS, WIRE_KEYS};
pub use edm_codes::SINKER_CODES;
pub use edm_dielectric::{Flushing, Tank, FLUSHING_WORDS, TANK_WORDS};
pub use edm_conditions::{
    condition_refusal, id_key as condition_id_key, wear_rate_ok, Condition, ConditionField, CONDITION_KEYS, CONDITION_ROLES,
    MAX_WIRE_MM, WIRE_MATCH_MM,
};
pub use edm_taper::{AngleSide, TaperAngle, TaperCorner};
pub use parse::{parse_mch, parse_mch_with};
pub use plasma::plasma_refusals;
pub use post_name::{parse_post_name, post_name_refusal};
pub use tooling::{mount_check, tooling_refusals, HolderInfo, HolderLookup, SPINDLE_MOUNTS};
pub use tree::{parse_mch_tree, MachineTree, MovingSide, PartKind, SpindleBody, ToolStation, TreePart};
pub use trunnion::{trunnion_refusals, RotarySignConvention};

mod check;
mod judge;
pub use check::{MachineCheck, SourceLocation, REPORT_CAP};
// Shared with the app-specific plasma, waterjet, EDM and posted-lathe checks.
pub use judge::{fine_bore_gate, move_extremes, say, slowest_axis, tap_gate};
