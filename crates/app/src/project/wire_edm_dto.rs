//! The wire contour's file mirror (#532): the op's parameters as plain
//! keys - the two workpiece planes, the machine condition's id, the
//! side, the start hole and the skim passes (#538). `side` has no default on purpose: a file
//! (or an MCP patch) that leaves it out is refused, never read as a
//! die - the side is stated, not inferred (#290 D8); the 4-axis block's
//! pairing rule (#537) likewise. The wire and the
//! gap are not here: they are the machine's condition table's.
//! Provenance: our own file format (`project/mod.rs`); the names are ours.

use super::*;

use crate::operation::{
    WireContourOp, WireFourAxis, WirePairingRule, WireSide, WireSkim, WireSyncPair, WireTabs, FOUR_AXIS_TOLERANCE,
};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WireSideDto {
    Die,
    Punch,
    /// An open run's kerf, left of travel (#1109).
    Left,
    /// An open run's kerf, right of travel (#1109).
    Right,
}

fn default_clearance() -> f64 {
    crate::operation::DEFAULT_CLEARANCE
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip predicate takes a reference
fn is_default_clearance(c: &f64) -> bool {
    c.to_bits() == default_clearance().to_bits()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WireContourDto {
    pub top_z: HeightDto,
    pub bottom_z: HeightDto,
    /// The machine's EDM condition id; absent is none chosen.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub condition: String,
    pub side: WireSideDto,
    /// The start hole / threading point, `[x, y]` mm; absent is not
    /// stated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<[f64; 2]>,
    /// The tabs (#539); absent is off. Each key inside is optional in
    /// the file so a missing one is refused by name at generation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tabs: Option<WireTabsDto>,
    /// The operator states the slug is caught; absent is not (#539).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub capture: bool,
    /// The constant taper, degrees (#536); absent is a vertical cut, so
    /// every file written before it reads unchanged.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub taper_deg: f64,
    /// No-core burnout's wall clearance, mm (#540); absent cuts the
    /// profile alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub burnout_clearance: Option<f64>,
    /// An open run's run-out clearance, mm (#1109); absent is the
    /// seed's, so a closed cut's file reads and writes as before.
    #[serde(default = "default_clearance", skip_serializing_if = "is_default_clearance")]
    pub clearance: f64,
    /// The skim passes after the rough cut (#538), in cut order; absent
    /// is the rough pass alone, so a file without skims reads as before.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skims: Vec<WireSkimDto>,
    /// The 4-axis cut (#537); absent is the one-profile cut, so every
    /// file written before it reads unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub four_axis: Option<WireFourAxisDto>,
}

/// One skim: its condition id (stated - no default) and whether it
/// walks the profile the other way round (absent is not reversed).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WireSkimDto {
    pub condition: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reverse: bool,
}

/// The pairing rule's words. No default: a 4-axis block that leaves the
/// rule out is refused, never read as arc length (#537: stated).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WirePairingDto {
    ArcLength,
    SyncPoints,
}

/// One sync pair, `[x, y]` mm each; a side left out is unpaired (kept,
/// and refused by name at generation).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct WireSyncDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upper: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lower: Option<[f64; 2]>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WireFourAxisDto {
    pub pairing: WirePairingDto,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sync: Vec<WireSyncDto>,
    /// The twist tolerance, mm; absent is the default.
    #[serde(default = "four_axis_tolerance")]
    pub tolerance: f64,
}

