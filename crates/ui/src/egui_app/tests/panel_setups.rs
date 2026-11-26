//! #2339: save a panel setup, change the arrangement, load the saved one
//! back - and get it back after a restart too. The structural suite pins
//! the round trip as behaviour: which panels are open, where each is
//! attached (a split or a float), its leaf size, its tab order, its active
//! tab and its collapsed state. A corrupt or older file loads the default
//! layout with a diagnostic, never a crash.
//! Gate map: covers ui::egui_app::layout::setups, ui::state (state_setups)
//! and camcraft_app::prefs (the setups file).
//! Provenance: our own tests, specced from the egui_dock 0.21 and
//! egui_kittest vendored sources; no HSMWorks material involved.

use egui_dock::{Node, NodePath, Split, Surface, SurfaceIndex};

use camcraft_app::prefs::{self, PanelSetups, SavedSetup};

use super::super::layout;
use super::*;
use crate::state::PanelSetupAsk;

/// A store file of its own, so two runs of the suite never share one.
fn store(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("camcraft-setups-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a temp folder for the setups");
    dir.join(prefs::SETUPS_FILE_NAME)
}

fn harness_with_store(path: std::path::PathBuf) -> Harness<'static, ShellUi> {
    let mut sh = ShellUi::blank();
    sh.state.load_panel_setups_from(path);
    Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, sh)
}

/// One panel of the dock as the user sees it: its tab, its surface, its
/// leaf's rect (size and side), its active state and its collapsed state.
#[derive(Debug)]
struct Leaf {
    tab: &'static str,
    surface: SurfaceIndex,
    rect: egui::Rect,
    active: bool,
    collapsed: bool,
}

fn leaves(dock: &DockState<Tab>) -> Vec<Leaf> {
    let mut out: Vec<Leaf> = dock
        .iter_all_tabs()
        .filter_map(|(path, tab)| {
            dock.leaf(path.node_path()).ok().map(|leaf| Leaf {
                tab: tab.title(),
                surface: path.surface,
                rect: leaf.rect,
                active: leaf.active == path.tab,
                collapsed: leaf.collapsed,
            })
        })
        .collect();
    out.sort_by_key(|l| l.tab);
    out
}

/// The rect of each floating window, in surface order.
fn floats(dock: &DockState<Tab>) -> Vec<(usize, egui::Rect)> {
    dock.iter_surfaces_indexed()
        .filter_map(|(i, s)| match s {
            Surface::Window(_, state) => Some((i.0, state.rect())),
            _ => None,
        })
        .collect()
}

/// Rect equality that treats the two "not laid out yet" rects as equal and
/// allows a point of rounding (egui_dock lays out in floats).
fn same_rect(a: egui::Rect, b: egui::Rect) -> bool {
    if a == b {
        return true;
    }
    (a.min - b.min).length() < 1.0 && (a.max - b.max).length() < 1.0
}

fn assert_same(got: &DockState<Tab>, want: &DockState<Tab>) {
    let (a, b) = (leaves(got), leaves(want));
    assert_eq!(a.len(), b.len(), "panel count: {a:?} vs {b:?}");
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(
            (x.tab, x.surface, x.active, x.collapsed),
            (y.tab, y.surface, y.active, y.collapsed),
            "same panel, surface and state"
        );
        assert!(
            same_rect(x.rect, y.rect),
            "{} back at its saved rect: {:?} vs {:?}",
            x.tab,
            x.rect,
            y.rect
        );
    }
    let (fa, fb) = (floats(got), floats(want));
    assert_eq!(fa.len(), fb.len(), "floating windows: {fa:?} vs {fb:?}");
    for ((_, x), (_, y)) in fa.iter().zip(&fb) {
        assert!(
            same_rect(*x, *y),
            "the floating window back at its saved rect: {x:?} vs {y:?}"
        );
    }
}

/// An arrangement unlike the default: a second tab behind Properties, a
/// bottom split for Machines at its own share, and Debug info floating at
/// a rect of its own (#3892: the old Tool library tab is dropped on load).
fn arrange(dock: &mut DockState<Tab>) {
    if let Some(props) = dock.find_tab(&Tab::Properties)
        && let Ok(leaf) = dock.leaf_mut(props.node_path())
    {
        leaf.append_tab(Tab::AiChat);
        leaf.active = props.tab;
    }
    dock.split(
        NodePath::MAIN_ROOT,
        Split::Below,
        0.72,
        Node::leaf(Tab::Machines),
    );
    if dock.find_tab(&Tab::DebugInfo).is_none() {
        let surface = dock.add_window(vec![Tab::DebugInfo]);
        if let Some(w) = dock.get_window_state_mut(surface) {
            w.set_position(egui::pos2(700.0, 120.0));
            w.set_size(egui::vec2(260.0, 220.0));
        }
    }
}

