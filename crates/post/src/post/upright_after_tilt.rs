//! Refuse an operation that opens upright after a tilted operation when
//! the script does not declare `uprightReturn = true` (#3477).
//! Provenance: RS-274 modal rotary words (an unstated A/B/C keeps its
//! last value); the manifest-flag pattern of this repo; our own design.
//!
//! The rotary words are modal. A tilted section ends at its lean, and an
//! upright section that follows states no angle (`post::dump` writes no
//! A/B/C on a move without a leaning axis). Only the script can command
//! the return, at a safe height, before the first X/Y traverse. A script
//! that declares `uprightReturn = true` says it does that; each
//! shipped post that declares it has a posting test of the real order
//! (`crates/post/tests/upright_after_tilt.rs`). A script without it
//! (a user's own `PostRef::File`) gets no such program: the engine
//! refuses it by name, because the cut would run at the old lean.

use super::cnc::Record;
use super::js::{is_true, Script};
use super::PostError;

/// An angle at or under this, in degrees, is zero.
const TOL_DEG: f64 = 1e-6;

/// Whether the script declares `uprightReturn = true`.
fn declared(s: &mut Script) -> Result<bool, PostError> {
    Ok(is_true(&s.eval_src("typeof uprightReturn !== 'undefined' && uprightReturn === true")?))
}

/// The refusal text for the first section that opens with no stated
/// angle while the previous section left the rotaries off zero, or
/// `None` when the script declares the return or no section does that.
///
/// # Errors
/// A script that cannot evaluate its manifest is a clean refusal.
pub(super) fn refusal(s: &mut Script, records: &[Record]) -> Result<Option<String>, PostError> {
    if declared(s)? {
        return Ok(None);
    }
    Ok(first_unstated_return(records))
}

/// The pure law over the stream (no script).
fn first_unstated_return(records: &[Record]) -> Option<String> {
    // The modal A/B/C that the control holds, from the last move that
    // stated them; `None` until a move states them.
    let mut pose: Option<[f64; 3]> = None;
    // The operation that left the rotaries off zero.
    let mut tilted_by: Option<String> = None;
    for chunk in super::wcs_rotate::chunk_bounds(records).into_iter().filter_map(|(a, b)| records.get(a..b)) {
        let name = operation_name(chunk);
        // The first record that commands motion. A canned cycle states no
        // rotary angle, so a cycle before any `Move` opens the section at
        // the modal (old) lean, the same as an unstated move.
        let first = chunk.iter().find_map(|r| match r {
            Record::Move(m) => Some(m.abc),
            Record::Cycle(_) => Some(None),
            _ => None,
        });
        let Some(first_abc) = first else { continue };
        if first_abc.is_none()
            && let Some(prev) = &tilted_by
        {
            return Some(text(&name, prev));
        }
        for r in chunk {
            if let Record::Move(m) = r
                && let Some(abc) = m.abc
            {
                pose = Some(abc);
            }
        }
        // C-axis live-tool work (#33) is section-scoped: the script
        // disengages C at the section's end, so its C angle is not left.
        if is_live_tool(chunk) {
            pose = pose.map(|[a, b, _]| [a, b, 0.0]);
        }
        tilted_by = pose.filter(|p| p.iter().any(|v| v.abs() > TOL_DEG)).map(|_| name);
    }
    None
}

fn operation_name(chunk: &[Record]) -> String {
    chunk
        .iter()
        .find_map(|r| match r {
            Record::Param { name, value } if name == "OperationName" => value.as_str(),
            _ => None,
        })
        .filter(|n| !n.is_empty())
        .unwrap_or("unnamed operation")
        .to_string()
}

fn is_live_tool(chunk: &[Record]) -> bool {
    chunk.iter().any(|r| matches!(r, Record::Param { name, .. } if name == "LiveTool"))
}

fn text(op: &str, prev: &str) -> String {
    format!(
        "operation {op:?} follows the tilted operation {prev:?} and states no rotary angles - the post script does not declare uprightReturn = true, so nothing returns the rotaries to zero at a safe height and the cut would run at the old tilt; post with a script that declares uprightReturn (fanuc, doosan-dnm, mach4, tormach) or add the return to the script and declare it"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cl::{CycleKind, MoveKind};
    use crate::post::cnc::{CycleRec, MoveRec};
    use serde_json::json;

    fn op(name: &str) -> Record {
        Record::Param { name: "OperationName".into(), value: json!(name) }
    }

    fn mv(abc: Option<[f64; 3]>) -> Record {
        Record::Move(MoveRec {
            kind: MoveKind::Rapid,
            x: 0.0,
            y: 0.0,
            z: 20.0,
            feed: None,
            center: None,
            ccw: false,
            plane: Default::default(),
            abc,
        })
    }

    fn section(name: &str, moves: Vec<Record>) -> Vec<Record> {
        let mut v = vec![Record::Section { begin: true }, op(name)];
        v.extend(moves);
        v.push(Record::Section { begin: false });
        v
    }

    #[test]
    fn an_unstated_section_after_a_lean_is_refused_by_name() {
        let r = [section("Lean", vec![mv(Some([0.0, 20.0, 0.0]))]), section("Flat", vec![mv(None)])].concat();
        let why = first_unstated_return(&r).expect("refused");
        assert!(why.contains("\"Flat\"") && why.contains("\"Lean\"") && why.contains("uprightReturn"), "{why}");
    }

    fn cycle() -> Record {
        Record::Cycle(CycleRec {
            kind: CycleKind::Drill,
            x: 10.0,
            y: 10.0,
            r_z: 2.0,
            bottom_z: -5.0,
            entry_z: 20.0,
            peck: 0.0,
            dwell: 0.0,
            first_move: 0,
            feed: Some(100.0),
        })
    }

    #[test]
    fn a_cycle_only_section_after_a_lean_is_refused_by_name() {
        let r = [section("Lean", vec![mv(Some([0.0, 20.0, 0.0]))]), section("FlatDrill", vec![cycle()])].concat();
        let why = first_unstated_return(&r).expect("refused");
        assert!(why.contains("\"FlatDrill\"") && why.contains("\"Lean\""), "{why}");
    }

    #[test]
    fn a_cycle_before_a_later_stated_zero_is_refused() {
        let r = [section("Lean", vec![mv(Some([0.0, 20.0, 0.0]))]), section("FlatDrill", vec![cycle(), mv(Some([0.0; 3]))])].concat();
        assert!(first_unstated_return(&r).is_some());
    }

    #[test]
    fn a_table_c_left_off_zero_is_a_tilt() {
        let r = [section("Spin", vec![mv(Some([0.0, 0.0, 45.0]))]), section("Flat", vec![mv(None)])].concat();
        assert!(first_unstated_return(&r).is_some());
    }

    #[test]
    fn a_stated_zero_and_a_live_tool_c_are_not_refused() {
        let back = [section("Lean", vec![mv(Some([0.0, 20.0, 0.0])), mv(Some([0.0; 3]))]), section("Flat", vec![mv(None)])].concat();
        assert_eq!(first_unstated_return(&back), None);
        let mut live = section("Live", vec![mv(Some([0.0, 0.0, 90.0]))]);
        live.insert(1, Record::Param { name: "LiveTool".into(), value: json!(1) });
        let r = [live, section("Flat", vec![mv(None)])].concat();
        assert_eq!(first_unstated_return(&r), None);
        let restated = [section("Lean", vec![mv(Some([0.0, 20.0, 0.0]))]), section("Lean2", vec![mv(Some([0.0, 30.0, 0.0]))])].concat();
        assert_eq!(first_unstated_return(&restated), None);
    }
}
