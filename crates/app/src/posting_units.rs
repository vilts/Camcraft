//! The program's unit (#902): the document's one mm / inch choice
//! ([`crate::units`]) is the unit every post writes its program in -
//! never the machine file's, a post option's or a setup's. Stated to
//! the script as `postOptions.Units` beside the user's post options;
//! the engine refuses an inch program on a script that cannot write
//! one (`post::units`).
//!
//! Provenance: RS-274 / ISO 6983 (G20 inch / G21 mm); the maintainer's
//! decision on #902 (the unit is a document setting). No vendor
//! material consulted.

use flecs_ecs::prelude::World;
use serde_json::Value;

use crate::units::{self, Unit};

/// The word a post reads for `unit`: `"mm"` or `"inch"`.
#[must_use]
pub fn unit_word(unit: Unit) -> &'static str {
    match unit {
        Unit::Millimeters => "mm",
        Unit::Inches => "inch",
    }
}

/// The options object a program posts with: the post options in force
/// ([`super::options_value`]) plus the document's unit as `Units`.
#[must_use]
pub fn program_options(world: &World) -> Value {
    let mut v = super::options_value(super::post_options(world));
    if let Some(map) = v.as_object_mut() {
        map.insert("Units".to_owned(), Value::from(unit_word(units::of(world))));
    }
    v
}
