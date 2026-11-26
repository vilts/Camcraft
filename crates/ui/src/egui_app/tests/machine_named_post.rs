//! #2374: a machine whose file names its post (`controller.post`) is
//! offered that post on the setup's Post page, in the same banner as the
//! kind offer and through the same `SelectPost`, so undo puts the
//! previous post back. The mount itself never changes the post.
//! Provenance: our own test over our own shipped machine file.
use super::*;
use camcraft_app::posting;
use camcraft_app::schema::PostRef;

const LINE: &str = "the machine names grblHAL as its post; switch to it?";

fn post(harness: &Harness<'static, ShellUi>) -> PostRef {
    posting::post_of(&harness.state().state.world)
}

#[test]
fn the_grblhal_machine_offers_its_own_post_and_the_switch_rides_undo() {
    let mut harness = shell_harness();
    let pending = |h: &mut Harness<'static, ShellUi>, c: CommandKind| h.state_mut().state.pending.push_back(c);
    pending(&mut harness, CommandKind::CreateSetup { name: None, datum_from: None });
    harness.run_steps(2);
    crate::actions::target(
        &mut harness.state_mut().state,
        UiTarget::SetupNode("Setup1".into(), crate::state::SetupPage::Post),
    );
    harness.run();
    assert!(harness.query_by_label_contains(LINE).is_none(), "no machine named yet - no offer");
    pending(&mut harness, CommandKind::LoadBuiltinMachine("grblHAL 3-axis".to_owned()));
    harness.run();
    harness.run();
    assert_eq!(post(&harness), PostRef::Builtin, "the mount never re-points the post by itself");
    assert!(
        harness.query_by_label_contains(LINE).is_some(),
        "the offer names the machine's post (status: {})",
        harness.state().state.status
    );
    harness.get_by_label("Switch post").scroll_to_me();
    harness.run();
    harness.get_by_label("Switch post").click();
    harness.run();
    harness.run();
    assert_eq!(post(&harness), PostRef::Named("grblHAL".to_owned()), "taken through SelectPost");
    assert!(harness.query_by_label_contains(LINE).is_none(), "the machine's post retires the offer");
    pending(&mut harness, CommandKind::Undo);
    harness.run();
    harness.run();
    assert_eq!(post(&harness), PostRef::Builtin, "undo restores Fanuc");
    assert!(harness.query_by_label_contains(LINE).is_some(), "and the offer stands again");
}

/// #2374: the Machines pane's file editor gives a user's own machine its
/// post from a dropdown, and "(none)" takes the key out again. The save
/// writes `controller.post`; nothing touches the document's history.
#[test]
fn a_users_own_machine_picks_its_post_from_the_editors_dropdown() {
    use super::machines_edit::{pick_kind as pick, press, read_json, select_row, user_dir};
    use super::machines_pane::{assert_no_commands, harness_with_user_dir, open_machines};
    let dir = user_dir("named_post");
    let path = dir.join("shop.mch");
    let text = r#"{
      "general": { "model": "SHOP GRBL", "kind": "mill" },
      "kinematics": { "default": { "units": { "length": "mm" }, "parts": [
        { "name": "X", "type": "linear" }, { "name": "Y", "type": "linear" }, { "name": "Z", "type": "linear" }
      ] } }
    }"#;
    std::fs::write(&path, text).expect("write");
    let mut h = harness_with_user_dir(Some(dir.clone()));
    h.run();
    open_machines(&mut h);
    select_row(&mut h, "SHOP GRBL");
    press(&mut h, "Edit machine");
    let draft = |h: &Harness<'static, ShellUi>| {
        h.state().state.machines_pane.editor.as_ref().and_then(|e| e.draft.value.pointer("/controller/post").cloned())
    };
    pick(&mut h, "(none)", "grblHAL");
    assert_eq!(draft(&h), Some("grblHAL".into()), "the pick is in the draft");
    h.get_by_label("Validation: passes");
    press(&mut h, "Save machine");
    press(&mut h, "Write file");
    assert_eq!(read_json(&path).pointer("/controller/post").and_then(|v| v.as_str()), Some("grblHAL"));
    pick(&mut h, "grblHAL", "(none)");
    assert_eq!(draft(&h), None, "(none) leaves the post unstated");
    assert_no_commands(&h);
    let _ = std::fs::remove_dir_all(&dir);
}
