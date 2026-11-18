//! #258: desktop Post dialog (`CommandKind::PostProgramDialog`) and
//! Setup -> Post grouped declared options UI tests.
//!
//! Provenance: our own page and window law; no vendor material consulted.

use camcraft_app::posting;
use camcraft_app::schema::PostRef;
use camcraft_app::setup_post_options;
use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, Vec2};
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

use crate::state::{CommandKind, State};

fn frame(ui: &mut egui::Ui, st: &mut State) {
    super::panes::shortcuts(ui, st);
    st.drain_actions();
    super::post_dialog::window(ui.ctx(), st);
}

#[test]
fn desktop_post_command_opens_dialog_and_applies_declared_script_options_undoably() {
    let mut state = State::new();
    assert!(!state.browser_build);
    assert!(state.post_dialog.is_none(), "fresh State keeps Post dialog closed for snapshots");

    state.pending.push_back(CommandKind::PostProgramDialog);
    state.drain_actions();
    assert!(state.post_dialog.is_some(), "PostProgramDialog opens in-app Post dialog on desktop");

    let mut h = Harness::builder()
        .with_size(Vec2::new(1100.0, 900.0))
        .build_ui_state(frame, state);
    h.run();

    // Key sections & action buttons are visible inside the Post dialog.
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Program number")
            .is_some(),
        "missing Program number"
    );
    assert!(
        h.query_by_role_and_label(Role::TextInput, "Program comment")
            .is_some(),
        "missing Program comment"
    );
    assert!(
        h.query_by(|n| n.role() == Role::CheckBox && n.label().as_deref() == Some("Write tool list"))
            .is_some(),
        "missing Fanuc writeToolList option"
    );
    assert!(h.query_by_role_and_label(Role::Button, "Post…").is_some());
    assert!(h.query_by_role_and_label(Role::Button, "Apply").is_some());
    assert!(h.query_by_role_and_label(Role::Button, "Cancel").is_some());

    // Switching to Operations tab renders the operations view inside the Post dialog, then return to Settings.
    h.get_by_label("Operations").click();
    h.run();
    assert!(h.query_by_label_contains("No operations").is_some());
    h.get_by_label("Settings").click();
    h.run();

    // Toggle "Write tool list" checkbox and click Apply.
    let cb = h.get_by(|n| n.role() == Role::CheckBox && n.label().as_deref() == Some("Write tool list"));
    let rect = cb.rect();
    cb.click();
    h.run_steps(2);
    assert_eq!(
        h.state()
            .post_dialog
            .as_ref()
            .and_then(|d| d.values.get("writeToolList")),
        Some(&camcraft_app::post::options::OptionValue::Bool(true)),
        "clicking Write tool list at {rect:?} must toggle draft value"
    );
    h.get_by_role_and_label(Role::Button, "Apply").click();
    h.run_steps(2);

    let key = setup_post_options::post_key(&PostRef::Builtin);
    let doc = setup_post_options::script_document(&h.state().world);
    assert!(
        doc.for_post(&key).contains_key("writeToolList"),
        "Apply must commit declared option override via undoable command (status={:?}, feedback={:?}, pending={}, dlg_open={}, dlg_vals={:?}, dlg_err={:?})",
        h.state().status,
        h.state().feedback,
        h.state().pending.len(),
        h.state().post_dialog.is_some(),
        h.state().post_dialog.as_ref().map(|d| d.values.get("writeToolList")),
        h.state().post_dialog.as_ref().and_then(|d| d.error.as_ref()),
    );

    // Undo restores default.
    h.input_mut().events.push(Event::ModifiersChanged(Modifiers::COMMAND));
    h.input_mut().events.push(Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    h.run_steps(2);
    let doc_after = setup_post_options::script_document(&h.state().world);
    assert!(
        !doc_after.for_post(&key).contains_key("writeToolList"),
        "Ctrl+Z reverts declared option edit"
    );
    let _ = posting::post_options(&h.state().world);
}

#[test]
fn setup_post_page_also_renders_declared_script_options() {
    let state = State::new();
    let mut h = Harness::builder()
        .with_size(Vec2::new(500.0, 1100.0))
        .build_ui_state(
            |ui, st: &mut State| {
                st.drain_actions();
                super::setup_pages::page(ui, st, "", crate::state::SetupPage::Post);
            },
            state,
        );
    h.run();
    h.get_by_label_contains("Post script options").click();
    h.run_steps(2);
    assert!(
        h.query_by(|n| n.role() == Role::CheckBox && n.label().as_deref() == Some("Write tool list"))
            .is_some(),
        "Setup -> Post must expose declared script options"
    );
}

