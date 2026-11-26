//! Shared AC trunnion machine fixture for the Mach4 3+2 suites (#1968,
//! #3477): an XYZ mill with an A table rotary about X that carries a C
//! rotary about Z, written as a `.mch` text and mirrored for the engine.
//!
//! Provenance: our own `.mch` schema and fixture values; no vendor
//! machine file consulted.
#![allow(dead_code)]

use camcraft_post::machine::parse_mch;
use camcraft_post::post::machine;
use serde_json::{json, Value};

pub struct TrunnionSpec<'a> {
    pub unit: &'a str,
    pub rtcp: Option<bool>,
    pub sign_conv: &'a str,
    pub a_zero: f64,
    pub c_zero: f64,
    pub c_rollover: bool,
    pub pivot: [f64; 3],
    pub offset: [f64; 3],
}

pub fn trunnion_mch_json(spec: &TrunnionSpec<'_>) -> String {
    let mut controller = json!({
        "synced_configuration": { "parts": {
            "X": { "coordinate": 0, "max_normal_speed": 4000.0, "max_rapid_speed": 8000.0 },
            "Y": { "coordinate": 1, "max_normal_speed": 4000.0, "max_rapid_speed": 8000.0 },
            "Z": { "coordinate": 2, "max_normal_speed": 3000.0, "max_rapid_speed": 6000.0 },
            "A": { "coordinate": 3, "max_normal_speed": 1500.0, "max_rapid_speed": 3000.0 },
            "C": { "coordinate": 4, "max_normal_speed": 2000.0, "max_rapid_speed": 4000.0 }
        } }
    });
    if let Some(flag) = spec.rtcp {
        controller["rtcp"] = json!(flag);
    }
    json!({
        "general": { "model": "MACH4 AC TRUNNION", "vendor": "Camcraft", "kind": "mill" },
        "kinematics": { "default": {
            "units": { "length": spec.unit, "angle": "degrees" },
            "sign_convention": spec.sign_conv,
            "parts": [{ "id": "base", "name": "table", "type": "table", "parts": [
                {
                    "id": "a_axis", "name": "A", "type": "rotary", "axis": [1.0, 0.0, 0.0],
                    "pivot": spec.pivot, "travel": { "min": -110.0, "max": 30.0 },
                    "zero_offset": spec.a_zero, "rollover": false,
                    "parts": [{
                        "id": "c_axis", "name": "C", "type": "rotary", "axis": [0.0, 0.0, 1.0],
                        "offset": spec.offset, "travel": { "min": -720.0, "max": 720.0 },
                        "zero_offset": spec.c_zero, "rollover": spec.c_rollover
                    }]
                },
                { "id": "x", "name": "X", "type": "linear", "travel": { "min": -300.0, "max": 300.0 },
                  "parts": [{ "id": "y", "name": "Y", "type": "linear", "travel": { "min": -200.0, "max": 200.0 } }] },
                { "id": "z", "name": "Z", "type": "linear", "travel": { "min": -250.0, "max": 0.0 }, "safe_retract": 0.0,
                  "parts": [{ "id": "head", "name": "head", "type": "head",
                    "spindle": { "min_speed": 1000.0, "max_speed": 18000.0 },
                    "tool_station": { "coolants": [{ "name": "Flood", "mCode": "M8" }, { "name": "Mist", "mCode": "M7" }] }
                  }] }
            ] }]
        } },
        "controller": controller,
        "tooling": { "number_of_tools": 16 }
    })
    .to_string()
}

/// The review trunnion (mm, no RTCP, table right-hand signs, pivot and
/// offset at the origin) with the given A and C zero offsets.
pub fn review_trunnion_with_zeros(a_zero: f64, c_zero: f64) -> Value {
    let spec = TrunnionSpec {
        unit: "mm", rtcp: Some(false), sign_conv: "table_rh", a_zero, c_zero,
        c_rollover: false, pivot: [0.0; 3], offset: [0.0; 3],
    };
    machine::mirror(&parse_mch(&trunnion_mch_json(&spec)).expect("review trunnion parses"))
}

pub fn review_trunnion() -> Value {
    review_trunnion_with_zeros(0.0, 0.0)
}
