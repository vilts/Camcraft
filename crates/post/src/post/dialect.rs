//! What a post script and a section say about WHICH spindle turns
//! (#1190): the two facts a caller needs before it hands a section to
//! a turning dialect, each stated once.
//!
//! - [`speaks_turning`]: the script's own manifest. Every shipped post
//!   declares `capabilities`, an integer mask (1 = milling, 2 =
//!   turning). A turning script opens a plain section with the WORK
//!   spindle (`G97 S M03`), so the app's program gate must know it is
//!   about to post through one. A script that declares no mask is read
//!   as not turning - nothing is guessed about a dialect it does not
//!   state.
//! - [`states_live_tool`]: the section's own statement. A path whose
//!   moves state spindle C angles is live-tool work; the stream's
//!   `PARAM LiveTool` is written from this one test (`dump`), and the
//!   app reads the same test, never a second copy.
//!
//! Provenance: our own contract (the manifest mask is the `.cps`
//! reference contract every script in `assets/posts` already states;
//! the C statement is issue #33's). No vendor material consulted.

use super::js::Script;
use super::PostError;
use crate::cl::Toolpath;

/// The `capabilities` bit a turning dialect declares.
const TURNING: f64 = 2.0;

/// The `capabilities` bit a milling dialect declares.
const MILLING: f64 = 1.0;

/// Which spindle a script's plain (non-live) section turns, by the
/// dialect its manifest declares (#1174): what the workholding rpm
/// limit asks before it judges a section's S against the chuck.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkVoice {
    /// The turning bit: a plain section opens `G97 S M03` / `G96` on
    /// the WORK spindle.
    WorkSpindle,
    /// The milling bit only: the S word drives the TOOL spindle.
    ToolSpindle,
    /// No dialect stated (no mask, or one with neither bit - a user's
    /// own script without the manifest line): the caller takes the
    /// stricter reading. #1199 settles undeclared scripts for good.
    Undeclared,
}

/// The [`WorkVoice`] the script's manifest declares.
///
/// # Errors
/// A script that does not load is a clean refusal, as everywhere.
pub fn work_voice(cps: &str) -> Result<WorkVoice, PostError> {
    let mut s = Script::load(cps)?;
    let mask = s.number_global("capabilities")?.unwrap_or(0.0);
    if !mask.is_finite() || mask < 0.0 {
        return Ok(WorkVoice::Undeclared);
    }
    // The mask is a small whole number; read its bits without a
    // float-to-integer cast that could wrap.
    let bit = |b: f64| ((mask / b).floor() % 2.0) >= 1.0;
    Ok(if bit(TURNING) {
        WorkVoice::WorkSpindle
    } else if bit(MILLING) {
        WorkVoice::ToolSpindle
    } else {
        WorkVoice::Undeclared
    })
}

/// The machine kinds a script's manifest states it serves (#1266):
/// `serves = ["plasma"];` (one word or a list; the words are the
/// machine file's kind words - mill, lathe, mill-turn, plasma,
/// wire_edm, sinker_edm). `None` = the manifest states none.
///
/// # Errors
/// A script that does not load, or a `serves` that is empty or holds a
/// word that names no machine kind (a typo must not read as "stated").
pub fn declared_serves(cps: &str) -> Result<Option<Vec<crate::machine::Kind>>, PostError> {
    let mut s = Script::load(cps)?;
    let Some(words) = s.words_global("serves")? else {
        return Ok(None);
    };
    if words.is_empty() {
        return Err(PostError::Script("serves is empty: list the machine kinds the script serves".into()));
    }
    let mut kinds = Vec::new();
    for w in &words {
        let k = crate::machine::Kind::from_word(w)
            .ok_or_else(|| PostError::Script(format!("serves names {w:?}, which is no machine kind (mill, lathe, mill-turn, plasma, wire_edm, sinker_edm)")))?;
        kinds.push(k);
    }
    Ok(Some(kinds))
}

