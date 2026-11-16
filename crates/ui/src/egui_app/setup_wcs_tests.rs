//! #196 in the surface: the Setups page's work-offset picker queues
//! `SetSetupWorkOffset` (never a world poke), the drain lands it as one
//! undo unit, Ctrl+Z / Ctrl+Y walk it, and a refused pick pushes nothing.
//! Provenance: our own page law; no vendor material consulted.

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, Vec2};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;

use crate::state::{CommandKind, State};

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

fn with_setup() -> State {
    let mut state = State::new();
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    state
}

fn pick(state: &State) -> Option<usize> {
    camcraft_app::setup::work_offset(&state.world, "Setup1")
}

fn frame(ui: &mut egui::Ui, st: &mut State) {
    super::panes::shortcuts(ui, st);
    st.drain_actions();
    super::setup_wcs_pick::picker(ui, st, "Setup1");
}

fn key(h: &mut Harness<'static, State>, k: Key) {
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::COMMAND));
    h.input_mut().events.push(Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    h.run_steps(2);
}

#[test]
fn the_picker_sets_the_setups_work_offset_through_the_command() {
    let mut h = Harness::builder()
        .with_size(Vec2::new(500.0, 400.0))
        .build_ui_state(frame, with_setup());
    h.run();
    assert_eq!(pick(h.state()), None, "a fresh setup is unset - it reads offset 1");
    assert!(h.state().pending.is_empty(), "drawing the picker queues nothing");
    let auto = super::setup_wcs_pick::pick_text(&h.state().world, "Setup1", None);
    h.get_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some(auto.as_str())).click();
    h.run();
    h.get_by_label("Work offset 2 (G55)").click();
    h.run();
    assert_eq!(pick(h.state()), Some(1), "the drain lands G55");
    assert_eq!(h.state().status, "Setup1 WCS posts as G55");
    key(&mut h, Key::Z);
    assert_eq!(pick(h.state()), None, "undo restores G54 (unset)");
    key(&mut h, Key::Y);
    assert_eq!(pick(h.state()), Some(1), "redo restores G55");
}

#[test]
fn a_refused_pick_changes_nothing_and_says_why() {
    let mut state = with_setup();
    run(&mut state, CommandKind::SetSetupWorkOffset { setup: "Setup1".into(), slot: Some(9) });
    assert_eq!(pick(&state), None);
    assert!(state.status.contains("work offset not changed"), "{}", state.status);
    // The refusal pushed nothing: the one undo unit is the CreateSetup.
    run(&mut state, CommandKind::Undo);
    assert!(camcraft_app::setup::names(&state.world).is_empty());
    assert!(!state.history.can_undo());
}

/// #329: the Heights page's Work offset reference offers no slot of its
/// own - one combo (From), and the setup's offset shown read-only with
/// the pointer to the Setups page. Picking G55 on the setup changes what
/// the row shows; the height itself is untouched.
#[test]
fn the_heights_page_shows_the_setups_offset_with_no_wcs_choice() {
    use camcraft_app::operation::{Height, HeightReference};
    let frame = |ui: &mut egui::Ui, st: &mut (State, Height)| {
        let (state, height) = st;
        super::heights_page::height_editor(ui, "Cut Z", &state.world, Some("Setup1"), height, "t.cut");
    };
    let height = Height::of(HeightReference::WorkOffset, 0.0);
    let combos = |h: &Harness<'static, (State, Height)>| {
        h.query_all_by(|n| n.role() == Role::ComboBox).count()
    };
    let shown = |h: &Harness<'static, (State, Height)>, text: &str| {
        h.query_all_by(|n| {
            n.label().is_some_and(|l| l.contains(text)) || n.value().is_some_and(|v| v.contains(text))
        })
        .count()
            > 0
    };
    let mut h = Harness::builder()
        .with_size(Vec2::new(500.0, 300.0))
        .build_ui_state(frame, (with_setup(), height));
    h.run();
    assert_eq!(combos(&h), 1, "only the From combo - no per-height WCS choice");
    assert!(shown(&h, "WCS (G54)  (from setup Setup1)"), "an unset setup reads G54");
    run(&mut h.state_mut().0, CommandKind::SetSetupWorkOffset { setup: "Setup1".into(), slot: Some(1) });
    h.run();
    assert_eq!(combos(&h), 1);
    assert!(shown(&h, "WCS (G55)  (from setup Setup1)"), "follows the setup's pick");
    assert_eq!(h.state().1, Height::of(HeightReference::WorkOffset, 0.0), "the height is untouched");
}

