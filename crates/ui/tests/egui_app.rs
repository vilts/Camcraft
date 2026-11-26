//! The `--ui` entry contract, in-process (run() itself must route the flag -
//! the migration-plan law from M-a). Since M-g the flag is a formality:
//! egui is the only shell, and anything else refuses by name rather than
//! silently drawing a different app.
//! Provenance: our own tests.
//!
//! Gate map (M4.4): covers ui::run, ui::headless_open (#2648).
//!
//! A headless run that draws makes a wgpu device. Such a test takes
//! `gpu_guard` first (#3379); the lib test `gpu_test_lock` pins it.


#[test]
fn egui_headless_cli_exit_code_is_zero() {
    let _gpu = camcraft_ui::headless_gpu::gpu_guard();
    assert_eq!(
        camcraft_ui::run(&[
            String::from("--ui"),
            String::from("egui"),
            String::from("--headless")
        ]),
        0
    );
}

#[test]
fn selecting_the_retired_imgui_shell_refuses_with_rc_2() {
    // The M-g law: no silent fallback. A user scripting `--ui imgui` gets a
    // named refusal and rc 2, before any window or renderer is touched.
    assert_eq!(
        camcraft_ui::run(&[
            String::from("--ui"),
            String::from("imgui"),
            String::from("--headless")
        ]),
        2
    );
}

#[test]
fn ui_flag_without_a_value_refuses_with_rc_2() {
    assert_eq!(camcraft_ui::run(&[String::from("--ui")]), 2);
}

#[test]
fn ui_equals_form_refuses_like_the_space_form() {
    // #2082 review: `--ui=imgui` must refuse with rc 2, not start silently.
    assert_eq!(
        camcraft_ui::run(&[String::from("--ui=imgui"), String::from("--headless")]),
        2
    );
    assert_eq!(camcraft_ui::run(&[String::from("--ui=")]), 2);
}

/// #2648: a file named on a headless start is opened there, or refused by
/// name with rc 2 - never ignored.
fn headless_with(file: &std::path::Path) -> i32 {
    camcraft_ui::run(&[String::from("--headless"), file.display().to_string()])
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("camcraft-ui-2648-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

const TETRA_STL: &str = "solid t
 facet normal 0 0 -1
  outer loop
   vertex 0 0 0
   vertex 0 30 0
   vertex 30 0 0
  endloop
 endfacet
 facet normal 0 -1 0
  outer loop
   vertex 0 0 0
   vertex 30 0 0
   vertex 0 0 30
  endloop
 endfacet
 facet normal -1 0 0
  outer loop
   vertex 0 0 0
   vertex 0 0 30
   vertex 0 30 0
  endloop
 endfacet
 facet normal 1 1 1
  outer loop
   vertex 30 0 0
   vertex 0 30 0
   vertex 0 0 30
  endloop
 endfacet
endsolid t
";

#[test]
fn a_headless_start_with_a_model_file_imports_and_draws_it() {
    let _gpu = camcraft_ui::headless_gpu::gpu_guard();
    let dir = temp_dir("model");
    let part = dir.join("part.stl");
    std::fs::write(&part, TETRA_STL).expect("write part");
    assert_eq!(headless_with(&part), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_headless_start_refuses_a_file_it_cannot_open_by_name() {
    let dir = temp_dir("refuse");
    // Not a model or project extension, and a model path that is not there.
    let notes = dir.join("notes.txt");
    std::fs::write(&notes, "hello").expect("write notes");
    assert_eq!(headless_with(&notes), 2);
    assert_eq!(headless_with(&dir.join("missing.stl")), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_headless_start_with_a_broken_model_fails_by_name() {
    let dir = temp_dir("broken");
    let part = dir.join("broken.stl");
    std::fs::write(&part, "solid nothing\nendsolid nothing\n").expect("write part");
    assert_eq!(headless_with(&part), 1, "a file that does not import is a failed run");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_headless_start_with_a_project_file_opens_and_draws_it() {
    let _gpu = camcraft_ui::headless_gpu::gpu_guard();
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/first-part.camproj");
    assert_eq!(headless_with(&project), 0);
}