/// Whether the script's manifest declares the turning dialect
/// (`capabilities` with the turning bit set) - [`work_voice`]'s
/// `WorkSpindle`, the one reading of the mask.
///
/// # Errors
/// A script that does not load is a clean refusal, as everywhere.
pub fn speaks_turning(cps: &str) -> Result<bool, PostError> {
    Ok(work_voice(cps)? == WorkVoice::WorkSpindle)
}

/// Whether a path is C-axis live-tool work: some move states the
/// spindle's C angle (issue #33). The stream's `PARAM LiveTool` and
/// every reader that asks "does a live tool cut this section?" share
/// this one test.
#[must_use]
pub fn states_live_tool(path: &Toolpath) -> bool {
    path.moves.iter().any(|m| m.rotary.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::{Move, MoveKind};
    use crate::geometry::Point3;

    #[test]
    fn serves_is_read_from_the_manifest() {
        use crate::machine::Kind;
        assert_eq!(declared_serves("x = 1;").ok(), Some(None));
        assert_eq!(declared_serves("serves = [\"plasma\"];").ok(), Some(Some(vec![Kind::Plasma])));
        assert_eq!(declared_serves("serves = \"Mill-Turn\";").ok(), Some(Some(vec![Kind::MillTurn])));
        assert_eq!(declared_serves("serves = [\"mill\", \"plasma\"];").ok(), Some(Some(vec![Kind::Mill, Kind::Plasma])));
        assert!(declared_serves("serves = [];").is_err());
        assert!(declared_serves("serves = [\"plazma\"];").is_err());
        assert!(declared_serves("serves = ;").is_err());
    }

    #[test]
    fn the_turning_bit_is_read_from_the_manifest_mask() {
        for (text, want) in [
            ("capabilities = 2;", true),
            ("capabilities = 3;", true),
            ("capabilities = 1;", false),
            ("capabilities = 0;", false),
            ("capabilities = 4;", false),
            ("capabilities = -2;", false),
            ("capabilities = \"2\";", false),
            ("capabilities = 0 / 0;", false),
            ("var other = 2;", false),
        ] {
            assert_eq!(speaks_turning(text).expect("loads"), want, "{text}");
        }
        assert!(speaks_turning("capabilities = ;").is_err(), "a script that does not parse refuses");
    }

    #[test]
    fn the_mask_names_the_spindle_a_plain_section_drives() {
        for (text, want) in [
            ("capabilities = 2;", WorkVoice::WorkSpindle),
            ("capabilities = 3;", WorkVoice::WorkSpindle),
            ("capabilities = 1;", WorkVoice::ToolSpindle),
            ("capabilities = 0;", WorkVoice::Undeclared),
            ("capabilities = 4;", WorkVoice::Undeclared),
            ("capabilities = -1;", WorkVoice::Undeclared),
            ("capabilities = 0 / 0;", WorkVoice::Undeclared),
            ("function onOpen() {}", WorkVoice::Undeclared),
        ] {
            assert_eq!(work_voice(text).expect("loads"), want, "{text}");
        }
        let shipped = |cps: &str| work_voice(cps).expect("a shipped post loads");
        assert_eq!(shipped(include_str!("../../../../assets/posts/fanuc-lathe.cps")), WorkVoice::WorkSpindle);
        assert_eq!(shipped(include_str!("../../../../assets/posts/tormach-lathe.cps")), WorkVoice::WorkSpindle);
        assert_eq!(shipped(include_str!("../../../../assets/posts/fanuc.cps")), WorkVoice::ToolSpindle);
    }

    #[test]
    fn a_path_is_live_tool_work_only_when_a_move_states_c() {
        let m = Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 0.0, 0.0), Some(100.0));
        assert!(!states_live_tool(&Toolpath::new(vec![m])));
        let mut c = m;
        c.rotary = Some(0.0);
        assert!(states_live_tool(&Toolpath::new(vec![m, c])));
        assert!(!states_live_tool(&Toolpath::new(Vec::new())));
    }
}