/// #329, #931: unset and an explicit offset 1 are one entry, worded
/// "Work offset n (NAME)" from the setup's post.
#[test]
fn an_unset_setup_reads_work_offset_one() {
    use super::setup_wcs_pick::pick_text;
    let state = with_setup();
    let w = &state.world;
    assert_eq!(pick_text(w, "Setup1", None), "Work offset 1 (G54)");
    assert_eq!(pick_text(w, "Setup1", Some(0)), "Work offset 1 (G54)");
    assert_eq!(pick_text(w, "Setup1", Some(1)), "Work offset 2 (G55)");
}

/// The combo's entries, read from the AccessKit tree with the popup open.
fn entries(h: &mut Harness<'static, State>) -> Vec<String> {
    let shown = super::setup_wcs_pick::pick_text(&h.state().world, "Setup1", pick(h.state()));
    h.get_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some(shown.as_str())).click();
    h.run();
    let mut out: Vec<String> = h
        .query_all_by(|n| n.label().is_some_and(|l| l.starts_with("Work offset ")) && n.role() != Role::ComboBox)
        .filter_map(|n| n.accesskit_node().label())
        .collect();
    out.dedup();
    h.key_press(Key::Escape);
    h.run();
    out
}

/// #931 slice 3: the combo lists offsets `1..=max` of the setup's OWN
/// post - six on the default post, nine on Tormach PathPilot (G59.1 -
/// G59.3) - each worded "Work offset n (NAME)".
#[test]
fn the_combo_lists_the_setups_post_range_by_its_names() {
    let mut h = Harness::builder()
        .with_size(Vec2::new(500.0, 600.0))
        .build_ui_state(frame, with_setup());
    h.run();
    let six = entries(&mut h);
    assert_eq!(six.len(), 6, "{six:?}");
    assert_eq!(six.first().map(String::as_str), Some("Work offset 1 (G54)"), "{six:?}");
    assert_eq!(six.last().map(String::as_str), Some("Work offset 6 (G59)"), "{six:?}");

    run(h.state_mut(), CommandKind::SelectPostNamed("Tormach PathPilot".into()));
    h.run();
    let nine = entries(&mut h);
    assert_eq!(nine.len(), 9, "{nine:?}");
    assert_eq!(nine.get(6).map(String::as_str), Some("Work offset 7 (G59.1)"), "{nine:?}");
    assert_eq!(nine.last().map(String::as_str), Some("Work offset 9 (G59.3)"), "{nine:?}");
}

/// #931: a stored pick the post cannot express stays visible as itself
/// (never read as offset 1); the post gate refuses it by setup name.
#[test]
fn a_pick_beyond_the_post_stays_visible_and_says_so() {
    let mut state = with_setup();
    run(&mut state, CommandKind::SelectPostNamed("Tormach PathPilot".into()));
    run(&mut state, CommandKind::SetSetupWorkOffset { setup: "Setup1".into(), slot: Some(8) });
    assert_eq!(pick(&state), Some(8), "{}", state.status);
    run(&mut state, CommandKind::SelectPostBuiltin);
    let mut h = Harness::builder()
        .with_size(Vec2::new(500.0, 600.0))
        .build_ui_state(frame, state);
    h.run();
    let shown = super::setup_wcs_pick::pick_text(&h.state().world, "Setup1", Some(8));
    assert_eq!(shown, "Work offset 9 - not on this post");
    let all = entries(&mut h);
    assert_eq!(all.len(), 7, "1-6 of the post, then the held 9 alone: {all:?}");
    assert_eq!(all.last().map(String::as_str), Some("Work offset 9 - not on this post"), "{all:?}");
}
