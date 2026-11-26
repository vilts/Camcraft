//! The advice for a selected entry that is not the recommended one on a
//! start-side pass (#3748): "Pass starts at" Inside or Outside round a
//! part enters best by the Arc entry from the air outside the stock. A
//! selected Helix, Ramp or Plunge is used as selected, and the operation
//! says so in one warning. Provenance: our own rule, from the maintainer's
//! words on #3741 and #3748 and general machining practice (enter from the
//! open side); no vendor material consulted.

/// The selected entry that the advice names: each lead-in type but Arc.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdvisedEntry {
    Plunge,
    Ramp,
    Helix,
}

macro_rules! advice {
    ($side:literal, $kind:literal) => {
        concat!(
            "Pass starts at ",
            $side,
            ": the ",
            $kind,
            " entry is used as selected, but Arc is recommended, because an Arc entry starts in the air outside \
             the stock where it fits and never plunges into the material"
        )
    };
}

/// The one sentence of advice: the selected type, the recommended type
/// (Arc) and the reason. `inside`: the pass starts at Inside, else at
/// Outside. The operation's note, its warning and the Lead in page use it.
#[must_use]
pub fn entry_advice(entry: AdvisedEntry, inside: bool) -> &'static str {
    match (entry, inside) {
        (AdvisedEntry::Plunge, true) => advice!("Inside", "Plunge"),
        (AdvisedEntry::Ramp, true) => advice!("Inside", "Ramp"),
        (AdvisedEntry::Helix, true) => advice!("Inside", "Helix"),
        (AdvisedEntry::Plunge, false) => advice!("Outside", "Plunge"),
        (AdvisedEntry::Ramp, false) => advice!("Outside", "Ramp"),
        (AdvisedEntry::Helix, false) => advice!("Outside", "Helix"),
    }
}

#[cfg(test)]
mod tests {
    use super::{AdvisedEntry, entry_advice};

    /// #3748: the advice names the side, the selected type and Arc, in one
    /// sentence.
    #[test]
    fn the_advice_names_the_selected_and_the_recommended_type() {
        let t = entry_advice(AdvisedEntry::Helix, false);
        assert_eq!(
            t,
            "Pass starts at Outside: the Helix entry is used as selected, but Arc is recommended, because an Arc entry \
             starts in the air outside the stock where it fits and never plunges into the material"
        );
        assert!(entry_advice(AdvisedEntry::Ramp, true).starts_with("Pass starts at Inside: the Ramp entry"));
        assert!(entry_advice(AdvisedEntry::Plunge, false).contains("the Plunge entry is used as selected"));
    }
}
