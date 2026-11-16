//! The work-offset slot law (#196, #329, #931): which work offset (a
//! 1-based index, the post's word for it) an operation posts at, and the
//! one-file refusal a per-setup pick brings.
//!
//! - **The setup governs.** Every operation of a setup posts at the
//!   setup's work-offset word (`SetupDef::work_offset`, set by
//!   `SetSetupWorkOffset`; unlabelled = `G54`), measured from the setup's
//!   one WCS (#342) - [`crate::setup::wcs_slot`] is the word law. A
//!   height measured from the work offset reads the same WCS (#329), so
//!   an operation cannot disagree with its setup by construction.
//! - **One program, one register per word.** A single NC file in which
//!   two setups post the same word with different placements would run
//!   the second setup in the first one's zero - the controller holds one
//!   `G54`. The one-file post refuses it by name; per-operation files
//!   are exempt (each file is one setup's, and its offset is set for it).
//!
//! Provenance: our own design notes over RS-274 / ISO 6983 work-offset
//! semantics (G54..G59 select one of six controller-held zero registers);
//! no vendor material consulted.

use flecs_ecs::prelude::World;

use crate::model;
use crate::schema::Location;

/// Placement difference below which two offsets are the same zero:
/// 1 nm, far under any machine resolution.
const SAME_MM: f64 = 1e-6;

/// The work-offset slot an operation of the `setup` context posts at -
/// the setup's ([`crate::setup::wcs_slot`]). [`crate::posting::wcs_context`]
/// and the one-file clause read it.
pub(crate) fn wcs_slot(world: &World, setup: Option<&str>) -> usize {
    crate::setup::wcs_slot(world, setup)
}

/// The offset's display name in a setup context's post (#931): what
/// the refusal names; the comparison is by index, never by this text.
fn word(world: &World, setup: Option<&str>, slot: usize) -> String {
    let n = u32::try_from(slot).map_or(u32::MAX, |s| s.saturating_add(1));
    crate::setup::offset_name(world, setup, n)
}

/// The placement a setup's WCS sits at (unplaced = the origin, the law
/// `wcs_context` reads).
fn placement(world: &World, setup: Option<&str>) -> Location {
    crate::setup::wcs_entity(world, setup)
        .and_then(|e| model::transform_of(world, &e))
        .unwrap_or_default()
}

fn same_zero(a: &Location, b: &Location) -> bool {
    let pos = (a.pos.x - b.pos.x).abs() <= SAME_MM
        && (a.pos.y - b.pos.y).abs() <= SAME_MM
        && (a.pos.z - b.pos.z).abs() <= SAME_MM;
    // q and -q are one rotation.
    let rot = |s: f64| (0..4).all(|i| (a.rot[i] - s * b.rot[i]).abs() <= 1e-9);
    pos && (rot(1.0) || rot(-1.0))
}

/// The one-file clause (module header): one line per offset INDEX that
/// two setups of the program post at from different placements (#931:
/// indexes compare, so two posts naming one offset differently are still
/// one register). Empty when every offset means one zero.
#[must_use]
pub(super) fn shared_word_problems(world: &World) -> Vec<String> {
    shared_word_problems_in(world, None)
}

/// [`shared_word_problems`] over one file's setups (#388: a mount's
/// file holds its own setups' registers, another machine's are its own).
/// `None` is the whole program.
#[must_use]
pub(super) fn shared_word_problems_in(world: &World, only: Option<&[String]>) -> Vec<String> {
    // (slot, setup, placement), first use in program order.
    let mut seen: Vec<(usize, String, Location)> = Vec::new();
    let mut out: Vec<String> = Vec::new();
    for (_, e) in crate::setup::operation_order(world) {
        let Some(op) = model::operation_of(world, &e) else {
            continue;
        };
        if op.suppressed {
            continue;
        }
        let setup = op.setup.as_deref();
        let slot = wcs_slot(world, setup);
        let name = crate::setup::effective(world, setup).unwrap_or_default();
        if only.is_some_and(|keep| !keep.contains(&name)) {
            continue;
        }
        let here = placement(world, setup);
        match seen.iter().find(|(s, n, _)| *s == slot && *n != name) {
            Some((_, first, there)) if !same_zero(there, &here) => {
                let line = format!(
                    "setups {first} and {name} both post at {} from different placements \
                     ({first}: X{:.3} Y{:.3} Z{:.3}, {name}: X{:.3} Y{:.3} Z{:.3}) - one program \
                     holds one {} register; pick another work offset for {name} on the Setups \
                     page, or post one file per operation",
                    word(world, setup, slot),
                    there.pos.x,
                    there.pos.y,
                    there.pos.z,
                    here.pos.x,
                    here.pos.y,
                    here.pos.z,
                    word(world, setup, slot)
                );
                if !out.contains(&line) {
                    out.push(line);
                }
            }
            _ => {}
        }
        if !seen.iter().any(|(s, n, _)| *s == slot && *n == name) {
            seen.push((slot, name, here));
        }
    }
    out
}
