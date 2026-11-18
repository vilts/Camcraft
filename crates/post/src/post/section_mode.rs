// Provenance: clean-room intermediate `.cnc` section kinematics mode marker (`PARAM SectionMode`).

//! Explicit section kinematics mode (`3-axis`, `3+2 indexed`, `simultaneous 5-axis`).
//!
//! Emitted before `SECTION` by [`set_section_mode`] and decoded cleanly by
//! [`read_section_mode`]. Unrecognized `SectionMode` values are refused by
//! name rather than guessed.

use super::cnc::Record;
use serde_json::{json, Value};
use std::fmt;
use thiserror::Error;

/// Kinematics execution mode carried by a post section (`PARAM SectionMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SectionMode {
    /// Planar/3D prismatic operation with fixed vertical tool axis (`3-axis`).
    ThreeAxis,
    /// Planar operation indexed to one fixed rotary orientation (`3+2 indexed`).
    Indexed3Plus2,
    /// Continuous synchronized multi-axis motion (`simultaneous 5-axis`).
    Simultaneous5Axis,
}

impl SectionMode {
    /// Parameter key written in `.cnc` streams (`PARAM SectionMode ...`).
    pub const PARAM_NAME: &'static str = "SectionMode";
    pub const THREE_AXIS_STR: &'static str = "3-axis";
    pub const INDEXED_3_PLUS_2_STR: &'static str = "3+2 indexed";
    pub const SIMULTANEOUS_5_AXIS_STR: &'static str = "simultaneous 5-axis";

    /// Canonical wire string written after `PARAM SectionMode`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ThreeAxis => Self::THREE_AXIS_STR,
            Self::Indexed3Plus2 => Self::INDEXED_3_PLUS_2_STR,
            Self::Simultaneous5Axis => Self::SIMULTANEOUS_5_AXIS_STR,
        }
    }

    /// Parse one canonical `SectionMode` string, refusing any unknown value by name.
    pub fn parse(raw: &str) -> Result<Self, SectionModeError> {
        match raw.trim() {
            Self::THREE_AXIS_STR => Ok(Self::ThreeAxis),
            Self::INDEXED_3_PLUS_2_STR => Ok(Self::Indexed3Plus2),
            Self::SIMULTANEOUS_5_AXIS_STR => Ok(Self::Simultaneous5Axis),
            other => Err(SectionModeError::InvalidValue { got: other.to_string() }),
        }
    }

    #[must_use]
    pub const fn is_3_plus_2(self) -> bool {
        matches!(self, Self::Indexed3Plus2)
    }

    #[must_use]
    pub const fn is_simultaneous(self) -> bool {
        matches!(self, Self::Simultaneous5Axis)
    }
}

impl fmt::Display for SectionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Refusal returned when a `PARAM SectionMode` value cannot be decoded safely.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SectionModeError {
    #[error(
        "SectionMode {got:?} is unknown (expected \"3-axis\", \"3+2 indexed\", or \"simultaneous 5-axis\")"
    )]
    InvalidValue { got: String },
}

/// Insert (or update) `PARAM SectionMode <mode>` directly before `SECTION`.
pub fn set_section_mode(records: &mut Vec<Record>, mode: SectionMode) {
    let val = json!(mode.as_str());
    for rec in records.iter_mut() {
        if let Record::Param { name, value } = rec
            && name == SectionMode::PARAM_NAME
        {
            *value = val;
            return;
        }
    }
    let at = records
        .iter()
        .position(|r| matches!(r, Record::Section { begin: true }))
        .unwrap_or(records.len());
    records.insert(
        at,
        Record::Param {
            name: SectionMode::PARAM_NAME.to_string(),
            value: val,
        },
    );
}

/// Decode `PARAM SectionMode` if present in `records`, refusing unrecognized values by name.
pub fn read_section_mode(records: &[Record]) -> Result<Option<SectionMode>, SectionModeError> {
    for r in records {
        if let Record::Param { name, value } = r
            && name == SectionMode::PARAM_NAME
        {
            return match value {
                Value::String(s) => SectionMode::parse(s).map(Some),
                other => Err(SectionModeError::InvalidValue { got: other.to_string() }),
            };
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::post::cnc;

    #[test]
    fn round_trips_all_three_modes_and_refuses_unknown_value() {
        for mode in [
            SectionMode::ThreeAxis,
            SectionMode::Indexed3Plus2,
            SectionMode::Simultaneous5Axis,
        ] {
            let mut recs = vec![Record::Section { begin: true }, Record::Section { begin: false }];
            set_section_mode(&mut recs, mode);
            let txt = cnc::serialize(&recs);
            let parsed = cnc::parse(&txt).expect("valid stream");
            assert_eq!(read_section_mode(&parsed), Ok(Some(mode)));
        }
        let bad = vec![Record::Param {
            name: SectionMode::PARAM_NAME.into(),
            value: json!("5axis-magic"),
        }];
        assert!(matches!(
            read_section_mode(&bad),
            Err(SectionModeError::InvalidValue { .. })
        ));
    }
}
