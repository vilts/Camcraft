//! #1206: each setup's own Machine and Post pages pick that setup's machine
//! and post processor (and set it back to follow the document), on that
//! setup only. Driven through the real pages with clicks.
//! Provenance: our own tests over our own shell; no HSMWorks material.

use super::*;
use crate::state::SetupPage;
use camcraft_app::operation::{Height, OpParams, SrcRef};

/// Two setups, Setup2 active, its page `page` open.
pub(super) fn two_setups(h: &mut Harness<'static, ShellUi>, page: SetupPage) {
    h.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    h.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    h.run_steps(2);
    h.state_mut().state.pending.push_back(CommandKind::ActivateSetup(Some("Setup2".into())));
    h.run_steps(2);
    crate::actions::target(&mut h.state_mut().state, UiTarget::SetupNode("Setup2".into(), page));
    h.run_steps(2);
}

#[test]
fn the_setup_machine_page_takes_its_own_machine() {
    let mut h = shell_harness();
    two_setups(&mut h, SetupPage::Machine);
    h.get_by_label("This setup has its own machine").click();
    h.run_steps(3);
    let st = &h.state().state;
    assert!(camcraft_app::setup_mount::own_machine(&st.world, Some("Setup2")).is_some(), "status: {}", st.status);
    assert!(camcraft_app::setup_mount::own_machine(&st.world, Some("Setup1")).is_none(), "Setup1 is untouched");
}

/// Open the ComboBox showing `now` and pick `to`, with real pointer events.
pub(super) fn pick(h: &mut Harness<'static, ShellUi>, now: &str, to: &str) {
    let at = h
        .get_by(|n| n.role() == egui::accesskit::Role::ComboBox && n.value().as_deref() == Some(now))
        .rect()
        .center();
    super::machines_pane::click(h, at);
    h.run_steps(10);
    let at = h.get_by_label(to).rect().center();
    super::machines_pane::click(h, at);
    h.run_steps(3);
}

#[test]
fn the_setup_post_page_takes_its_own_post_through_the_picker() {
    let mut h = shell_harness();
    two_setups(&mut h, SetupPage::Post);
    h.get_by_label("This setup has its own post").click();
    h.run_steps(3);
    pick(&mut h, "Fanuc", "Haas");
    let st = &h.state().state;
    assert_eq!(camcraft_app::setup_mount::own_post(&st.world, Some("Setup2")), Some(camcraft_app::schema::PostRef::Named("Haas".into())), "status: {}", st.status);
    assert_eq!(camcraft_app::setup_mount::own_post(&st.world, Some("Setup1")), None, "Setup1 is untouched");
    assert_eq!(camcraft_app::posting::document_post_of(&st.world), camcraft_app::schema::PostRef::Builtin, "the document keeps its post");
}

/// The Machine page's Library ComboBox text for the active setup's machine.
pub(super) fn library_label(h: &Harness<'static, ShellUi>) -> String {
    let now = camcraft_app::machine::of(&h.state().state.world).0;
    camcraft_app::posting::MACHINES
        .iter()
        .find(|(n, _)| camcraft_app::posting::builtin_path(n) == now)
        .map_or("other", |(n, _)| *n)
        .to_owned()
}

#[test]
fn the_setup_machine_page_picks_its_own_machine_from_the_library() {
    let mut h = shell_harness();
    two_setups(&mut h, SetupPage::Machine);
    h.get_by_label("This setup has its own machine").click();
    h.run_steps(3);
    let shown = library_label(&h);
    pick(&mut h, &shown, "Lathe");
    let st = &h.state().state;
    assert_eq!(camcraft_app::machine::of(&st.world).0, camcraft_app::posting::builtin_path("Lathe"), "status: {}", st.status);
    assert_eq!(camcraft_app::setup_mount::own_machine(&st.world, Some("Setup1")), None, "Setup1 is untouched");
}