/// The whole done-means of #2339, through the window's own buttons: save
/// the arrangement, reset the dock, load the saved one back.
#[test]
fn save_then_load_restores_the_panel_setup() {
    let path = store("round-trip");
    let mut h = harness_with_store(path.clone());
    h.run();
    arrange(&mut h.state_mut().dock);
    h.run_steps(2);
    let saved = h.state().dock.clone();
    // The window opens from the View door, the way the user opens it.
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Panels")
        .click();
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::MenuItem, "Panel setups...")
        .click();
    h.run_steps(2);
    assert!(
        h.state().state.panel_setups_open,
        "the door opens the window"
    );
    h.state_mut().state.setup_name_draft = String::from("Bench");
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Save current setup")
        .click();
    h.run_steps(2);
    let (stored, note) = prefs::load_setups(&path);
    assert!(note.is_none(), "a saved file reads clean: {note:?}");
    assert_eq!(
        stored.last.as_deref(),
        Some("Bench"),
        "the saved setup is remembered"
    );
    assert!(stored.find("Bench").is_some(), "the setup is in the file");
    // Change the arrangement the way the View door does.
    layout::apply(&mut h.state_mut().dock, crate::state::LayoutRequest::Reset);
    h.run_steps(2);
    assert_ne!(
        leaves(&h.state().dock).len(),
        leaves(&saved).len(),
        "the reset really moved the dock"
    );
    // Load the saved one back from the window's row.
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Load")
        .click();
    h.run_steps(2);
    assert_same(&h.state().dock, &saved);
    assert_eq!(h.state().state.status, "panel setup 'Bench' loaded");
    // View state only: nothing queued, nothing on the history.
    assert!(
        h.state().state.pending.is_empty(),
        "a setup load queued a command"
    );
    assert!(
        h.state().state.history.is_empty(),
        "a setup load landed in the history"
    );
}

/// The same setup comes back at the next start: a fresh State reads the
/// file and its dock starts as the arrangement, with auto-load on.
#[test]
fn the_saved_setup_starts_the_next_run() {
    let path = store("restart");
    let mut h = harness_with_store(path.clone());
    h.run();
    arrange(&mut h.state_mut().dock);
    h.run_steps(2);
    let saved = h.state().dock.clone();
    h.state_mut().state.setup_name_draft = String::from("Bench");
    h.state_mut().state.panel_setup_ask = Some(PanelSetupAsk::Save(String::from("Bench")));
    h.run_steps(2);
    // The restart: a new State, the same file, the shell's start dock.
    let mut restarted = State::new();
    restarted.load_panel_setups_from(path);
    let dock = layout::setups::start_dock(&mut restarted);
    assert_same(&dock, &saved);
    assert!(
        !restarted.status.contains("panel setup"),
        "a good file starts without a diagnostic: {}",
        restarted.status
    );
    // The window's list shows the setup after the restart.
    let mut h2 = harness_with_store(
        restarted
            .panel_setups_path()
            .expect("a store")
            .to_path_buf(),
    );
    h2.state_mut().state.panel_setups_open = true;
    h2.run_steps(2);
    assert!(
        h2.query_by_label_contains("Bench").is_some(),
        "the saved setup is listed"
    );
    assert!(
        h2.query_by_label("Load the last setup when CamCraft starts")
            .is_some()
    );
    assert!(h2.query_by_label("Reset to default").is_some());
}

/// Auto-load off: the next start takes the default layout, the file stays.
#[test]
fn auto_load_off_starts_with_the_default_layout() {
    let path = store("auto-off");
    let mut h = harness_with_store(path.clone());
    h.run();
    arrange(&mut h.state_mut().dock);
    h.run_steps(2);
    h.state_mut().state.panel_setup_ask = Some(PanelSetupAsk::Save(String::from("Bench")));
    h.run_steps(2);
    h.state_mut().state.panel_setups_open = true;
    h.run_steps(2);
    h.get_by_label("Load the last setup when CamCraft starts")
        .click();
    h.run_steps(2);
    let (stored, _) = prefs::load_setups(&path);
    assert!(!stored.auto_load, "the choice is written at once");
    assert!(stored.find("Bench").is_some(), "the setup is still saved");
    let mut restarted = State::new();
    restarted.load_panel_setups_from(path);
    let dock = layout::setups::start_dock(&mut restarted);
    assert_same(&dock, &build_dock());
}

