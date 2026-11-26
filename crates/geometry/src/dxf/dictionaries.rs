//! Provenance: dictionary entries, handles, ownership and group 102 brackets in the Autodesk DXF Reference.
use super::entities::is_entity_kind;
use super::pairs::{Pair, Pairs, Record, handle, integer};
use super::types::{DxfError, error};
use std::collections::{BTreeMap, BTreeSet};

fn invalid(message: &str) -> DxfError {
    error("DICTIONARY", message)
}

/// Application-defined data in a `102 {NAME` .. `102 }` bracket, for example reactor handles.
/// The content is opaque. A bracket is never nested and closes before its record ends.
#[derive(Default)]
struct AppGroup {
    open: bool,
}

impl AppGroup {
    /// True when the pair is a bracket marker or bracket content, not a field of the record.
    fn consume(&mut self, kind: &str, pair: Pair<'_>) -> Result<bool, DxfError> {
        if pair.code != 102 {
            return Ok(self.open);
        }
        let marker = pair.value.trim();
        if marker == "}" {
            if !self.open {
                return Err(error(kind, "group 102 closes no open block"));
            }
            self.open = false;
        } else if marker.len() > 1 && marker.starts_with('{') {
            if self.open {
                return Err(error(kind, "nested group 102 block"));
            }
            self.open = true;
        } else {
            return Err(error(kind, "malformed group 102 block marker"));
        }
        Ok(true)
    }

    fn finish(&self, kind: &str) -> Result<(), DxfError> {
        if self.open {
            return Err(error(kind, "unclosed group 102 block"));
        }
        Ok(())
    }
}

/// Read OBJECTS. A DICTIONARY is checked field by field. Every other object is inert: the
/// reader uses no object data, so it reads only the common header (handle, group 102
/// brackets, owner) and consumes the rest to the next record. Each object joins the
/// ownership graph, so a stray or unowned record is still refused.
pub(super) fn read(input: &mut Pairs<'_>, limit: usize) -> Result<(), DxfError> {
    let mut owners = BTreeMap::new();
    let mut entries = BTreeMap::new();
    loop {
        let record = input
            .record()?
            .ok_or_else(|| error("OBJECTS", "missing ENDSEC"))?;
        match record.kind {
            "ENDSEC" => {
                record.empty()?;
                return check_links(owners, entries);
            }
            "DICTIONARY" => {
                if owners.len() >= limit {
                    return Err(invalid("dictionary count exceeds limit"));
                }
                let id = handle(record.required(5)?, record.kind)?;
                let owner = read_fields(record, id, &mut entries, limit)?;
                if id == 0 || owners.insert(id, owner).is_some() {
                    return Err(invalid("zero or duplicate dictionary handle"));
                }
            }
            "EOF" | "SECTION" => return Err(error("OBJECTS", "missing ENDSEC")),
            kind if is_entity_kind(kind) => {
                return Err(error(kind, "graphical entity in OBJECTS"));
            }
            _ => {
                if owners.len() >= limit {
                    return Err(error("OBJECTS", "object count exceeds limit"));
                }
                let (id, owner) = read_object(record)?;
                if owners.insert(id, Some(owner)).is_some() {
                    return Err(error(record.kind, "duplicate object handle"));
                }
            }
        }
    }
}

