//! The operator side of a machine (#2632): the side of the work envelope
//! from which the operator looks into it. A `.mch` file may state it
//! (`general.operator_side`); a file that states nothing takes the
//! default of its kind. Display data only: no check, post or simulation
//! reads it.
//! Provenance: general machine-tool practice (the operator of a vertical
//! mill or a gantry table stands at the front, and +Y runs away from the
//! operator; a lathe is drawn as the operator sees the turning plane) and
//! our own design notes (`docs/decisions/2632-operator-side.md`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{kind, Kind, Machine};

/// A side of the work envelope, named by the world axis that points out
/// of the envelope toward the operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OperatorSide {
    #[serde(rename = "-X")]
    MinusX,
    #[serde(rename = "+X")]
    PlusX,
    #[serde(rename = "-Y")]
    MinusY,
    #[serde(rename = "+Y")]
    PlusY,
}

impl OperatorSide {
    /// Each side, in the order that a refusal lists the words.
    pub const ALL: [OperatorSide; 4] = [Self::MinusX, Self::PlusX, Self::MinusY, Self::PlusY];

    /// The word that a machine file writes for this side.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::MinusX => "-X",
            Self::PlusX => "+X",
            Self::MinusY => "-Y",
            Self::PlusY => "+Y",
        }
    }

    /// The side that `word` names, `None` for any other word.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.word() == word.trim())
    }

    /// The default side of a machine kind. A lathe is seen from +Y: the
    /// lathe view draws the turning plane with +X up and +Z to the right.
    /// Each other kind is seen from the front, -Y.
    #[must_use]
    pub const fn default_for(kind: Kind) -> Self {
        match kind {
            Kind::Lathe => Self::PlusY,
            Kind::Mill | Kind::MillTurn | Kind::Plasma | Kind::Waterjet | Kind::WireEdm | Kind::SinkerEdm => Self::MinusY,
        }
    }

    /// The side of `machine`: the stated one, or the default of its kind.
    #[must_use]
    pub fn of(machine: &Machine) -> Self {
        machine.operator_side.unwrap_or_else(|| Self::default_for(kind(Some(machine))))
    }

    /// The world axis (0 = X, 1 = Y) and the sign that point from the
    /// envelope toward the operator.
    #[must_use]
    pub const fn outward(self) -> (usize, f64) {
        match self {
            Self::MinusX => (0, -1.0),
            Self::PlusX => (0, 1.0),
            Self::MinusY => (1, -1.0),
            Self::PlusY => (1, 1.0),
        }
    }
}

/// `general.operator_side` of a machine file: absent or null is unstated;
/// one of the four words states the side; anything else refuses the file
/// by name, so that a wrong word never reads as the default.
pub(super) fn parse(v: &Value) -> Result<Option<OperatorSide>, String> {
    match v.pointer("/general/operator_side") {
        None | Some(Value::Null) => Ok(None),
        Some(x) => x
            .as_str()
            .and_then(OperatorSide::from_word)
            .map(Some)
            .ok_or_else(|| format!("machine file: general.operator_side must be \"-X\", \"+X\", \"-Y\" or \"+Y\", not {x}")),
    }
}
