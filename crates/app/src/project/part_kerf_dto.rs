//! #3469: the part-off kerf setting in the project file - the piece kept (`kept`: `cut_off` / `chuck`) and the back stock (mm). Our own format; the additive law: each key is absent at its default, so a file saved before the keys reads the #3452 kerf and an unchanged op writes the same file.

use serde::{Deserialize, Serialize};

use crate::turning::{PartKept, PartKerf};

/// The piece of a part-off that is the finished part, in the file.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PartKeptDto {
    #[default]
    CutOff,
    Chuck,
}

impl PartKeptDto {
    pub(super) fn is_default(&self) -> bool {
        *self == PartKeptDto::CutOff
    }
}

/// The back stock key is written only when it is not 0.
pub(super) fn no_back_stock(v: &f64) -> bool {
    *v == 0.0
}

/// The setting from its two file keys.
pub(super) fn kerf_of(kept: PartKeptDto, back_stock: f64) -> PartKerf {
    let kept = match kept {
        PartKeptDto::CutOff => PartKept::CutOff,
        PartKeptDto::Chuck => PartKept::Chuck,
    };
    PartKerf { kept, back_stock }
}

/// The two file keys of a setting.
pub(super) fn kerf_dto(k: PartKerf) -> (PartKeptDto, f64) {
    let kept = match k.kept {
        PartKept::CutOff => PartKeptDto::CutOff,
        PartKept::Chuck => PartKeptDto::Chuck,
    };
    (kept, k.back_stock)
}
