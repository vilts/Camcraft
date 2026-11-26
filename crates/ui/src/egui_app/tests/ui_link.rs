//! #3751 in the window: `camcraft-cli --ui call ...` finds the window by
//! its link file, and the shell's own frame loop answers each call on the
//! window's History - the browser shows the new operation, and the
//! user's Ctrl+Z removes it.
//! Provenance: our own tests over our own shell; no HSMWorks material.

use super::*;
use super::super::browser_tree::Leaf;
use camcraft_app::mcp::Inbox;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn has_op_row(state: &State, name: &str) -> bool {
    let (_, leaves) = panes::browser_rows(state);
    leaves.iter().any(|l| *l == Leaf::Op(name.to_owned()))
}

fn ctrl_z(harness: &mut Harness<'static, ShellUi>) {
    harness.input_mut().events.push(Event::ModifiersChanged(Modifiers::COMMAND));
    harness.input_mut().events.push(Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    harness.run_steps(2);
}

/// The `--ui` lane as the CLI runs it, on a thread (the CLI process
/// stand-in), while this thread draws the window's frames.
fn cli_ui(harness: &mut Harness<'static, ShellUi>, dir: &std::path::Path, args: &[&str]) -> i32 {
    let dir = dir.to_path_buf();
    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    let client = std::thread::spawn(move || camcraft_app::cli_ui::run_in(Some(dir), &args));
    let deadline = Instant::now() + Duration::from_secs(30);
    while !client.is_finished() {
        assert!(Instant::now() < deadline, "the --ui call did not finish");
        harness.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    client.join().expect("the CLI thread")
}

#[test]
fn a_cli_ui_call_shows_in_the_window_and_ctrl_z_removes_it() {
    let dir: PathBuf = std::env::temp_dir().join(format!("camcraft-ui-link-window-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut harness = Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::blank());
    let inbox = Inbox::link(&dir, Box::new(|| {})).expect("the window opens its link");
    harness.state_mut().state.ui_link = Some(inbox);

    for (tool, args) in [
        ("set_stock", r#"{"box":{"dx":60,"dy":60,"dz":20}}"#),
        ("add_tool", r#"{"catalog":"EM-0400-2FL"}"#),
    ] {
        assert_eq!(cli_ui(&mut harness, &dir, &["call", tool, args]), 0, "{tool}");
    }
    assert!(!has_op_row(&harness.state().state, "Face1"));
    let code = cli_ui(&mut harness, &dir, &["call", "add_operation", r#"{"kind":"Face","tool":"EM-0400-2FL"}"#]);
    assert_eq!(code, 0, "the window ran the call");
    harness.run();
    let st = &harness.state().state;
    assert!(has_op_row(st, "Face1"), "the browser lists the CLI's operation");
    assert_eq!(st.status, "agent: add_operation");

    // The user's own Ctrl+Z undoes the CLI's step: it sits on the History.
    ctrl_z(&mut harness);
    let st = &harness.state().state;
    assert!(!has_op_row(st, "Face1"), "Ctrl+Z removed the CLI's operation");
    assert!(st.history.can_redo());

    // A refused tool call is exit 1, and the window changes nothing.
    let code = cli_ui(&mut harness, &dir, &["call", "add_operation", r#"{"kind":"Face","tool":"no-such-tool"}"#]);
    assert_eq!(code, 1, "a refusal exits 1");
    assert!(!has_op_row(&harness.state().state, "Face1"));

    // The window's link file goes with the window.
    harness.state_mut().state.ui_link = None;
    assert!(camcraft_app::mcp::link::entries(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
