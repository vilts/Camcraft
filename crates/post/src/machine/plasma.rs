//! The plasma kind's file rules (#669): what a `.mch` that declares
//! `"kind": "plasma"` may and may not state.
//!
//! Provenance: our own rules, specced from Tormach's public 1300PL
//! Operator's Manual UM10720 v1223A - §2 (the machine: an X/Y gantry,
//! a Z torch lifter, a Hypertherm Powermax torch - no spindle), §6.3
//! (the control owns every Z move: probe, pierce height, cut height)
//! and §7.1-7.2 (the plasma G/M words; no M3/M4/M5, no M7/M8/M9, per
//! Tormach's public PathPilot Post Processor Guidelines). Behaviour
//! only, our own words; no vendor machine file or post was read.
//!
//! One law at two doors: the reader (`parse_mch`) refuses the FIRST
//! rule broken, the Machines pane's validator lists them all - both
//! read [`plasma_refusals`], so the two can never disagree. The parsed
//! form's gate (`Machine::is_sane`, the project reader's door) reads
//! [`Machine::plasma_sane`], the same rules over the parsed numbers.

use serde_json::Value;

use super::def::{kind, Kind, Machine};
use super::no_spindle::{head_refusals, NoSpindle};

/// Why a plasma table states no spindle: one sentence, every refusal.
const NO_SPINDLE: &str = "a plasma machine has no spindle - a torch states no rpm, power, speed range or tapping";

/// The plasma rules over a file's raw JSON, as `(key, rule)` pairs in
/// file order. The caller has already read the declared kind as
/// plasma; nothing here re-reads it. Empty = the file keeps them.
///
/// - every `head` part states no `spindle` (each stated key is named:
///   `spindle.max_speed`, `spindle.power`, ...) and no `live_tool`;
/// - no coolant list, on a station or at the tooling root (a plasma
///   control takes no M7/M8/M9);
/// - no rotary axis (a plasma table cuts flat sheet in X and Y).
///
/// X, Y and Z are REQUIRED by the validator's per-kind table, like a
/// mill's; Z is the torch lifter there, not a depth axis.
#[must_use]
pub fn plasma_refusals(v: &Value) -> Vec<(String, String)> {
    let rules = NoSpindle {
        spindle: NO_SPINDLE,
        live_tool: "a plasma machine has no spindle to drive a live tool",
        coolant: "a plasma machine states no coolant - its control takes no M7/M8/M9",
    };
    let (mut out, rotaries) = head_refusals(v, &rules);
    for name in rotaries {
        out.push((name, "a plasma table cuts flat sheet in X and Y - it states no rotary axis".to_owned()));
    }
    out
}

impl Machine {
    /// The plasma rules over the PARSED machine (a hand-edited project
    /// file never went through `parse_mch`): a machine that declares
    /// plasma states no spindle number, range, tapping word, spindle
    /// count, live tool, coolant or rotary. True for every other kind.
    #[must_use]
    pub fn plasma_sane(&self) -> bool {
        if kind(Some(self)) != Kind::Plasma {
            return true;
        }
        self.spindle_min.is_none()
            && self.spindle_max.is_none()
            && self.spindle_power.is_none()
            && self.spindle_ranges.is_empty()
            && self.rigid_tapping.is_none()
            && self.spindles.is_none()
            && self.live_tool.is_none()
            && self.coolants.is_empty()
            && self.axes.iter().all(|a| !a.rotary)
    }
}
