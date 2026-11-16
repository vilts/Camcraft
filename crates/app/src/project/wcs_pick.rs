//! A setup's work-offset pick in the project file (#196, #931): offsets
//! 1-6 keep their legacy words `G54`..`G59` (every file older builds
//! wrote reads byte-identical), offsets 7 and up are their 1-based
//! number. Older builds refuse the number - the release notes say so.
//!
//! Provenance: our own persistence contract (issue #931).

use crate::post::work_offset;
use crate::setup_offsets::MAX_SLOT;

/// The file text of a 0-based pick.
pub(crate) fn work_offset_text(slot: usize) -> String {
    let n = u32::try_from(slot).map_or(u32::MAX, |s| s.saturating_add(1));
    work_offset::legacy_word(n).map_or_else(|| n.to_string(), str::to_owned)
}

/// The 0-based pick a file text names: a legacy word or a whole number
/// `1..=MAX_SLOT + 1`. Anything else is `None` (a corrupt file).
pub(crate) fn work_offset_pick(text: &str) -> Option<usize> {
    let n = work_offset::index_of(&serde_json::Value::String(text.to_owned()))?;
    let slot = usize::try_from(n.checked_sub(1)?).ok()?;
    (slot <= MAX_SLOT).then_some(slot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_words_and_numbers_round_trip() {
        for slot in [0usize, 5, 6, 8, MAX_SLOT] {
            assert_eq!(work_offset_pick(&work_offset_text(slot)), Some(slot), "{slot}");
        }
        assert_eq!(work_offset_text(0), "G54");
        assert_eq!(work_offset_text(5), "G59");
        assert_eq!(work_offset_text(6), "7");
        assert_eq!(work_offset_pick("0"), None);
        assert_eq!(work_offset_pick("G60"), None);
        assert_eq!(work_offset_pick("7.5"), None);
        assert_eq!(work_offset_pick("256"), None);
    }
}
