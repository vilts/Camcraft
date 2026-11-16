//! The v51 -> v52 work-offset rung (#329): a height measured from the
//! work offset stops naming its own slot - the setup's offset is program
//! zero for every operation in it. An older file's per-height slots move
//! onto their setup when the whole setup agreed, and the load is refused
//! by name when it did not: the loader never silently re-points a zero.
//!
//! - **What an older setup "named".** Under the v51 law an operation
//!   posted at its setup's pick, else the first slot its heights
//!   referenced, else `G54`; and each height was measured from its own
//!   slot. So a setup's slots are: its pick (if any), every operation's
//!   posted slot, and every slot a height referenced - suppressed
//!   operations included (their heights still generate). One slot: it
//!   becomes the setup's `work_offset` (left unwritten for `G54`, the
//!   default, so a G54 file keeps its byte shape). More than one: refused,
//!   naming each operation and the slots it used.
//! - **Documents without setups** (or an operation naming no setup row)
//!   have nowhere to carry a pick: they load when everything agrees on
//!   `G54` and are refused by name otherwise.
//! - **A v52 file never writes a slot**; one that carries it was edited
//!   by hand and is refused as corrupt rather than ignored.
//!
//! The slots are read from the operation DTOs' JSON (the domain height
//! has no slot to carry them), before any world write.
//!
//! Provenance: our own persistence contract and design notes over RS-274
//! / ISO 6983 work-offset semantics; no vendor material consulted.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::{EntityKind, ProjectError, ProjectFile, corrupt};
use crate::schema::LEGACY_WCS_NAMES;

fn word(slot: usize) -> String {
    LEGACY_WCS_NAMES.get(slot).map_or_else(|| format!("work offset #{slot}"), |w| (*w).to_owned())
}

/// Every `work_offset` slot a parameter JSON's heights name, in file
/// order (`{"reference": {"work_offset": {"slot": n}}}`).
pub(super) fn height_slots(v: &Value, out: &mut Vec<usize>) {
    match v {
        Value::Object(m) => {
            if let Some(slot) = m
                .get("reference")
                .and_then(|r| r.get("work_offset"))
                .and_then(|w| w.get("slot"))
            {
                // A slot that is not a small integer is out of range too.
                out.push(slot.as_u64().map_or(usize::MAX, |n| usize::try_from(n).unwrap_or(usize::MAX)));
            }
            m.values().for_each(|c| height_slots(c, out));
        }
        Value::Array(a) => a.iter().for_each(|c| height_slots(c, out)),
        _ => {}
    }
}

/// One operation's slots under the v51 law (module header): the posted
/// slot first, then every height's.
struct OpSlots {
    label: String,
    setup: Option<String>,
    slots: BTreeSet<usize>,
}

/// Run the rung on a file of version `from` (already stepped to 51 or
/// read at 52+). Older files migrate; v52 files are checked slot-free.
pub(super) fn settle(file: &mut ProjectFile, from: u32) -> Result<(), ProjectError> {
    let mut ops: Vec<OpSlots> = Vec::new();
    for dto in &file.entities {
        let Some(op) = dto.operation.as_ref() else {
            continue;
        };
        let json = serde_json::to_value(&op.params).map_err(|e| corrupt(e.to_string()))?;
        let mut heights = Vec::new();
        height_slots(&json, &mut heights);
        if from >= 52 {
            if !heights.is_empty() {
                return Err(corrupt(format!(
                    "operation '{}' names a work-offset slot on a height - since v52 the \
                     setup's work offset is the only one",
                    dto.name
                )));
            }
            continue;
        }
        if let Some(bad) = heights.iter().find(|i| **i >= LEGACY_WCS_NAMES.len()) {
            return Err(corrupt(format!(
                "operation '{}' measures a height from work offset #{bad} - the slots are G54 \
                 to G59",
                dto.name
            )));
        }
        ops.push(OpSlots {
            label: op.label.clone().unwrap_or_else(|| dto.name.clone()),
            setup: op.setup.clone(),
            slots: heights.into_iter().collect(),
        });
    }
    if from >= 52 {
        return Ok(());
    }
    lift(file, &ops)
}

/// The migration proper (module header).
fn lift(file: &mut ProjectFile, ops: &[OpSlots]) -> Result<(), ProjectError> {
    // Setup name -> (entity index, picked slot), name-sorted like
    // `setup::setups`; the first one is what `None` means.
    let mut setups: BTreeMap<String, (usize, Option<usize>)> = BTreeMap::new();
    for (i, dto) in file.entities.iter().enumerate() {
        if dto.kind != Some(EntityKind::Setup) {
            continue;
        }
        let pick = match dto.setup.as_ref().and_then(|s| s.work_offset.as_deref()) {
            None => None,
            Some(w) => Some(LEGACY_WCS_NAMES.iter().position(|s| *s == w).ok_or_else(|| {
                corrupt(format!("setup '{}' picks unknown work offset {w:?}", dto.name))
            })?),
        };
        setups.insert(dto.name.clone(), (i, pick));
    }
    let first = setups.keys().next().cloned();
    // Effective setup name (None = no setup row to carry a pick) -> the
    // operations in it with the slots each used.
    let mut groups: BTreeMap<Option<String>, Vec<(&str, BTreeSet<usize>)>> = BTreeMap::new();
    for op in ops {
        let name = op.setup.clone().or_else(|| first.clone()).filter(|n| setups.contains_key(n));
        let pick = name.as_ref().and_then(|n| setups.get(n)).and_then(|s| s.1);
        let posted = pick.or_else(|| op.slots.first().copied()).unwrap_or(0);
        let mut used = op.slots.clone();
        used.insert(posted);
        groups.entry(name).or_default().push((&op.label, used));
    }
    let mut problems = Vec::new();
    let mut lifts: Vec<(usize, usize)> = Vec::new();
    for (name, members) in &groups {
        let pick = name.as_ref().and_then(|n| setups.get(n)).and_then(|s| s.1);
        let mut all: BTreeSet<usize> = members.iter().flat_map(|(_, s)| s.iter().copied()).collect();
        all.extend(pick);
        let Some(&only) = all.iter().next().filter(|_| all.len() == 1) else {
            let ops = members
                .iter()
                .map(|(l, s)| format!("{l} ({})", s.iter().map(|i| word(*i)).collect::<Vec<_>>().join(", ")))
                .collect::<Vec<_>>()
                .join("; ");
            let picked = pick.map(|p| format!(" (setup picks {})", word(p))).unwrap_or_default();
            problems.push(format!(
                "{}{picked}: {ops}",
                name.as_ref().map_or_else(|| "the document (no setup)".to_owned(), |n| format!("setup {n}"))
            ));
            continue;
        };
        match name.as_ref().and_then(|n| setups.get(n)) {
            Some(&(idx, None)) if only != 0 => lifts.push((idx, only)),
            Some(_) => {}
            None if only != 0 => problems.push(format!(
                "the document has no setup to carry {} (operations: {}) - create a setup in the \
                 release that wrote this file, save, and reopen",
                word(only),
                members.iter().map(|(l, _)| *l).collect::<Vec<_>>().join(", ")
            )),
            None => {}
        }
    }
    if !problems.is_empty() {
        return Err(ProjectError::WorkOffsetConflict(problems.join("\n")));
    }
    for (idx, slot) in lifts {
        if let Some(s) = file.entities.get_mut(idx).and_then(|e| e.setup.as_mut()) {
            s.work_offset = LEGACY_WCS_NAMES.get(slot).map(|w| (*w).to_owned());
        }
    }
    Ok(())
}
