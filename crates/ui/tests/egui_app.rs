//! The `--ui` entry contract, in-process (run() itself must route the flag -
//! the migration-plan law from M-a). Since M-g the flag is a formality:
//! egui is the only shell, and anything else refuses by name rather than
//! silently drawing a different app.
//! Provenance: our own tests.
//!
//! Gate map (M4.4): covers ui::run.


#[test]
fn egui_headless_cli_exit_code_is_zero() {
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