/// A small legal skim gives the UI tests real files without the demo's deep wall cut.
fn postable_face() -> State {
    use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, SetStock};
    use camcraft_app::operation::{Height, OpParams, SrcRef};
    use camcraft_app::stock::StockShape;
    let mut state = State::new();
    state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    state.drain_actions();
    state.history.execute(&state.world, SetStock::new(
        &state.world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default(),
    ).expect("stock"));
    let tool = camcraft_app::tools::catalog().into_iter()
        .find(|(name, _)| *name == "EM-0400-2FL").expect("catalog tool").1;
    state.history.execute(&state.world, AddTool::new(&state.world, "Mill", tool).expect("tool"));
    let params = OpParams::Face {
        holes: Default::default(), blend_turns: false, tilt: None, stepover: 3.0,
        clearance_z: Height::fixed(25.0), cut_z: Height::fixed(19.5), top_z: None,
        stepdown: None, feed: 1200.0, plunge_feed: 300.0, climb: true,
    };
    state.history.execute(&state.world, CreateOperation::new(
        &state.world, "Face1", "Mill", SrcRef::Stock, params,
    ).expect("face"));
    state.history.execute(&state.world, GenerateOperation::new(&state.world, "Face1").expect("generate"));
    state
}

#[test]
fn dialog_uses_setups_own_post_while_options_follow_document() {
    use camcraft_app::commands::SetSetupPost;
    use camcraft_app::post::options::OptionValue;
    let mut state = postable_face();
    let haas = PostRef::Named("Haas".into());
    state.history.execute(&state.world,
        SetSetupPost::new(&state.world, "Setup1", Some(haas.clone())).expect("own Haas"));
    assert_eq!(posting::document_post_of(&state.world), PostRef::Builtin);
    assert_eq!(state.post_options_target(), None, "only setup follows document options");
    assert!(setup_post_options::script_own(&state.world, Some("Setup1")).is_none());
    let path = std::env::temp_dir().join(format!("camcraft-dialog-haas-{}.nc", std::process::id()));
    let before = posting::post_setup(&state.world, &path).expect("legal Haas program").program;
    assert!(!before.contains("POST: Haas"));
    state.pending.push_back(CommandKind::PostProgramDialog);
    state.drain_actions();
    let dlg = state.post_dialog.as_mut().expect("dialog");
    assert_eq!(dlg.post, haas, "the options owner must not select the post");
    assert_eq!(dlg.schema, posting::declared_options_for_post(&haas).expect("Haas schema"));
    dlg.values.insert("writeVersion".into(), OptionValue::Bool(true));
    let mut h = Harness::builder().with_size(Vec2::new(1100.0, 1100.0)).build_ui_state(frame, state);
    h.run();
    h.get_by_role_and_label(Role::Button, "Apply").click();
    h.run_steps(2);
    let key = setup_post_options::post_key(&haas);
    let doc = setup_post_options::script_document(&h.state().world);
    assert_eq!(doc.for_post(&key).get("writeVersion"), Some(&OptionValue::Bool(true)));
    assert!(doc.for_post(&setup_post_options::post_key(&PostRef::Builtin)).is_empty());
    assert!(setup_post_options::script_own(&h.state().world, Some("Setup1")).is_none());
    let after = posting::post_setup(&h.state().world, &path).expect("Haas with option").program;
    assert!(after.contains("POST: Haas"), "the actual post reads the changed options: {after}");
    h.state_mut().pending.push_back(CommandKind::Undo);
    h.state_mut().drain_actions();
    assert_eq!(posting::post_setup(&h.state().world, &path).expect("undo posts").program, before);
    std::fs::remove_file(path).expect("remove posted file");
}

