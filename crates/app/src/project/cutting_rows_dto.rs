//! The file mirror of a tool's per-material cutting data (#890): a list
//! of `{"material": <word>, "rpm": n, "feed": n}` under the tool's
//! additive `cutting_data` key. The material is the same word the
//! document's `material` key writes (`WorkMaterial::word`), or (#2015)
//! `unknown` for the tool's `Unknown` row; an unknown word refuses the
//! file by name at read, never a silently dropped row. #2015 adds the
//! optional `stepdown` / `stepover` keys (mm).
//!
//! Provenance: our own file format, the M13.2 material-word law reused.

use serde::{Deserialize, Serialize};

use crate::tools::WorkMaterial;
use camcraft_tools::cutting_rows::{CuttingRow, UNKNOWN_WORD};

/// A row's material word: a group of the built-in table, or `unknown`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(try_from = "String", into = "String")]
pub struct MaterialWord(pub Option<WorkMaterial>);

impl TryFrom<String> for MaterialWord {
    type Error = String;

    fn try_from(w: String) -> Result<Self, Self::Error> {
        if w == UNKNOWN_WORD {
            return Ok(MaterialWord(None));
        }
        WorkMaterial::from_word(&w)
            .map(|m| MaterialWord(Some(m)))
            .ok_or_else(|| format!("unknown workpiece material {w:?} in a tool's cutting data"))
    }
}

impl From<MaterialWord> for String {
    fn from(m: MaterialWord) -> String {
        m.0.map_or(UNKNOWN_WORD, WorkMaterial::word).to_owned()
    }
}

/// A setup stock's material word (#2015): a group of the built-in
/// table only; any other word refuses the file by name at read.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(try_from = "String", into = "String")]
pub struct StockMaterialWord(pub WorkMaterial);

impl TryFrom<String> for StockMaterialWord {
    type Error = String;

    fn try_from(w: String) -> Result<Self, Self::Error> {
        WorkMaterial::from_word(&w)
            .map(StockMaterialWord)
            .ok_or_else(|| format!("unknown workpiece material {w:?} on a setup's stock"))
    }
}

impl From<StockMaterialWord> for String {
    fn from(m: StockMaterialWord) -> String {
        m.0.word().to_owned()
    }
}

/// One row as the file writes it (mm/min feed, like the tool's own).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct CuttingRowDto {
    pub material: MaterialWord,
    pub rpm: f64,
    pub feed: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stepdown: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stepover: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waterjet: Option<camcraft_tools::waterjet::WaterjetCutData>,
    /// #1521: a torch's recipe for this material and one plate thickness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plasma: Option<camcraft_tools::plasma_cut::PlasmaCutData>,
}

impl CuttingRowDto {
    #[must_use]
    pub fn from_row(r: &CuttingRow) -> Self {
        Self {
            material: MaterialWord(r.material),
            rpm: r.rpm,
            feed: r.feed,
            stepdown: r.stepdown,
            stepover: r.stepover,
            waterjet: r.waterjet,
            plasma: r.plasma,
        }
    }

    #[must_use]
    pub fn to_row(self) -> CuttingRow {
        CuttingRow {
            material: self.material.0,
            rpm: self.rpm,
            feed: self.feed,
            stepdown: self.stepdown,
            stepover: self.stepover,
            waterjet: self.waterjet,
            plasma: self.plasma,
        }
    }
}

/// The rows a tool file writes (#2015): every row but an `Unknown` row
/// that only repeats the tool's own rpm / feed - the reader derives that
/// one back ([`camcraft_tools::cutting_rows::ensure_unknown_row`]), so a
/// tool with just its own numbers saves byte for byte as before.
#[must_use]
pub fn rows_to_write(rows: &[CuttingRow], rpm: f64, feed: f64) -> Vec<CuttingRowDto> {
    rows.iter()
        .filter(|r| !camcraft_tools::cutting_rows::is_derived_unknown(r, rpm, feed))
        .map(CuttingRowDto::from_row)
        .collect()
}
