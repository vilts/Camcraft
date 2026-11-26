//! Provenance: original machine tool reservation grammar from issue #2604.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// A controller selector kept for a non-cutting device. Both selectors may be stated.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReservedTool {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "use")]
    pub usage: String,
}

/// Recheck saved machines as well as file-form entries.
pub fn validate(entries: &[ReservedTool]) -> Result<(), String> {
    let mut numbers = HashSet::new();
    let mut names = HashSet::new();
    for (index, entry) in entries.iter().enumerate() {
        let key = format!("tooling.reserved[{index}]");
        if entry.number.is_none() && entry.name.is_none() {
            return Err(format!("{key}: state a tool number or name"));
        }
        if !matches!(entry.usage.as_str(), "probe" | "tool_setter") {
            return Err(format!("{key}.use: '{}' must be probe or tool_setter", entry.usage));
        }
        if let Some(number) = entry.number {
            if !(1..=99_999).contains(&number) {
                return Err(format!("{key}.number: {number} must be in 1-99999 (T0 clears the spindle)"));
            }
            if !numbers.insert(number) {
                return Err(format!("{key}.number: T{number} is reserved twice"));
            }
        }
        if let Some(name) = &entry.name {
            if name.is_empty() || name.trim() != name || !name.bytes().all(|b| b.is_ascii_graphic() || b == b' ') {
                return Err(format!("{key}.name: state a non-empty printable ASCII name without surrounding spaces"));
            }
            if !names.insert(name.to_ascii_lowercase()) {
                return Err(format!("{key}.name: '{name}' is reserved twice"));
            }
        }
    }
    Ok(())
}

/// Read only the machine's explicit reservations; no convention applies to a generic machine.
pub(super) fn parse(value: &Value) -> Result<Vec<ReservedTool>, String> {
    let Some(value) = value.pointer("/tooling/reserved") else { return Ok(Vec::new()) };
    let entries = serde_json::from_value::<Vec<ReservedTool>>(value.clone())
        .map_err(|why| format!("tooling.reserved: {why}"))?;
    validate(&entries)?;
    Ok(entries)
}

/// Judge a standalone post input before the CLI creates or replaces its output.
/// A reserved probe needs its own declared kind, before or after its TOOL record.
pub fn check_stream(machine: &super::Machine, text: &str) -> Result<(), String> {
    use crate::post::cnc::Record;
    if machine.reserved_tools.is_empty() {
        return Ok(());
    }
    validate(&machine.reserved_tools)?;
    let selectors = Selectors {
        machine,
        numbers: machine.reserved_tools.iter().filter_map(|r| r.number.map(|n| (n, r))).collect(),
        names: machine.reserved_tools.iter().filter_map(|r| r.name.as_ref().map(|n| (n.to_ascii_lowercase(), r))).collect(),
    };
    let records = crate::post::cnc::parse(text).map_err(|why| format!("reserved tool check: {why}"))?;
    let mut kind: Option<String> = None;
    let mut tool: Option<SelectedTool> = None;
    let mut fresh_kind = false;
    let mut in_section = false;
    let mut tool_pending = false;
    for record in records {
        match record {
            Record::Param { name, value } if name == "ToolKind" => {
                kind = value.as_str().map(str::to_owned);
                fresh_kind = true;
                if in_section {
                    check_tool(&selectors, &tool, &kind)?;
                    fresh_kind = false;
                }
            }
            Record::Tool { number, name } => {
                if tool_pending || in_section {
                    check_tool(&selectors, &tool, &kind)?;
                    fresh_kind = false;
                }
                if !fresh_kind {
                    kind = None;
                }
                let folded = name.trim().to_ascii_lowercase();
                tool = Some(SelectedTool { number, name, folded });
                tool_pending = true;
                fresh_kind = false;
            }
            Record::Section { begin: true } => {
                in_section = true;
                if kind.is_some() {
                    check_tool(&selectors, &tool, &kind)?;
                    fresh_kind = false;
                }
            }
            Record::Section { begin: false } => {
                check_tool(&selectors, &tool, &kind)?;
                in_section = false;
                tool_pending = false;
                fresh_kind = false;
                // A section end does not unload the spindle. Keep its identity and kind.
            }
            Record::Move(_) | Record::Cycle(_) | Record::Thread(Some(_))
            | Record::Touch(_) | Record::Retreat(_) | Record::Guard(_)
            | Record::Probe { .. } | Record::Found(_) | Record::Torch(_) | Record::Jet(_) => {
                if tool.is_none() {
                    return Err("TOOL selector is not stated; declare TOOL before motion on a machine with reserved tools".into());
                }
                check_tool(&selectors, &tool, &kind)?;
                in_section = true;
                fresh_kind = false;
            }
            Record::Param { .. } | Record::Warn { .. } | Record::Part { .. }
            | Record::Transfer { .. } | Record::Comp { .. } | Record::Thread(None)
            | Record::Plasma(_) | Record::Waterjet(_) => {}
        }
    }
    if tool_pending || in_section {
        check_tool(&selectors, &tool, &kind)?;
    }
    Ok(())
}

struct Selectors<'a> {
    machine: &'a super::Machine,
    numbers: HashMap<u32, &'a ReservedTool>,
    names: HashMap<String, &'a ReservedTool>,
}

struct SelectedTool {
    number: u32,
    name: String,
    folded: String,
}

fn check_tool(
    selectors: &Selectors<'_>,
    tool: &Option<SelectedTool>,
    kind: &Option<String>,
) -> Result<(), String> {
    let Some(SelectedTool { number, name, folded }) = tool else { return Ok(()) };
    let probe = kind.as_deref().is_some_and(|kind| kind.eq_ignore_ascii_case("touch probe"));
    for entry in selectors.numbers.get(number).into_iter().chain(selectors.names.get(folded)) {
        if !(entry.usage == "probe" && probe) {
            return Err(format!(
                "tool '{name}' (T{number}) is reserved for {} on {}; a probe requires ToolKind touch probe",
                entry.usage, selectors.machine.model
            ));
        }
    }
    Ok(())
}
