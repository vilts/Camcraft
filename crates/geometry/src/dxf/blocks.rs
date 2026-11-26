//! Provenance: reserved layout definitions and block record owners in the Autodesk DXF Reference.
use super::entities::layer_name;
use super::pairs::{Pairs, Record, handle, integer, number};
use super::types::{DxfError, error};
use std::collections::{BTreeMap, BTreeSet};

/// Keep the named owners until EOF so TABLES and BLOCKS can arrive in either order.
#[derive(Default)]
pub(super) struct Blocks {
    records: BTreeMap<String, u64>,
    wrappers: BTreeMap<String, Option<u64>>,
    handles: BTreeSet<u64>,
}

fn layout_name(name: &str) -> Option<String> {
    if name.len() > 255 {
        return None;
    }
    let name = name.to_ascii_uppercase();
    if name == "*MODEL_SPACE" {
        return Some(name);
    }
    let suffix = name.strip_prefix("*PAPER_SPACE")?;
    // Further layouts use the decimal sequence 0, 1, 2, ...
    (suffix.bytes().all(|c| c.is_ascii_digit()) && !(suffix.len() > 1 && suffix.starts_with('0')))
        .then_some(name)
}

impl Blocks {
    fn register_handle(&mut self, id: u64, kind: &str) -> Result<(), DxfError> {
        if id == 0 || !self.handles.insert(id) {
            return Err(error(kind, "zero or duplicate record handle"));
        }
        Ok(())
    }

    pub fn table_record(&mut self, record: Record<'_>, limit: usize) -> Result<(), DxfError> {
        let Some(name) = record.field(2)?.and_then(layout_name) else {
            return Ok(());
        };
        if let Some(value) = record.field(5)? {
            let id = handle(value, record.kind)?;
            if self.records.len() >= limit {
                return Err(error(record.kind, "layout record count exceeds limit"));
            }
            self.register_handle(id, record.kind)?;
            if self.records.insert(name, id).is_some() {
                return Err(error(record.kind, "duplicate layout name"));
            }
        }
        Ok(())
    }

    pub fn read(&mut self, input: &mut Pairs<'_>, limit: usize) -> Result<(), DxfError> {
        loop {
            let start = input
                .record()?
                .ok_or_else(|| error("BLOCKS", "missing ENDSEC"))?;
            if start.kind == "ENDSEC" {
                return start.empty();
            }
            if start.kind != "BLOCK" {
                return Err(error(start.kind, "expected BLOCK or ENDSEC"));
            }
            if self.wrappers.len() >= limit {
                return Err(error("BLOCKS", "layout block count exceeds limit"));
            }
            let name = layout_name(start.required(2)?)
                .ok_or_else(|| error("BLOCK", "unsupported block definition"))?;
            let start_owner = self.wrapper_fields(start, &name)?;
            let end = input
                .record()?
                .ok_or_else(|| error("BLOCK", "missing ENDBLK"))?;
            if end.kind != "ENDBLK" {
                return Err(error(
                    end.kind,
                    "expected ENDBLK immediately after layout BLOCK",
                ));
            }
            let end_owner = self.wrapper_fields(end, &name)?;
            if start_owner.zip(end_owner).is_some_and(|(a, b)| a != b) {
                return Err(error("ENDBLK", "owner differs from BLOCK"));
            }
            if self
                .wrappers
                .insert(name, start_owner.or(end_owner))
                .is_some()
            {
                return Err(error("BLOCK", "duplicate layout definition"));
            }
        }
    }

    fn wrapper_fields(&mut self, record: Record<'_>, name: &str) -> Result<Option<u64>, DxfError> {
        let begin = record.kind == "BLOCK";
        let subclass = if begin {
            "AcDbBlockBegin"
        } else {
            "AcDbBlockEnd"
        };
        let mut seen = BTreeSet::new();
        let mut classes = BTreeSet::new();
        let mut owner = None;
        let mut fields = record.pairs();
        while let Some(pair) = fields.next()? {
            if pair.code != 100 && !seen.insert(pair.code) {
                return Err(error(record.kind, "duplicate wrapper field"));
            }
            match pair.code {
                5 => self.register_handle(handle(pair.value, record.kind)?, record.kind)?,
                330 => {
                    let id = handle(pair.value, record.kind)?;
                    owner = (id != 0).then_some(id);
                }
                100 if matches!(pair.value, "AcDbEntity") || pair.value == subclass => {
                    if !classes.insert(pair.value) {
                        return Err(error(record.kind, "duplicate subclass marker"));
                    }
                }
                8 => layer_name(pair.value, record.kind)?,
                67 if matches!(integer(pair.value, record.kind)?, 0 | 1) => {}
                2 | 3 if begin && pair.value.eq_ignore_ascii_case(name) => {}
                70 if begin && integer(pair.value, record.kind)? == 0 => {}
                10 | 20 | 30 if begin && number(pair.value, record.kind)? == 0.0 => {}
                1 if begin && pair.value.is_empty() => {}
                4 if begin => {}
                _ => {
                    return Err(error(
                        record.kind,
                        &format!("unsupported wrapper group {} or value", pair.code),
                    ));
                }
            }
        }
        Ok(owner)
    }

    pub fn check_owners(&self) -> Result<(), DxfError> {
        for (name, owner) in &self.wrappers {
            if owner.is_some() && self.records.get(name) != owner.as_ref() {
                return Err(error(
                    "BLOCK",
                    "owner does not resolve to the layout BLOCK_RECORD",
                ));
            }
        }
        Ok(())
    }
}
