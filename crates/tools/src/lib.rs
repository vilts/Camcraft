//! camcraft-tools - the tool-catalog seam of the M4.1 crate split
//! (R2, docs/M4.1-CRATE-SPLIT.md). Just the plain types: the R0 audit
//! proved tools.rs imports nothing (not even geometry), and it still
//! holds - the entity wrappers live in the app's schema, the picker
//! and sheet rendering in commands/tool and tool_list.
//!
//! Provenance: end mill / ball nose / bull nose shapes and the
//! coatings/carbide taxonomy are general tool-catalog practice from
//! public machine-tool handbooks and ISO 13399 concepts; nothing from
//! any vendor catalog file. The catalog is user data, not code -
//! this crate only says what a tool record IS.

pub mod catalog_asset;
#[cfg(test)]
mod catalog_asset_tests;
pub mod cutting;
// #368: a modelled tool body as the stack laws read it.
pub mod holder_profile;
// #793: the built-in BT/CAT taper holder library, drawn as spun profiles.
pub mod holder_library;
// #93: a drawn revolved (spun) profile for milling tools and holders.
pub mod spun;
// #93 slice 2: a milling tool's own drawn cutter, checked against its numbers.
pub mod cutter_profile;
// #93 slice 3: the drawn cutter edited by its parameters (refit, neck).
pub mod cutter_fit;
// #852: a necked tool's drawn shank, as the stack laws judge it.
pub mod neck;
// #369: a lathe-held body as the revolved-stock law reads it.
pub mod lathe_holder;
// #396: the insert plate a lathe body's judge leaves out.
pub mod lathe_insert_zone;
// #630: the lathe tool orientation code and its control point.
pub mod orientation;
pub mod tools;
// #891: turning surface speed, feed per rev and the max-rpm clamp.
pub mod turning_data;
