//! #343 in the browser: each setup is a node with a radio (the active
//! setup) and its own WCS, Stock, Machine, Post, Fixtures (each fixture a
//! child), Tool list and Operations; a document without setups shows the
//! same children for its implicit setup. The radio activates undoably;
//! each node opens its page - activating its setup first, so no editor
//! ever edits a setup the node does not belong to; the machine and post
//! verbs land on the active setup's own mount when it has one.
//! Provenance: our own tests over our own shell; no HSMWorks material.

use super::*;
use crate::state::SetupPage;
use camcraft_app::posting;
use camcraft_app::schema::PostRef;
use super::super::browser_pane;
use super::super::browser_tree::Leaf;

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

/// (label, depth, leaf) for every row.
fn tree(state: &State) -> Vec<(String, usize, Leaf)> {
    let (rows, leaves) = panes::browser_rows(state);
    rows.iter().zip(leaves).map(|(r, l)| (r.label.clone(), r.depth, l)).collect()
}

/// The labels of `setup`'s children, in order, from its node to the next
/// row at its own depth.
fn children(state: &State, setup: &str) -> Vec<String> {
    let rows = tree(state);
    let Some(at) = rows.iter().position(|(_, _, l)| *l == Leaf::Setup(setup.to_owned())) else {
        return Vec::new();
    };
    let depth = rows[at].1;
    rows[at + 1..].iter().take_while(|(_, d, _)| *d > depth).filter(|(_, d, _)| *d == depth + 1).map(|(n, _, _)| n.clone()).collect()
}

const NODES: [&str; 7] = ["WCS", "Stock", "Operations", "Tool list", "Fixtures", "Machine", "Post"];

#[test]
fn every_setup_owns_its_nodes_in_order() {
    let mut state = State::new();
    // No setups: the implicit setup's nodes stand straight under Setup.
    let rows = tree(&state);
    let flat: Vec<&str> = rows
        .iter()
        .filter(|(_, d, _)| *d == 1)
        .map(|(n, _, _)| n.as_str())
        .collect();
    assert_eq!(
        &flat[..10],
        [
            "Setups",
            "WCS",
            "Stock",
            "Operations",
            "Tool list",
            "Fixtures",
            "Machine",
            "Post",
            "(no models yet)",
            "(no tools yet)",
        ]
    );
    run(
        &mut state,
        CommandKind::CreateSetup {
            name: None,
            datum_from: None,
        },
    );
    run(
        &mut state,
        CommandKind::CreateSetup {
            name: None,
            datum_from: None,
        },
    );
    for s in ["Setup1", "Setup2"] {
        assert_eq!(children(&state, s), NODES, "{s}'s nodes");
    }
    let (rows, leaves) = panes::browser_rows(&state);
    let node = |s: &str| {
        leaves
            .iter()
            .position(|l| *l == Leaf::Setup(s.to_owned()))
            .expect("setup node")
    };
    assert_eq!(
        rows[node("Setup2")].kind,
        painted::Kind::Setup(true),
        "the last created is active"
    );
    assert_eq!(rows[node("Setup1")].kind, painted::Kind::Setup(false));
    assert!(
        !rows.iter().any(|r| r.label == "Operations" && r.depth == 0),
        "no document-wide Operations section"
    );
}