/// The face the app fixtures cut: the catalog end mill over the stock,
/// 0.5 mm off its top and above the seeded 30 mm block (#1657: a face
/// through the block is a true gouge on the stock-source keep law).
fn face() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(45.0),
        cut_z: Height::fixed(31.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// The posted mounts of the document, each file removed after reading.
fn posted_mounts(h: &Harness<'static, ShellUi>, tag: &str) -> Vec<camcraft_app::posting::PostedMount> {
    let path = std::env::temp_dir().join(format!("camcraft-1206-{tag}.nc"));
    let files = camcraft_app::posting::post_mounts(&h.state().state.world, &path).expect("posts one file per mount");
    for f in &files {
        let _ = std::fs::remove_file(&f.path);
    }
    files
}

/// #1206 pin: Setup1 (the document's 3-axis mill and Fanuc post) and
/// Setup2 (its own Tormach 770MX, its own Tormach PathPilot), each with a
/// face, picked on their own pages, post one file per mount through each post.
#[test]
fn each_setup_picks_its_own_machine_and_post_and_posts_one_file_per_mount() {
    const TOOL: &str = "EM-0400-2FL";
    let mut h = shell_harness();
    rack(&mut h.state_mut().state, &[TOOL]);
    // The document mounts a 3-axis mill; the seeded contour's holder would
    // strike the part, so it leaves the program before the faces are cut.
    h.state_mut().state.pending.push_back(CommandKind::LoadBuiltinMachine("VMC 3-axis".into()));
    h.state_mut().state.pending.push_back(CommandKind::DeleteOperation("Contour1".into()));
    h.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    h.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    h.run_steps(2);
    // Each setup gets the same box to cut (a new setup has no stock); Setup2
    // also gets its work offset on the box's top face. Then each its face.
    // The box holds the seeded 60 x 60 x 30 block with 3 mm to spare on
    // every side and 2 mm on top (#1657: the parts must stand inside the
    // stock; a 2 mm side spare puts the 4 mm mill's last pass exactly on
    // the stock edge, which the section law reads as a gouge - #1659).
    let box_stock = || CommandKind::SetStock {
        shape: camcraft_app::stock::StockShape::Box { dx: 66.0, dy: 66.0, dz: 32.0 },
        pos: [30.0, 30.0, 0.0],
        yaw_deg: 0.0,
    };
    h.state_mut().state.pending.push_back(CommandKind::ActivateSetup(Some("Setup1".into())));
    h.state_mut().state.pending.push_back(box_stock());
    h.state_mut().state.pending.push_back(CommandKind::CreateOperation { name: "Face1".into(), tool: TOOL.into(), src: SrcRef::Stock, params: face() });
    h.state_mut().state.pending.push_back(CommandKind::ActivateSetup(Some("Setup2".into())));
    h.state_mut().state.pending.push_back(box_stock());
    h.state_mut().state.pending.push_back(CommandKind::SetWcs { pos: [0.0, 0.0, 32.0], roll_deg: 0.0, pitch_deg: 0.0, yaw_deg: 0.0 });
    h.state_mut().state.pending.push_back(CommandKind::CreateOperation { name: "Face2".into(), tool: TOOL.into(), src: SrcRef::Stock, params: face() });
    h.run_steps(2);
    // Setup2 (active) takes its own machine and post on its own pages.
    crate::actions::target(&mut h.state_mut().state, UiTarget::SetupNode("Setup2".into(), SetupPage::Machine));
    h.run_steps(2);
    h.get_by_label("This setup has its own machine").click();
    h.run_steps(3);
    let shown = library_label(&h);
    pick(&mut h, &shown, "Tilt-head 5-axis");
    crate::actions::target(&mut h.state_mut().state, UiTarget::SetupNode("Setup2".into(), SetupPage::Post));
    h.run_steps(2);
    h.get_by_label("This setup has its own post").click();
    h.run_steps(3);
    pick(&mut h, "Fanuc", "Haas");
    // Generate after the picks: a mount change clears the cached toolpath.
    h.state_mut().state.pending.push_back(CommandKind::GenerateOperation("Face1".into()));
    h.state_mut().state.pending.push_back(CommandKind::GenerateOperation("Face2".into()));
    h.run_steps(2);
    let files = posted_mounts(&h, "pin");
    let st = &h.state().state;
    assert_eq!(files.len(), 2, "status: {}", st.status);
    let setup2 = files.iter().find(|f| f.mount.setups.iter().any(|s| s == "Setup2")).expect("Setup2's file");
    let setup1 = files.iter().find(|f| f.mount.setups.iter().any(|s| s == "Setup1")).expect("Setup1's file");
    assert_eq!(setup2.mount.post, camcraft_app::schema::PostRef::Named("Haas".into()));
    assert_eq!(setup1.mount.post, camcraft_app::schema::PostRef::Builtin, "Setup1 follows the document's post");
    assert_ne!(setup1.mount.machine.0, setup2.mount.machine.0, "different machines");
    assert_ne!(setup1.posted.program, setup2.posted.program, "each file is its own post's program");

    // #1224: the shell's own Post command writes the same two files, each
    // in its own dialect for its own machine, with its own setup's work.
    let path = std::env::temp_dir().join(format!("camcraft-1224-ui-{}.nc", std::process::id()));
    h.state_mut().state.pending.push_back(CommandKind::PostProgramFile(path.clone()));
    h.run_steps(2);
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let read = |k: &str| {
        let p = path.with_file_name(format!("{stem}_{k}.nc"));
        let text = std::fs::read_to_string(&p);
        let _ = std::fs::remove_file(&p);
        text.unwrap_or_else(|e| panic!("{p:?}: {e} (status: {})", h.state().state.status))
    };
    let (a, b) = (read("01_Setup1"), read("02_Setup2"));
    assert_eq!(a, setup1.posted.program, "the shell posts what the app posts");
    assert_eq!(b, setup2.posted.program);
    assert!(a.starts_with("%\nO0001 (VMC 3-AXIS)") && a.contains("Face1 - FACE - G54"), "Fanuc on the VMC:\n{a}");
    assert!(!a.contains("Face2") && !a.contains("TILT-HEAD"), "nothing of Setup2:\n{a}");
    assert!(b.starts_with("%\n(TILT-HEAD 5-AXIS)") && !b.contains("O0001"), "Haas on the tilt head:\n{b}");
    assert!(b.contains("Face2 - FACE") && !b.contains("Face1") && !b.contains("VMC"), "nothing of Setup1:\n{b}");
}
