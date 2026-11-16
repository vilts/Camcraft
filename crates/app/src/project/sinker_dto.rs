//! The Sinker Burn's file mirror (#541): the op's parameters as plain
//! keys - an additive variant, no older file names it. The orbit
//! (#544) is an additive key inside it; the side burn (#545) is its
//! own appended variant over the same keys.
//! Provenance: our own file format (`project/mod.rs`); the names are ours.

use super::*;

use crate::operation::{
    SinkerBurnOp, SinkerDatum, SinkerOrbit, SinkerOrbitKind, SinkerPass, SinkerSideBurnOp, SinkerSideDirection,
};

/// The orbit pattern's file word (#544) - the same words a machine's
/// `edm.orbits[].kind` states.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SinkerOrbitKindDto {
    Circular,
    Vector,
    Planetary,
}

fn four() -> u32 {
    4
}

fn one() -> u32 {
    1
}

/// The orbit (#544): additive - a file without it burns a plain plunge.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct SinkerOrbitDto {
    pub kind: SinkerOrbitKindDto,
    pub radius: f64,
    #[serde(default = "four")]
    pub vectors: u32,
    #[serde(default = "one")]
    pub turns: u32,
}

impl SinkerOrbitDto {
    fn from_orbit(o: &SinkerOrbit) -> Self {
        SinkerOrbitDto {
            kind: match o.kind {
                SinkerOrbitKind::Circular => SinkerOrbitKindDto::Circular,
                SinkerOrbitKind::Vector => SinkerOrbitKindDto::Vector,
                SinkerOrbitKind::Planetary => SinkerOrbitKindDto::Planetary,
            },
            radius: o.radius,
            vectors: o.vectors,
            turns: o.turns,
        }
    }

    fn to_orbit(self) -> SinkerOrbit {
        SinkerOrbit {
            kind: match self.kind {
                SinkerOrbitKindDto::Circular => SinkerOrbitKind::Circular,
                SinkerOrbitKindDto::Vector => SinkerOrbitKind::Vector,
                SinkerOrbitKindDto::Planetary => SinkerOrbitKind::Planetary,
            },
            radius: self.radius,
            vectors: self.vectors,
            turns: self.turns,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SinkerDatumDto {
    WorkTop,
    CavityFloor,
    ElectrodeTip,
}

/// Which pass of the cavity (#543). A file of #541 names none: single.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SinkerPassDto {
    #[default]
    Single,
    Rough,
    Finish,
}

impl SinkerPassDto {
    fn is_single(&self) -> bool {
        *self == SinkerPassDto::Single
    }
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SinkerDto {
    pub clearance_z: HeightDto,
    pub top_z: HeightDto,
    pub floor_z: HeightDto,
    pub datum: SinkerDatumDto,
    pub depth: f64,
    pub x: f64,
    pub y: f64,
    pub feed: f64,
    #[serde(default)]
    pub condition: String,
    /// #543: additive keys, written only for a rough or finish burn.
    #[serde(default, skip_serializing_if = "SinkerPassDto::is_single")]
    pub pass: SinkerPassDto,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub cavity_x: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub cavity_y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orbit: Option<SinkerOrbitDto>,
    /// #546: additive keys - an older file states none (one layer, no
    /// measured wear) and loads unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wear_lost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wear_over: Option<f64>,
}

impl SinkerDto {
    pub(super) fn from_op(p: &SinkerBurnOp) -> Self {
        Self::from_burn(p)
    }

    fn from_burn(p: &SinkerBurnOp) -> Self {
        SinkerDto {
            clearance_z: p.clearance_z.into(),
            top_z: p.top_z.into(),
            floor_z: p.floor_z.into(),
            datum: match p.datum {
                SinkerDatum::WorkTop => SinkerDatumDto::WorkTop,
                SinkerDatum::CavityFloor => SinkerDatumDto::CavityFloor,
                SinkerDatum::ElectrodeTip => SinkerDatumDto::ElectrodeTip,
            },
            depth: p.depth,
            x: p.x,
            y: p.y,
            feed: p.feed,
            condition: p.condition.clone(),
            pass: match p.pass {
                SinkerPass::Single => SinkerPassDto::Single,
                SinkerPass::Rough => SinkerPassDto::Rough,
                SinkerPass::Finish => SinkerPassDto::Finish,
            },
            cavity_x: p.cavity_x,
            cavity_y: p.cavity_y,
            layer: p.layer,
            wear_lost: p.wear_lost,
            wear_over: p.wear_over,
            orbit: p.orbit.as_ref().map(SinkerOrbitDto::from_orbit),
        }
    }

    pub(super) fn to_params(&self) -> OpParams {
        OpParams::SinkerBurn(self.to_burn())
    }

    fn to_burn(&self) -> SinkerBurnOp {
        SinkerBurnOp {
            clearance_z: self.clearance_z.into(),
            top_z: self.top_z.into(),
            floor_z: self.floor_z.into(),
            datum: match self.datum {
                SinkerDatumDto::WorkTop => SinkerDatum::WorkTop,
                SinkerDatumDto::CavityFloor => SinkerDatum::CavityFloor,
                SinkerDatumDto::ElectrodeTip => SinkerDatum::ElectrodeTip,
            },
            depth: self.depth,
            x: self.x,
            y: self.y,
            feed: self.feed,
            condition: self.condition.clone(),
            pass: match self.pass {
                SinkerPassDto::Single => SinkerPass::Single,
                SinkerPassDto::Rough => SinkerPass::Rough,
                SinkerPassDto::Finish => SinkerPass::Finish,
            },
            cavity_x: self.cavity_x,
            cavity_y: self.cavity_y,
            layer: self.layer,
            wear_lost: self.wear_lost,
            wear_over: self.wear_over,
            orbit: self.orbit.map(SinkerOrbitDto::to_orbit),
        }
    }
}

/// The side burn's direction word (#545).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SinkerSideDirectionDto {
    PlusX,
    MinusX,
    PlusY,
    MinusY,
}

/// The Sinker Side Burn's file mirror (#545): the burn's keys, flat, and
/// the side's three - an appended variant, no older file names it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SinkerSideDto {
    #[serde(flatten)]
    pub burn: SinkerDto,
    pub direction: SinkerSideDirectionDto,
    pub amount: f64,
    pub undersize: f64,
}

impl SinkerSideDto {
    pub(super) fn from_op(p: &SinkerSideBurnOp) -> Self {
        SinkerSideDto {
            burn: SinkerDto::from_burn(&p.burn),
            direction: match p.direction {
                SinkerSideDirection::PlusX => SinkerSideDirectionDto::PlusX,
                SinkerSideDirection::MinusX => SinkerSideDirectionDto::MinusX,
                SinkerSideDirection::PlusY => SinkerSideDirectionDto::PlusY,
                SinkerSideDirection::MinusY => SinkerSideDirectionDto::MinusY,
            },
            amount: p.amount,
            undersize: p.undersize,
        }
    }

    pub(super) fn to_params(&self) -> OpParams {
        OpParams::SinkerSideBurn(SinkerSideBurnOp {
            burn: self.burn.to_burn(),
            direction: match self.direction {
                SinkerSideDirectionDto::PlusX => SinkerSideDirection::PlusX,
                SinkerSideDirectionDto::MinusX => SinkerSideDirection::MinusX,
                SinkerSideDirectionDto::PlusY => SinkerSideDirection::PlusY,
                SinkerSideDirectionDto::MinusY => SinkerSideDirection::MinusY,
            },
            amount: self.amount,
            undersize: self.undersize,
        })
    }
}