#[test]
fn operations_and_fixtures_sit_under_their_own_setup() {
    let mut shell = ShellUi::seeded();
    let state = &mut shell.state;
    run(state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(state, CommandKind::CreateSetup { name: None, datum_from: None });
    let source = model::find_by_name(&state.world, "Contour1").expect("seed op");
    let def = model::operation_of(&state.world, &source).expect("definition");
    run(state, CommandKind::CreateOperation { name: "Contour2".into(), tool: def.tool, src: def.src, params: def.params });
    let rows = tree(state);
    let op = |n: &str| rows.iter().position(|(_, _, l)| *l == Leaf::Op(n.to_owned())).expect("op row");
    let node = |s: &str| rows.iter().position(|(_, _, l)| *l == Leaf::Setup(s.to_owned())).expect("node");
    assert!(node("Setup1") < op("Contour1") && op("Contour1") < node("Setup2"), "Contour1 is Setup1's");
    assert!(op("Contour2") > node("Setup2"), "Contour2 joined the active Setup2");
    assert_eq!(rows[op("Contour1")].1, 3, "setup / Operations / op");
    // Setup1's tool list counts the seed's tool.
    let (r, l) = panes::browser_rows(state);
    let tools = l.iter().position(|x| *x == Leaf::Page("Setup1".into(), SetupPage::Tools)).expect("tool list");
    assert_eq!(r[tools].suffix, "1 tool");
}

#[test]
fn the_radio_activates_a_setup_undoably() {
    let mut state = State::new();
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    assert_eq!(camcraft_app::setup::active(&state.world).as_deref(), Some("Setup2"));
    browser_pane::focus(&mut state, "Setup1");
    state.drain_actions();
    assert_eq!(camcraft_app::setup::active(&state.world).as_deref(), Some("Setup1"));
    browser_pane::focus(&mut state, "Setup1");
    assert!(state.pending.is_empty(), "the active setup's radio queues nothing");
    run(&mut state, CommandKind::Undo);
    assert_eq!(camcraft_app::setup::active(&state.world).as_deref(), Some("Setup2"), "undo restores");
}

#[test]
fn each_node_opens_its_page_and_activates_its_setup_first() {
    let mut state = State::new();
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    for (leaf, want) in [
        (Leaf::Wcs("Setup1".into()), UiTarget::Wcs),
        (Leaf::Stock("Setup1".into()), UiTarget::Stock),
        (Leaf::Page("Setup1".into(), SetupPage::Machine), UiTarget::SetupNode("Setup1".into(), SetupPage::Machine)),
        (Leaf::Page("Setup1".into(), SetupPage::Post), UiTarget::SetupNode("Setup1".into(), SetupPage::Post)),
        (Leaf::Page("Setup1".into(), SetupPage::Fixtures), UiTarget::SetupNode("Setup1".into(), SetupPage::Fixtures)),
        (Leaf::Page("Setup1".into(), SetupPage::Tools), UiTarget::SetupNode("Setup1".into(), SetupPage::Tools)),
    ] {
        run(&mut state, CommandKind::ActivateSetup(Some("Setup2".into())));
        browser_pane::click(&mut state, leaf.clone());
        state.drain_actions();
        assert_eq!(state.pane_target(), want, "{leaf:?}");
        assert_eq!(camcraft_app::setup::active(&state.world).as_deref(), Some("Setup1"), "{leaf:?} activates Setup1");
    }
    // The setup row opens its settings and activates nothing.
    browser_pane::click(&mut state, Leaf::Setup("Setup2".into()));
    state.drain_actions();
    assert_eq!(state.pane_target(), UiTarget::SetupNode("Setup2".into(), SetupPage::Settings));
    assert_eq!(camcraft_app::setup::active(&state.world).as_deref(), Some("Setup1"));
}

#[test]
fn the_machine_and_post_pages_render_for_a_setup() {
    let mut harness = shell_harness();
    harness.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    harness.run_steps(2);
    for (page, heading) in [
        (SetupPage::Machine, "Machine - Setup1"),
        (SetupPage::Post, "Post - Setup1"),
        (SetupPage::Fixtures, "Fixtures - Setup1"),
        (SetupPage::Tools, "Tool list - Setup1"),
        (SetupPage::Settings, "Setup1"),
    ] {
        crate::actions::target(&mut harness.state_mut().state, UiTarget::SetupNode("Setup1".into(), page));
        harness.run_steps(2);
        assert!(harness.query_by_label(heading).is_some(), "{heading}");
    }
    assert!(harness.query_by_label_contains("This setup has its own post").is_none(), "settings page shows no mount radio");
}

#[test]
fn machine_and_post_verbs_land_on_the_active_setups_own_mount() {
    let mut state = State::new();
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    // Setup2 (active) takes its own post: the post verbs now edit only it.
    run(&mut state, CommandKind::SetupOwnPost { setup: "Setup2".into(), own: true });
    assert_eq!(camcraft_app::setup_mount::own_post(&state.world, Some("Setup2")), Some(PostRef::Builtin));
    run(&mut state, CommandKind::SelectPostNamed("Haas".into()));
    assert_eq!(camcraft_app::setup_mount::own_post(&state.world, Some("Setup2")), Some(PostRef::Named("Haas".into())));
    assert_eq!(posting::document_post_of(&state.world), PostRef::Builtin, "the document's post is untouched");
    assert_eq!(camcraft_app::setup_mount::post_of(&state.world, Some("Setup1")), PostRef::Builtin);
    // The machine verbs the same way.
    run(&mut state, CommandKind::SetupOwnMachine { setup: "Setup2".into(), own: true });
    run(&mut state, CommandKind::LoadBuiltinMachine("Lathe".into()));
    assert_eq!(camcraft_app::machine::kind_of(&state.world), camcraft_app::machine::Kind::Lathe);
    assert!(camcraft_app::machine::document_of(&state.world).1.is_none(), "the document mounts nothing still");
    run(&mut state, CommandKind::ActivateSetup(Some("Setup1".into())));
    assert_eq!(camcraft_app::machine::kind_of(&state.world), camcraft_app::machine::Kind::Mill, "Setup1 follows the document");
    // Undo walks every step back: Setup2 follows the document again.
    for _ in 0..5 {
        run(&mut state, CommandKind::Undo);
    }
    assert_eq!(camcraft_app::setup_mount::own_post(&state.world, Some("Setup2")), None);
    assert_eq!(camcraft_app::setup_mount::own_machine(&state.world, Some("Setup2")), None);
}

/// #745: the Machine page shows the overview as a label | value grid,
/// one row per fact, no `?` for an unstated minimum speed.
#[test]
fn the_machine_page_shows_a_property_grid() {
    let mut harness = shell_harness();
    harness.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    harness.state_mut().state.pending.push_back(CommandKind::LoadBuiltinMachine("VMC 3-axis".into()));
    harness.run_steps(2);
    crate::actions::target(&mut harness.state_mut().state, UiTarget::SetupNode("Setup1".into(), SetupPage::Machine));
    harness.run_steps(3);
    for label in ["Kind", "Machine", "Spindle", "Axes", "Tool changer", "Coolant", "Workspace", "Source"] {
        assert!(harness.query_all_by_label(label).next().is_some(), "row label {label}");
    }
    assert!(harness.query_by_label("Built-in: VMC 3-axis").is_some());
    assert!(harness.query_by_label_contains("?").is_none(), "no bare question mark");
}
