//! Native Agie job export through the desktop Post dialog, using our synthetic punch.
//! Gate map: covers egui_app::post_dialog, egui_app::post_dialog_tabs and state::State.

use super::{CommandKind, Harness, Queryable, ShellUi, Vec2, frame};
use camcraft_app::{post::options::OptionValue, posting::agie, schema::PostRef};
use egui::accesskit::Role;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn open_punch(tag: &str) -> (Fixture, Harness<'static, ShellUi>) {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "camcraft-agie-{tag}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir(&dir).unwrap();
    let fixture = Fixture(dir);
    let output = fixture.0.join("jobs");
    fs::create_dir(&output).unwrap();
    let project = fixture.0.join("punch.camproj");
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/agie");
    let script = fs::read_to_string(example.join("punch.camscript")).unwrap();
    // The desktop generates and posts the operation itself. Use the example only to
    // create the input document. Quote filesystem paths as script string literals.
    let (operations, _) = script.split_once("cam.generate();").unwrap();
    let obj = serde_json::to_string(&example.join("punch.obj")).unwrap();
    let operations = operations.replace("\"examples/agie/punch.obj\"", &obj);
    let save_as = serde_json::to_string(&project).unwrap();
    let result = camcraft_app::script::run(&format!("{operations}\ncam.save({save_as});"));
    assert!(result.ok && result.errors.is_empty(), "{:?}", result.errors);

    let mut shell = ShellUi::blank();
    let st = &mut shell.state;
    st.pending
        .push_back(CommandKind::OpenProjectFile(project.clone()));
    st.settle();
    assert_eq!(st.project_path.as_ref(), Some(&project), "{}", st.status);
    st.pending
        .push_back(CommandKind::SelectPostNamed(agie::POST.into()));
    st.pending
        .push_back(CommandKind::GenerateOperation("Punch1".into()));
    st.settle();
    st.pending.push_back(CommandKind::SetPostScriptOptions {
        setup: st.post_options_target(),
        post: PostRef::Named(agie::POST.into()),
        overrides: [("separationLengthMm".into(), OptionValue::Float(1.0))].into(),
    });
    // A nonempty parent exercises the direct export without opening a platform file picker.
    st.post_last_dir = Some(output);
    st.post_last_name = Some("CCSEP20".into());
    st.pending.push_back(CommandKind::PostProgramDialog);
    let mut h = Harness::builder()
        .with_size(Vec2::new(1440.0, 1100.0))
        .build_ui_state(frame, shell);
    h.run();
    (fixture, h)
}

fn click_post(h: &mut Harness<'static, ShellUi>) {
    h.get_by_role_and_label(Role::Button, "Post…").click();
    h.run_steps(3);
}

#[test]
fn native_post_dialog_writes_three_files_and_reports_the_required_work_checks() {
    let (fixture, mut h) = open_punch("prepared");
    let name = h.get_by_role_and_label(Role::TextInput, "Job folder name");
    assert_eq!(name.value().as_deref(), Some("CCSEP20"));
    h.get_by_label_contains("Requested finish: Fine Ra 0.30 µm");
    h.get_by_label(
        "Check the effective work quality on the control before assigning cutting steps.",
    );

    click_post(&mut h);
    let status = &h.state().state.status;
    assert!(h.state().state.post_dialog.is_none(), "{status}");
    for check in [
        "3 controller files, zero working steps",
        "Select the newly created piece",
        "CCB030 requests Ra 0.30",
        "Assign the matching TECCUT row",
        "245, 155, 140 and 134 µm",
    ] {
        assert!(status.contains(check), "missing {check}: {status}");
    }
    h.get_by_label_contains("Native wire EDM job prepared in");
    let job = fixture.0.join("jobs/CCSEP20");
    assert_eq!(fs::read_dir(&job).unwrap().count(), 3);
    let goldens = Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/tests/golden/nc/agie-native");
    for file in ["CCSEP20.ISO", "CCSEP20.SBR", "CCSEP20.SBL"] {
        assert_eq!(
            fs::read(job.join(file)).unwrap(),
            fs::read(goldens.join(file)).unwrap(),
            "{file}"
        );
    }
}

#[test]
fn native_post_dialog_refuses_an_invalid_job_name_without_writing_files() {
    let (fixture, mut h) = open_punch("invalid-name");
    h.get_by_role_and_label(Role::TextInput, "Job folder name")
        .click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Job folder name")
        .type_text("bad");
    h.run_steps(2);
    click_post(&mut h);
    h.get_by_label(
        "Enter a job name of up to eight uppercase letters or digits, starting with a letter",
    );
    assert_eq!(fs::read_dir(fixture.0.join("jobs")).unwrap().count(), 0);
}

#[test]
fn native_post_dialog_refuses_per_operation_output_without_writing_files() {
    let (fixture, mut h) = open_punch("per-op");
    h.get_by_role_and_label(Role::ComboBox, "Output").click();
    h.run();
    h.get_by_label("One file per operation").click();
    h.run_steps(2);
    click_post(&mut h);
    h.get_by_label("Choose One program to create a native wire EDM job directory");
    assert_eq!(fs::read_dir(fixture.0.join("jobs")).unwrap().count(), 0);
}
