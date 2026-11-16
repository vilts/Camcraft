//! The file mirror of a tool's per-material cutting data (#890): a list
//! of `{"material": <word>, "rpm": n, "feed": n}` under the tool's
//! additive `cutting_data` key. The material is the same word the
//! document's `material` key writes (`WorkMaterial::word`); an unknown
//! word refuses the file by name at read, never a silently dropped row.
//!
//! Provenance: our own file format, the M13.2 material-word law reused.

use serde::{Deserialize, Serialize};

use crate::tools::WorkMaterial;
use camcraft_tools::cutting_rows::CuttingRow;

/// A material word that names a row of the built-in table.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(try_from = "String", into = "String")]
pub struct MaterialWord(pub WorkMaterial);

impl TryFrom<String> for MaterialWord {
    type Error = String;

    fn try_from(w: String) -> Result<Self, Self::Error> {
        WorkMaterial::from_word(&w)
            .map(MaterialWord)
            .ok_or_else(|| format!("unknown workpiece material {w:?} in a tool's cutting data"))
    }
}

impl From<MaterialWord> for String {
    fn from(m: MaterialWord) -> String {
        m.0.word().to_owned()
    }
}

/// One row as the file writes it (mm/min feed, like the tool's own).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct CuttingRowDto {
    pub material: MaterialWord,
    pub rpm: f64,
    pub feed: f64,
}

impl CuttingRowDto {
    #[must_use]
    pub fn from_row(r: &CuttingRow) -> Self {
        Self { material: MaterialWord(r.material), rpm: r.rpm, feed: r.feed }
    }

    #[must_use]
    pub fn to_row(self) -> CuttingRow {
        CuttingRow { material: self.material.0, rpm: self.rpm, feed: self.feed }
    }
}
