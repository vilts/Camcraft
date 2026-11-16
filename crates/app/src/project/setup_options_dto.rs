//! #389: a setup's own post options as files hold them. The key is absent
//! when the setup follows the document's options (every earlier file, and
//! every setup never given its own), so the absent key is the whole
//! migration and an untouched document saves byte for byte as before -
//! additive, no version step, like the document's #110 / #111 switches.
//! Owned options are spelled in full: absent already means "follows".
//!
//! Provenance: our own file grammar (the `PostOptionsDto` words restated
//! for one setup); no vendor material consulted.

use serde::{Deserialize, Serialize};

use crate::schema::{BetweenSetups, EndUnits, PostOptions, ThreadingOutput};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetupPostOptionsDto {
    /// `"g32"` (the passes) or `"g76"` (the cycle).
    pub threading: String,
    pub skip_stops: bool,
    pub per_op_files: bool,
    /// #596's end-of-program unit: absent when left modal (the default),
    /// so files written before it read the same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_units: Option<String>,
    /// #616's per-operation smoothing: absent is off, so files written
    /// before it read the same.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub smoothing: bool,
    /// #1386's Explain G-code: absent is off, so files written before
    /// it read the same.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub explain: bool,
    /// #1532's between-setups choice: absent is the default pause, so
    /// files written before it read the same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub between_setups: Option<String>,
}

impl SetupPostOptionsDto {
    #[must_use]
    pub fn from_options(o: &PostOptions) -> Self {
        Self {
            threading: match o.threading {
                ThreadingOutput::G32Passes => "g32".to_owned(),
                ThreadingOutput::G76Cycle => "g76".to_owned(),
            },
            skip_stops: o.skip_stops,
            per_op_files: o.per_op,
            end_units: match o.end_units {
                EndUnits::Leave => None,
                EndUnits::Inch => Some("inch".to_owned()),
                EndUnits::Millimeters => Some("mm".to_owned()),
            },
            smoothing: o.smoothing,
            explain: o.explain,
            between_setups: (o.between_setups != BetweenSetups::default()).then(|| o.between_setups.word().to_owned()),
        }
    }

    /// The live value; an unspellable thread output is a corrupt file,
    /// refused by the setup's name.
    pub fn to_options(&self, setup: &str) -> Result<PostOptions, String> {
        let threading = match self.threading.as_str() {
            "g32" => ThreadingOutput::G32Passes,
            "g76" => ThreadingOutput::G76Cycle,
            other => return Err(format!("setup '{setup}' names an unknown threading output {other:?}")),
        };
        let end_units = match self.end_units.as_deref() {
            None => EndUnits::Leave,
            Some("inch") => EndUnits::Inch,
            Some("mm") => EndUnits::Millimeters,
            Some(other) => return Err(format!("setup '{setup}' names an unknown end-of-program unit {other:?}")),
        };
        let between_setups = match self.between_setups.as_deref() {
            None => BetweenSetups::default(),
            Some(w) => BetweenSetups::from_word(w)
                .ok_or_else(|| format!("setup '{setup}' names an unknown between-setups choice {w:?}"))?,
        };
        Ok(PostOptions {
            threading,
            skip_stops: self.skip_stops,
            per_op: self.per_op_files,
            end_units,
            smoothing: self.smoothing,
            explain: self.explain,
            between_setups,
        })
    }
}
