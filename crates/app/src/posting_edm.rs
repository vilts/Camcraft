//! The sinker burn's stream PARAMs (#542): the op's condition id and the
//! electrode's pickup offset, said before the section opens so the
//! sinker post can select the condition and reference the electrode in
//! its preamble; an orbit (#544) says its geometry and radius, and a
//! Sinker Side Burn (#545) rides the same PARAMs, so a post that cannot
//! state either refuses it by name instead of posting its explicit moves
//! as something else. Every other op's stream is unchanged.
//!
//! `SinkerCondition` is the sinker's own word, not the wire section's
//! `EdmCondition` (#533): the engine refuses each through any script
//! that does not serve that kind, so neither EDM post reads the other's
//! section as its own.
//!
//! Provenance: our own stream vocabulary (docs/CNC-INTERMEDIATE.md) over
//! the umbrella's decisions (#290 D4, D5). No vendor material.

use crate::operation::{OpParams, SinkerSideBurnOp};
use crate::post::cnc::Record;
use crate::tools::{Tool, ToolKind};
use serde_json::json;

/// Insert `PARAM SinkerCondition`, `PARAM ElectrodePickup` and, for an
/// orbit, `PARAM EdmOrbit` / `PARAM EdmOrbitRadius` ahead of the
/// stream's first TOOL / SECTION record, for a Sinker Burn or Sinker
/// Side Burn on an electrode. A blank condition is not said - the post
/// refuses the section by name for want of it.
pub(super) fn insert(params: &OpParams, tool: &Tool, records: &mut Vec<Record>) {
    let sb = match params {
        OpParams::SinkerBurn(p) | OpParams::SinkerSideBurn(SinkerSideBurnOp { burn: p, .. }) => p,
        _ => return,
    };
    if tool.kind != ToolKind::Electrode {
        return;
    }
    let mut add = Vec::new();
    let id = sb.condition.trim();
    if !id.is_empty() {
        add.push(Record::Param { name: "SinkerCondition".into(), value: json!(id) });
    }
    if tool.shank_len.is_finite() && tool.shank_len >= 0.0 {
        add.push(Record::Param { name: "ElectrodePickup".into(), value: json!(tool.shank_len) });
    }
    if let Some(o) = &sb.orbit {
        add.push(Record::Param { name: "EdmOrbit".into(), value: json!(o.kind.word()) });
        if o.radius.is_finite() {
            add.push(Record::Param { name: "EdmOrbitRadius".into(), value: json!(o.radius) });
        }
    }
    let at = records
        .iter()
        .position(|r| matches!(r, Record::Tool { .. } | Record::Section { .. }))
        .unwrap_or(records.len());
    records.splice(at..at, add);
}