/// The common object header: handle 5, then group 102 brackets and owner 330, up to the
/// first subclass marker 100. The rest is opaque. An object needs a handle and an owner.
fn read_object(record: Record<'_>) -> Result<(u64, u64), DxfError> {
    let kind = record.kind;
    let mut brackets = AppGroup::default();
    let (mut id, mut owner, mut header) = (None, None, true);
    let mut fields = record.pairs();
    while let Some(pair) = fields.next()? {
        // A graphical entity has the AcDbEntity subclass; it never belongs in OBJECTS.
        if pair.code == 100 && pair.value.trim().eq_ignore_ascii_case("AcDbEntity") {
            return Err(error(kind, "graphical entity in OBJECTS"));
        }
        if !header || brackets.consume(kind, pair)? {
            continue;
        }
        match pair.code {
            5 if id.is_none() => id = Some(handle(pair.value, kind)?),
            330 if owner.is_none() => owner = Some(handle(pair.value, kind)?),
            5 | 330 => return Err(error(kind, "duplicate object header field")),
            100 => header = false,
            _ => {}
        }
    }
    brackets.finish(kind)?;
    let id = id
        .filter(|&id| id != 0)
        .ok_or_else(|| error(kind, "missing or zero object handle"))?;
    let owner = owner
        .filter(|&owner| owner != 0)
        .ok_or_else(|| error(kind, "missing object owner"))?;
    Ok((id, owner))
}

fn read_fields(
    record: Record<'_>,
    id: u64,
    entries: &mut BTreeMap<u64, u64>,
    limit: usize,
) -> Result<Option<u64>, DxfError> {
    let mut owner = None;
    let mut singleton_fields = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut needs_target = false;
    let mut brackets = AppGroup::default();
    let mut fields = record.pairs();
    while let Some(pair) = fields.next()? {
        if needs_target && !matches!(pair.code, 350 | 360) {
            return Err(invalid("missing dictionary entry target"));
        }
        if brackets.consume(record.kind, pair)? {
            continue;
        }
        if !matches!(pair.code, 3 | 350 | 360) && !singleton_fields.insert(pair.code) {
            return Err(invalid("duplicate dictionary field"));
        }
        match pair.code {
            5 => {}
            100 if pair.value == "AcDbDictionary" => {}
            330 => owner = Some(handle(pair.value, record.kind)?),
            280 if matches!(integer(pair.value, record.kind)?, 0 | 1) => {}
            281 if (0..=5).contains(&integer(pair.value, record.kind)?) => {}
            3 => {
                if entries.len() >= limit {
                    return Err(invalid("dictionary entry count exceeds limit"));
                }
                if pair.value.trim().is_empty() || !names.insert(pair.value.to_ascii_uppercase()) {
                    return Err(invalid("empty or duplicate entry name"));
                }
                needs_target = true;
            }
            350 | 360 if needs_target => {
                let target = handle(pair.value, record.kind)?;
                if entries.insert(target, id).is_some() {
                    return Err(invalid("dictionary entry target has multiple owners"));
                }
                needs_target = false;
            }
            _ => {
                return Err(invalid(&format!(
                    "unsupported dictionary group {} or value",
                    pair.code
                )));
            }
        }
    }
    if needs_target {
        return Err(invalid("missing dictionary entry target"));
    }
    brackets.finish(record.kind)?;
    Ok(owner)
}

fn check_links(
    mut owners: BTreeMap<u64, Option<u64>>,
    entries: BTreeMap<u64, u64>,
) -> Result<(), DxfError> {
    // A named entry also identifies a parent when the child omits its owner field.
    for (child, parent) in entries {
        let owner = owners
            .get_mut(&child)
            .ok_or_else(|| invalid("unresolved entry target"))?;
        if owner.is_some_and(|declared| declared != parent) {
            return Err(invalid("entry target names a different owner"));
        }
        *owner = Some(parent);
    }

    // A chain terminates at a root or at a chain validated in a previous walk.
    // Mark each visited dictionary once to bound the work without recursion.
    let mut visited = BTreeMap::new();
    for &start in owners.keys() {
        let mut node = start;
        while node != 0 {
            if let Some(previous_walk) = visited.get(&node) {
                if *previous_walk == start {
                    return Err(invalid("cyclic dictionary ownership"));
                }
                break;
            }
            visited.insert(node, start);
            node = owners
                .get(&node)
                .ok_or_else(|| invalid("unresolved dictionary owner"))?
                .unwrap_or(0);
        }
    }
    Ok(())
}