fn four_axis_tolerance() -> f64 {
    FOUR_AXIS_TOLERANCE
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct WireTabsDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub at: Vec<[f64; 2]>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cutoff_condition: String,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

impl WireContourDto {
    pub(super) fn from_op(p: &WireContourOp) -> Self {
        WireContourDto {
            top_z: p.top_z.into(),
            bottom_z: p.bottom_z.into(),
            condition: p.condition.clone(),
            side: match p.side {
                WireSide::Die => WireSideDto::Die,
                WireSide::Punch => WireSideDto::Punch,
                WireSide::Left => WireSideDto::Left,
                WireSide::Right => WireSideDto::Right,
            },
            start: p.start,
            tabs: p.tabs.as_ref().map(|t| WireTabsDto {
                count: t.count,
                width: t.width,
                at: t.at.clone(),
                cutoff_condition: t.cutoff_condition.clone(),
            }),
            capture: p.capture,
            taper_deg: p.taper_deg,
            four_axis: p.four_axis.as_ref().map(|f| WireFourAxisDto {
                pairing: match f.pairing {
                    WirePairingRule::ArcLength => WirePairingDto::ArcLength,
                    WirePairingRule::SyncPoints => WirePairingDto::SyncPoints,
                },
                sync: f.sync.iter().map(|s| WireSyncDto { upper: s.upper, lower: s.lower }).collect(),
                tolerance: f.tolerance,
            }),
            burnout_clearance: p.burnout,
            clearance: p.clearance,
            skims: p.skims.iter().map(|s| WireSkimDto { condition: s.condition.clone(), reverse: s.reverse }).collect(),
        }
    }

    pub(super) fn to_params(&self) -> OpParams {
        OpParams::WireContour(WireContourOp {
            top_z: self.top_z.into(),
            bottom_z: self.bottom_z.into(),
            condition: self.condition.clone(),
            side: match self.side {
                WireSideDto::Die => WireSide::Die,
                WireSideDto::Punch => WireSide::Punch,
                WireSideDto::Left => WireSide::Left,
                WireSideDto::Right => WireSide::Right,
            },
            start: self.start,
            tabs: self.tabs.as_ref().map(|t| WireTabs {
                count: t.count,
                width: t.width,
                at: t.at.clone(),
                cutoff_condition: t.cutoff_condition.clone(),
            }),
            capture: self.capture,
            taper_deg: self.taper_deg,
            four_axis: self.four_axis.as_ref().map(|f| WireFourAxis {
                pairing: match f.pairing {
                    WirePairingDto::ArcLength => WirePairingRule::ArcLength,
                    WirePairingDto::SyncPoints => WirePairingRule::SyncPoints,
                },
                sync: f.sync.iter().map(|s| WireSyncPair { upper: s.upper, lower: s.lower }).collect(),
                tolerance: f.tolerance,
            }),
            burnout: self.burnout_clearance,
            clearance: self.clearance,
            skims: self.skims.iter().map(|s| WireSkim { condition: s.condition.clone(), reverse: s.reverse }).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation::Height;

    fn op(tabs: Option<WireTabs>, capture: bool) -> WireContourOp {
        WireContourOp {
            top_z: Height::fixed(12.0),
            bottom_z: Height::fixed(0.0),
            condition: "C110".into(),
            side: WireSide::Die,
            start: Some([0.0, 0.0]),
            tabs,
            capture,
            taper_deg: 0.0,
            four_axis: None,
            burnout: None,
            clearance: crate::operation::DEFAULT_CLEARANCE,
            skims: Vec::new(),
        }
    }

    /// #539: an untabbed cut writes no tab or capture key (no existing
    /// file changes); a tabbed one round-trips every key.
    #[test]
    fn the_tab_keys_round_trip_and_are_absent_when_off() {
        let off = serde_json::to_string(&WireContourDto::from_op(&op(None, false))).expect("json");
        assert!(!off.contains("tabs") && !off.contains("capture"), "{off}");
        let tabs = WireTabs { count: Some(2), width: Some(1.0), at: vec![[0.0, 10.0], [0.0, -10.0]], cutoff_condition: "C120".into() };
        let on = op(Some(tabs), true);
        let text = serde_json::to_string(&WireContourDto::from_op(&on)).expect("json");
        let back: WireContourDto = serde_json::from_str(&text).expect("reads");
        assert_eq!(back.to_params(), OpParams::WireContour(on));
        // A file stating tabs with a key left out loads, and the
        // generator refuses the missing key by name.
        let partial: WireContourDto = serde_json::from_str(
            r#"{"top_z":{"from":"fixed","value":12.0},"bottom_z":{"from":"fixed","value":0.0},"side":"die","tabs":{"count":2}}"#,
        )
        .or_else(|_| serde_json::from_str(&text.replace(r#""width":1.0,"#, "")))
        .expect("a partial tab statement loads");
        let OpParams::WireContour(p) = partial.to_params() else { unreachable!() };
        assert!(p.tabs.is_some_and(|t| t.width.is_none()));
    }
}
