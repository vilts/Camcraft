//! The v53 -> v54 one-WCS rung (#342): a setup has exactly one work
//! coordinate system, and `G54`..`G59` is only the word the post writes
//! for it. The loader never silently re-points a program zero.
//!
//! - **What an older setup posted.** Under the v53 law every operation
//!   of a setup posted at the setup's pick (else `G54`), measured from
//!   the entity the setup's slot list named for that slot (else
//!   `<setup>/<word>`). That entity becomes the setup's one WCS, by name
//!   and placement unchanged - so a single-WCS document posts byte for
//!   byte as it did.
//! - **Slots nobody posted are dropped, with a note.** A placed offset
//!   no setup posted cut nothing; it goes, and the load says which ones
//!   (and where they sat), so the user hears of it.
//! - **Documents without setups** posted `G54` always: the `G54` entity
//!   stays their WCS, every other slot is dropped with a note.
//! - **Operations naming a setup the file does not hold** posted at
//!   `<setup>/G54`; that entity is renamed `<setup>/WCS` (what the v54
//!   law resolves for them), so they keep their zero.
//! - **Two origins in one setup cannot be carried and are refused.** A
//!   v51-or-older file whose operations measured from different offsets
//!   inside one setup is refused by name by the v52 rung
//!   (`wcs_migrate`) before this one runs; under v52/v53 one setup could
//!   only reach one origin, so nothing here needs to choose.
//! - **A v54 file names one WCS per setup**; one still listing slots was
//!   edited by hand and is refused as corrupt rather than guessed at.
//!
//! Provenance: our own persistence contract and design notes over RS-274
//! / ISO 6983 work-offset semantics; no vendor material consulted.

use std::collections::BTreeSet;

use super::{EntityKind, ProjectError, ProjectFile, WcsDto, corrupt};
use crate::schema::LEGACY_WCS_NAMES;

/// Run the rung on a file read at version `from` (already stepped to 53
/// or read at 54+). Returns the load notes.
pub(super) fn settle(file: &mut ProjectFile, from: u32) -> Result<Vec<String>, ProjectError> {
    if from >= 54 {
        for dto in &file.entities {
            if let Some(s) = &dto.setup
                && matches!(s.wcs, WcsDto::Slots(_))
            {
                return Err(corrupt(format!(
                    "setup '{}' lists work offset slots - since v54 a setup names its one WCS",
                    dto.name
                )));
            }
        }
        return Ok(Vec::new());
    }
    // Each setup's posted entity (module header).
    let mut keep: BTreeSet<String> = BTreeSet::new();
    let mut setups: BTreeSet<String> = BTreeSet::new();
    for dto in &mut file.entities {
        if dto.kind != Some(EntityKind::Setup) {
            continue;
        }
        let name = dto.name.clone();
        let Some(s) = dto.setup.as_mut() else {
            continue; // validate_kind refuses it by name later
        };
        let slot = match s.work_offset.as_deref() {
            None => 0,
            Some(w) => LEGACY_WCS_NAMES.iter().position(|x| *x == w).ok_or_else(|| {
                corrupt(format!("setup '{name}' picks unknown work offset {w:?}"))
            })?,
        };
        let list = match &s.wcs {
            WcsDto::Slots(l) => l.clone(),
            WcsDto::One(w) if w.is_empty() => Vec::new(),
            WcsDto::One(_) => {
                return Err(corrupt(format!(
                    "setup '{name}' names one WCS in a v{from} file - that shape arrived in v54"
                )));
            }
        };
        let posted = list
            .get(slot)
            .filter(|w| !w.is_empty())
            .cloned()
            .unwrap_or_else(|| format!("{name}/{}", LEGACY_WCS_NAMES[slot]));
        keep.insert(posted.clone());
        setups.insert(name);
        s.wcs = WcsDto::One(posted);
    }
    if setups.is_empty() {
        keep.insert(LEGACY_WCS_NAMES[0].to_owned());
    }
    // Operations in a setup the file does not hold posted at
    // `<setup>/G54`; the v54 law reads `<setup>/WCS` for them.
    let dangling: BTreeSet<String> = file
        .entities
        .iter()
        .filter_map(|d| d.operation.as_ref()?.setup.clone())
        .filter(|s| !setups.contains(s))
        .collect();
    let mut renames: Vec<(String, String)> = Vec::new();
    for s in &dangling {
        let old = format!("{s}/{}", LEGACY_WCS_NAMES[0]);
        let new = format!("{s}/{}", crate::setup::WCS_SUFFIX);
        if file.entities.iter().any(|d| d.name == new) {
            return Err(corrupt(format!("'{new}' already exists - cannot carry '{old}' onto it")));
        }
        renames.push((old.clone(), new.clone()));
        keep.insert(new);
    }
    for dto in &mut file.entities {
        if dto.kind == Some(EntityKind::Wcs)
            && let Some((_, new)) = renames.iter().find(|(old, _)| *old == dto.name)
        {
            dto.name = new.clone();
        }
    }
    let mut notes = Vec::new();
    file.entities.retain(|dto| {
        // Only a well-formed offset row is dropped; a malformed one stays
        // for `validate_kind` to refuse by name (a corrupt file is never
        // laundered by the migration).
        if dto.kind != Some(EntityKind::Wcs) || keep.contains(&dto.name) || !well_formed(dto) {
            return true;
        }
        let [x, y, z] = dto.translation;
        notes.push(format!(
            "dropped work offset '{}' (X{x:.3} Y{y:.3} Z{z:.3}): no setup posted it - since v54 a \
             setup has one WCS, and G54-G59 is only the work offset its post uses",
            dto.name
        ));
        false
    });
    Ok(notes)
}

/// A pre-v54 offset row the validator would accept: a slot name (bare or
/// under a setup) and no solid, tool, operation or setup payload.
fn well_formed(dto: &super::EntityDto) -> bool {
    let named = LEGACY_WCS_NAMES.contains(&dto.name.as_str())
        || dto.name.rsplit_once('/').is_some_and(|(_, last)| LEGACY_WCS_NAMES.contains(&last));
    named
        && dto.facets.is_none()
        && dto.stock.is_none()
        && dto.tool.is_none()
        && dto.operation.is_none()
        && dto.setup.is_none()
}