#[test]
fn dialog_per_operation_post_uses_entered_prefix_in_filenames() {
    let dir = std::env::temp_dir().join(format!("camcraft-dialog-prefix-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("output folder");
    let mut state = postable_face();
    state.project_path = Some(dir.join("part.camproj"));
    state.pending.push_back(CommandKind::SetPerOpOutput(true));
    state.pending.push_back(CommandKind::PostProgramDialog);
    state.drain_actions();
    state.post_dialog.as_mut().expect("dialog").target_dir = dir.display().to_string();
    let mut h = Harness::builder().with_size(Vec2::new(1100.0, 1100.0)).build_ui_state(frame, state);
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Program prefix").click();
    h.run();
    h.input_mut().events.push(Event::Key {
        key: Key::A, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND,
    });
    h.run_steps(2);
    h.get_by_role_and_label(Role::TextInput, "Program prefix").type_text("finish");
    h.run_steps(2);
    assert_eq!(h.state().post_dialog.as_ref().expect("dialog").file_name, "finish");
    h.get_by_role_and_label(Role::Button, "Post…").click();
    h.run_steps(3);
    assert!(h.state().status.starts_with("posted 1 file(s)"), "{}", h.state().status);
    let mut names: Vec<_> = std::fs::read_dir(&dir).expect("output folder").map(|entry| {
        entry.expect("output file").file_name().to_string_lossy().into_owned()
    }).collect();
    names.sort();
    assert_eq!(names, ["finish_01_Face1.nc"], "the entered prefix names the actual files");
    assert_eq!(h.state().post_view.as_ref().expect("viewer").path, dir.join("finish_01_Face1.nc"));
    std::fs::remove_dir_all(dir).expect("remove output folder");
}

#[test]
fn dialog_and_post_page_offer_end_units_only_for_supported_posts() {
    use camcraft_app::schema::{EndUnits, PostOptions};

    let open_dialog = |post: PostRef, end_units: EndUnits| {
        let mut state = State::new();
        if post != PostRef::Builtin {
            state.history.execute(
                &state.world,
                camcraft_app::commands::SelectPost::new(&state.world, post).expect("select post"),
            );
        }
        if end_units != EndUnits::Leave {
            let cur = posting::post_options(&state.world);
            state.history.execute(
                &state.world,
                camcraft_app::commands::SetPostOptions::new(
                    &state.world,
                    PostOptions { end_units, ..cur },
                )
                .expect("set end_units"),
            );
        }
        state.pending.push_back(CommandKind::PostProgramDialog);
        state.drain_actions();
        let mut h = Harness::builder()
            .with_size(Vec2::new(1100.0, 900.0))
            .build_ui_state(frame, state);
        h.run();
        h
    };

    for supported in [
        PostRef::Builtin,
        PostRef::Named("Tormach PathPilot".into()),
        PostRef::Named("grblHAL".into()),
        PostRef::Named("TinyG".into()),
        PostRef::Named("Mazak".into()),
        PostRef::Named("Doosan DNM".into()),
        PostRef::Named("Siemens 840D".into()),
    ] {
        let h = open_dialog(supported.clone(), EndUnits::Leave);
        assert!(
            h.query_by_role_and_label(Role::ComboBox, "End units").is_some(),
            "Post dialog must offer End units for {supported:?}"
        );
        if supported == PostRef::Named("Siemens 840D".into()) {
            let h_inch = open_dialog(supported.clone(), EndUnits::Inch);
            assert!(
                h_inch
                    .query_by(|n| {
                        n.role() == Role::ComboBox && n.value().as_deref() == Some("Inch (G700)")
                    })
                    .is_some(),
                "Siemens 840D dialog must label G700"
            );
        }

        let mut page_state = State::new();
        if supported != PostRef::Builtin {
            page_state.history.execute(
                &page_state.world,
                camcraft_app::commands::SelectPost::new(&page_state.world, supported.clone())
                    .expect("select post"),
            );
        }
        let mut ph = Harness::builder()
            .with_size(Vec2::new(500.0, 1100.0))
            .build_ui_state(
                |ui, st: &mut State| {
                    st.drain_actions();
                    super::setup_pages::page(ui, st, "", crate::state::SetupPage::Post);
                },
                page_state,
            );
        ph.run();
        assert!(
            ph.query_by_label("End in").is_some(),
            "Setup -> Post page must offer End in for {supported:?}"
        );
    }

    for unsupported in [
        PostRef::Named("Haas".into()),
        PostRef::Named("Makera Carvera".into()),
        PostRef::Named("Heidenhain".into()),
        PostRef::Named("Fanuc Lathe".into()),
    ] {
        let h = open_dialog(unsupported.clone(), EndUnits::Leave);
        assert!(
            h.query_by_role_and_label(Role::ComboBox, "End units").is_none(),
            "Post dialog must not offer End units for {unsupported:?}"
        );

        let mut page_state = State::new();
        page_state.history.execute(
            &page_state.world,
            camcraft_app::commands::SelectPost::new(&page_state.world, unsupported.clone())
                .expect("select post"),
        );
        let mut ph = Harness::builder()
            .with_size(Vec2::new(500.0, 1100.0))
            .build_ui_state(
                |ui, st: &mut State| {
                    st.drain_actions();
                    super::setup_pages::page(ui, st, "", crate::state::SetupPage::Post);
                },
                page_state,
            );
        ph.run();
        assert!(
            ph.query_by_label("End in").is_none(),
            "Setup -> Post page must not offer End in for {unsupported:?}"
        );
    }

    // Carried-over end_units on an unsupported post offers a one-click clear, not Inch/mm choices.
    let mut carried = open_dialog(PostRef::Named("Fanuc Lathe".into()), EndUnits::Inch);
    assert!(carried.query_by_label("Clear to Leave modal").is_some());
    carried.get_by_label("Clear to Leave modal").click();
    carried.run_steps(2);
    assert_eq!(
        carried.state().post_dialog.as_ref().expect("dialog").options.end_units,
        EndUnits::Leave
    );
}
