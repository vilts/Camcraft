//! #3748: "Pass starts at" Inside or Outside round a part - a selected
//! Helix, Ramp or Plunge lead-in is used as selected and gives one
//! advisory warning that names it, the recommended Arc and the reason;
//! the Arc entry gives none. Provenance: our own regression of the
//! maintainer's rule on #3748; no vendor material consulted.

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::machining2d::{PassStart, entry_advice};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::entry_not_recommended;
use camcraft_app::verify::{AdvisedEntry, Warning};

/// The advice note of the ladder posts one advisory warning with the
/// selected type, at the first cut; no note, no warning. The warning's
/// text is the note's sentence, and its file name is stable.
#[test]
fn the_advice_note_posts_one_advisory_warning() {
    let Some(note) = entry_advice(PassStart::Outside, Entry3D::Helix) else { panic!("no advice for Helix") };
    let path = Toolpath { notes: vec!["link: other".to_owned(), note.to_owned()], ..Default::default() };
    let w = entry_not_recommended(&path).expect("the advice note posts a warning");
    assert_eq!(w, Warning::EntryNotRecommended { at: 0, entry: AdvisedEntry::Helix, inside: false });
    assert!(!w.blocks_post(), "{w}");
    assert!(w.to_string().starts_with(note), "{w}");
    assert!(w.to_string().contains("but Arc is recommended"), "{w}");
    let v = serde_json::to_value(w).expect("serializes");
    assert_eq!(v, serde_json::json!({ "entry_not_recommended": { "at": 0, "entry": "helix", "inside": false } }));
    assert_eq!(entry_not_recommended(&Toolpath::default()), None);
    let inside = entry_advice(PassStart::Inside, Entry3D::Plunge).unwrap_or_default();
    let w = entry_not_recommended(&Toolpath { notes: vec![inside.to_owned()], ..Default::default() });
    assert_eq!(w, Some(Warning::EntryNotRecommended { at: 0, entry: AdvisedEntry::Plunge, inside: true }));
    // Arc and Auto give no advice.
    assert_eq!(entry_advice(PassStart::Outside, Entry3D::Arc), None);
    assert_eq!(entry_advice(PassStart::Auto, Entry3D::Helix), None);
}

/// #3748 pins 1 and 2 on the maintainer's fig8 Outside: the selected Helix
/// runs and the op has exactly one `EntryNotRecommended` warning that
/// names it; with Arc the op has no such warning and no helix move.
#[test]
#[ignore = "heavy lane (#3748 fig8 Outside): two generations and their verify run past the 60 s debug budget - run with --release -- --ignored"]
fn fig8_outside_selected_helix_warns_once_and_arc_not_at_all() {
    let advised = |w: &&String| w.starts_with("EntryNotRecommended");
    let (world, mut h, name, stock) = super::fig8_world();
    let (path, warnings) = super::offset_op_entered(&world, &mut h, &name, stock.0.z, PassStart::Outside, Entry3D::Helix);
    let named: Vec<&String> = warnings.iter().filter(advised).collect();
    assert_eq!(named.len(), 1, "Helix: {named:?}");
    assert!(named[0].contains("entry: Helix") && named[0].contains("inside: false"), "{named:?}");
    let helix = path.moves.iter().any(|m| m.kind != MoveKind::Rapid && m.arc.is_some() && m.from.z - m.to.z > 1e-9);
    assert!(helix, "the selected helix did not run: {:?}", path.notes);
    let (world, mut h, name, stock) = super::fig8_world();
    let (path, warnings) = super::offset_op_entered(&world, &mut h, &name, stock.0.z, PassStart::Outside, Entry3D::Arc);
    assert!(!warnings.iter().any(|w| w.starts_with("EntryNotRecommended")), "Arc: {warnings:?}");
    let helix = path.moves.iter().any(|m| m.kind != MoveKind::Rapid && m.arc.is_some() && m.from.z - m.to.z > 1e-9);
    assert!(!helix, "Arc: a helix move: {:?}", path.notes);
}
