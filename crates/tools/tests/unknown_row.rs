//! #2015: every tool's cutting data holds an `Unknown` row - the numbers
//! it runs when no row names the material, and the fallback for a row's
//! missing stepdown / stepover. Pins the selection order, the fallback,
//! the derivation from a tool's own rpm / feed (never for a kind whose
//! table cannot hold it) and the refusals of unusable steps.
//! Provenance: synthetic values testing the issue's explicit rules.
//! Gate map (#2015): covers tools::cutting_rows.

use camcraft_tools::cutting::WorkMaterial;
use camcraft_tools::cutting_rows::{
    CuttingRow, canonical, ensure_unknown_row, is_derived_unknown, refusal, row_for, row_in_tool, row_keys,
    rows_with_unknown, steps_for, steps_in_tool, unknown_of, unknown_row,
};
use camcraft_tools::tools::{ToolKind, catalog};
use camcraft_tools::waterjet::waterjet_tool;

fn steel() -> CuttingRow {
    CuttingRow { stepdown: Some(2.0), ..CuttingRow::plain(Some(WorkMaterial::MildSteel), 3000.0, 400.0) }
}

fn unknown() -> CuttingRow {
    CuttingRow { stepdown: Some(5.0), stepover: Some(3.0), ..CuttingRow::plain(None, 9000.0, 1200.0) }
}

#[test]
fn the_named_row_wins_then_the_unknown_row_answers() {
    let rows = [unknown(), steel()];
    assert_eq!(row_for(&rows, Some(WorkMaterial::MildSteel)).map(|r| r.rpm), Some(3000.0));
    assert_eq!(row_for(&rows, Some(WorkMaterial::Brass)).map(|r| r.rpm), Some(9000.0), "no brass row: Unknown");
    assert_eq!(row_for(&rows, None).map(|r| r.rpm), Some(9000.0), "no material stated: Unknown");
    assert!(row_for(&[steel()], Some(WorkMaterial::Brass)).is_none(), "a table without Unknown has no fallback");
}

#[test]
fn a_missing_step_falls_back_to_the_unknown_row() {
    let rows = [unknown(), steel()];
    assert_eq!(steps_for(&rows, Some(WorkMaterial::MildSteel)), (Some(2.0), Some(3.0)));
    assert_eq!(steps_for(&rows, Some(WorkMaterial::Brass)), (Some(5.0), Some(3.0)));
    assert_eq!(steps_for(&[steel()], Some(WorkMaterial::MildSteel)), (Some(2.0), None));
    assert_eq!(steps_for(&[], None), (None, None));
}

#[test]
fn a_tool_gets_its_unknown_row_from_its_own_numbers_once() {
    let mut t = catalog().remove(0).1;
    t.cutting = vec![steel()];
    assert!(ensure_unknown_row(&mut t));
    assert_eq!(t.cutting[0], CuttingRow::plain(None, t.rpm, t.feed), "first, from the running numbers");
    assert!(is_derived_unknown(&t.cutting[0], t.rpm, t.feed));
    assert_eq!(t.cutting[1], steel());
    assert!(t.is_sane());
    let again = t.clone();
    assert!(!ensure_unknown_row(&mut t), "never twice");
    assert_eq!(t, again);
    assert_eq!(unknown_row(&t.cutting).map(|r| r.feed), Some(t.feed));
}

#[test]
fn the_derived_row_answers_until_one_is_stated_and_canonical_drops_a_repeat() {
    let mut t = catalog().remove(0).1;
    t.cutting = vec![steel()];
    assert_eq!(unknown_of(&t), Some(CuttingRow::plain(None, t.rpm, t.feed)));
    assert_eq!(row_in_tool(&t, Some(WorkMaterial::Brass)).map(|r| r.rpm), Some(t.rpm));
    assert_eq!(steps_in_tool(&t, Some(WorkMaterial::Brass)), (None, None));
    assert_eq!(rows_with_unknown(&t).len(), 2);
    let mut stored = t.clone();
    ensure_unknown_row(&mut stored);
    canonical(&mut stored);
    assert_eq!(stored, t, "a stored repeat of the running numbers is derived again");
    t.cutting.insert(0, unknown());
    let kept = t.clone();
    canonical(&mut t);
    assert_eq!(t, kept, "a stated Unknown row stays");
    assert_eq!(steps_in_tool(&t, Some(WorkMaterial::Brass)), (Some(5.0), Some(3.0)));
}

#[test]
fn kinds_whose_table_cannot_hold_it_get_none() {
    let mut jet = waterjet_tool(0.8);
    assert!(!ensure_unknown_row(&mut jet), "a waterjet's rows are recipes");
    assert!(unknown_of(&jet).is_none());
    assert!(jet.cutting.is_empty());
    let mut torch = catalog().into_iter().map(|(_, t)| t).find(|t| t.kind == ToolKind::Torch).expect("a torch");
    assert!(ensure_unknown_row(&mut torch), "a torch row states rpm 0");
    assert!(torch.is_sane());
}

#[test]
fn unusable_steps_and_a_second_unknown_row_are_refused() {
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let r = CuttingRow { stepover: Some(bad), ..unknown() };
        assert!(refusal(&[r], false).is_some_and(|s| s.contains("Unknown") && s.contains("stepover")), "{bad}");
        let r = CuttingRow { stepdown: Some(bad), ..steel() };
        assert!(refusal(&[r], false).is_some_and(|s| s.contains("stepdown")), "{bad}");
    }
    assert!(refusal(&[unknown(), unknown()], false).is_some_and(|s| s.contains("Unknown twice")));
    assert_eq!(row_keys().first(), Some(&None), "the editor offers Unknown first");
    assert_eq!(row_keys().len(), WorkMaterial::ALL.len() + 1);
}