/// A saved arrangement this build cannot read loads the default layout and
/// says why - at the load and at the start alike.
#[test]
fn a_corrupt_setup_loads_the_default_layout_with_a_diagnostic() {
    let path = store("corrupt");
    let broken = PanelSetups {
        auto_load: true,
        last: Some(String::from("Old")),
        setups: vec![SavedSetup {
            name: String::from("Old"),
            dock: serde_json::json!({ "format": "a dock of another build" }),
        }],
    };
    prefs::save_setups(&path, &broken).expect("the broken file lands on disk");
    let mut state = State::new();
    state.load_panel_setups_from(path.clone());
    assert!(
        !state.status.contains("panel setup"),
        "a store that only holds a bad dock loads quiet: {}",
        state.status
    );
    let mut dock = build_dock();
    layout::apply(
        &mut dock,
        crate::state::LayoutRequest::Toggle(crate::state::Panel::Browser),
    );
    layout::setups::apply_ask(
        &mut state,
        &mut dock,
        PanelSetupAsk::Load(String::from("Old")),
    );
    assert_same(&dock, &build_dock());
    assert!(
        state.status.contains("not loaded"),
        "the refusal is named: {}",
        state.status
    );
    // The start takes the same road.
    let mut restarted = State::new();
    restarted.load_panel_setups_from(path);
    let started = layout::setups::start_dock(&mut restarted);
    assert_same(&started, &build_dock());
    assert!(
        restarted.status.contains("not restored"),
        "the start says why: {}",
        restarted.status
    );
}

/// A file of an older or unknown format is an empty store, named on the
/// status line, and the dock starts as the default.
#[test]
fn an_older_setups_file_starts_the_default_layout() {
    let path = store("old-format");
    std::fs::write(
        &path,
        r#"{ "format": "camcraft-panel-setups/0", "setups": [] }"#,
    )
    .expect("an older file lands on disk");
    let mut state = State::new();
    state.load_panel_setups_from(path);
    assert!(
        state.panel_setups.setups.is_empty(),
        "no setup is read from a foreign format"
    );
    assert!(
        state.status.contains("not camcraft-panel-setups/1"),
        "the format is named: {}",
        state.status
    );
    let dock = layout::setups::start_dock(&mut state);
    assert_same(&dock, &build_dock());
}

/// A setup saved in a floating window where the viewport is illegal is
/// refused too: only a dock that obeys the permanence law loads.
#[test]
fn a_viewport_in_a_float_is_refused() {
    let path = store("float-viewport");
    let mut dock = build_dock();
    let viewport = dock
        .find_tab(&Tab::Viewport)
        .expect("the default dock has the viewport");
    dock.detach_tab(
        viewport,
        egui::Rect::from_min_size(egui::pos2(80.0, 80.0), egui::vec2(400.0, 300.0)),
    );
    let saved = PanelSetups {
        auto_load: true,
        last: Some(String::from("Float")),
        setups: vec![SavedSetup {
            name: String::from("Float"),
            dock: layout::setups::encode(&dock).expect("a dock is writable"),
        }],
    };
    prefs::save_setups(&path, &saved).expect("the file lands on disk");
    let mut state = State::new();
    state.load_panel_setups_from(path);
    let started = layout::setups::start_dock(&mut state);
    assert_same(&started, &build_dock());
    assert!(
        state.status.contains("floating window"),
        "the law is named: {}",
        state.status
    );
}

/// #3892: the tool library left the dock for its own window. A setup saved
/// with the old "Tool library" tab (beside Properties and in a float) still
/// loads; the retired tab, and the float that held only it, are dropped.
#[test]
fn a_saved_setup_with_the_old_tool_library_tab_still_loads() {
    let path = store("old-tools-tab");
    let mut dock = build_dock();
    if let Some(props) = dock.find_tab(&Tab::Properties)
        && let Ok(leaf) = dock.leaf_mut(props.node_path())
    {
        leaf.append_tab(Tab::AiChat);
        leaf.append_tab(Tab::Tools);
    }
    dock.add_window(vec![Tab::Tools]);
    let setup = SavedSetup { name: String::from("Old"), dock: layout::setups::encode(&dock).expect("a dock is writable") };
    let saved = PanelSetups { auto_load: true, last: Some(String::from("Old")), setups: vec![setup] };
    prefs::save_setups(&path, &saved).expect("the file lands on disk");
    let mut state = State::new();
    state.load_panel_setups_from(path);
    let started = layout::setups::start_dock(&mut state);
    assert!(started.find_tab(&Tab::Tools).is_none(), "the retired tab is dropped on load");
    assert!(floats(&started).is_empty(), "the float that held only the old tab goes too");
    assert!(started.find_tab(&Tab::AiChat).is_some(), "the saved setup loaded, not the default: {}", state.status);
}
