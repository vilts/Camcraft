//! Provenance: original blank hooks, RS-274 conventions, and this repository's committed milling goldens.

use super::{cnc::{self, Record}, PostError};
use crate::cl::MoveKind;
use serde_json::json;

/// A clean-room starting point. Empty hooks deliberately emit no program.
pub const BLANK_TEMPLATE: &str = r#"
description = "Blank milling post";
vendor = "Camcraft";
legal = "Independent clean-room template using public RS-274 / ISO 6983 conventions.";
extension = "nc";
capabilities = 1;
serves = ["mill"];
programUnits = "mm";
function onOpen() {}
function onParameter() {}
function onToolChange() {}
function onSection() {}
function onSectionEnd() {}
function onCommand() {}
function onRapid() {}
function onLinear() {}
function onCircular() {}
function onPlunge() {}
function onCycle() {}
function onCyclePoint() {}
function onCycleEnd() {}
function onDwell() {}
function onSpindleSpeed() {}
function onRadiusCompensation() {}
function onThreadCycle() {}
function onThreadCycleEnd() {}
function onPartLoading() {}
function onPartUnloading() {}
function onPartTransfer() {}
function onProbe() {}
function onProbeMove() {}
function onProbeRetreat() {}
function onProbeGuard() {}
function onProbeResult() {}
function onPlasma() {}
function onTorchOn() {}
function onTorchOff() {}
function onComment() {}
function onClose() {}
"#;

/// Default preview machine, embedded so native and wasm callers need no filesystem.
pub const DEFAULT_VMC_JSON: &str = include_str!("../../../../assets/machines/vmc.mch");

/// A generator golden with the operation context needed to post it safely.
pub struct PreviewSample {
    pub name: &'static str,
    pub kind: &'static str,
    pub tool_name: &'static str,
    pub tool_kind: &'static str,
    pub diameter_mm: f64,
    pub rpm: f64,
    pub golden: &'static str,
}

/// Representative milling streams: straight facing, arcs, pocketing, and canned drilling.
pub const SAMPLES: &[PreviewSample] = &[
    PreviewSample { name: "Facing", kind: "Facing", tool_name: "10MM 3FL END MILL", tool_kind: "Flat end mill", diameter_mm: 10.0, rpm: 4500.0,
        golden: include_str!("../../../app/tests/golden/face_square_40x40.cl") },
    PreviewSample { name: "Contour", kind: "2D Contour", tool_name: "6MM 3FL END MILL", tool_kind: "Flat end mill", diameter_mm: 6.0, rpm: 6000.0,
        golden: include_str!("../../../app/tests/golden/contour_plate_with_hole.cl") },
    PreviewSample { name: "Pocket", kind: "Pocket", tool_name: "8MM 3FL END MILL", tool_kind: "Flat end mill", diameter_mm: 8.0, rpm: 5000.0,
        golden: include_str!("../../../app/tests/golden/pocket_zigzag_plate_with_island.cl") },
    PreviewSample { name: "Drilling", kind: "Drill", tool_name: "6MM HSS DRILL", tool_kind: "Drill", diameter_mm: 6.0, rpm: 1500.0,
        golden: include_str!("../../../app/tests/golden/drill_plate.cl") },
];

impl PreviewSample {
    /// Build a deterministic `.cnc` preview, dressing legacy paths as real programs.
    pub fn stream(&self) -> Result<String, PostError> {
        let mut path = cnc::parse(self.golden)?;
        let moves: Vec<_> = path.iter().filter_map(|r| match r {
            Record::Move(m) => Some(m),
            _ => None,
        }).collect();
        let feeds: Vec<_> = path.iter().filter_map(|r| match r {
            Record::Cycle(c) => Some(moves.iter().skip(c.first_move)
                .find(|m| m.kind != MoveKind::Rapid).and_then(|m| m.feed)),
            _ => None,
        }).collect();
        let mut feeds = feeds.into_iter();
        for record in &mut path {
            if let Record::Cycle(c) = record {
                let feed = feeds.next().flatten();
                if c.feed.is_none() { c.feed = feed; }
            }
        }
        let mut records = Vec::new();
        for (name, value) in [
            ("SetupName", json!("Preview setup")),
            ("OperationName", json!(self.name)),
            ("OperationKind", json!(self.kind)),
            ("ToolKind", json!(self.tool_kind)),
            ("ToolDiameter", json!(self.diameter_mm)),
            ("ClearancePlane", json!(5.0)),
        ] {
            records.push(Record::Param { name: name.into(), value });
        }
        records.push(Record::Tool { number: 1, name: self.tool_name.into() });
        records.push(Record::Param { name: "SpindleSpeed".into(), value: json!(self.rpm) });
        records.extend(path);
        Ok(cnc::serialize(&super::normalize_program_order(records)))
    }
}
