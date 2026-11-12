//! M-a: frame tests for the graduated shell - egui_kittest harnesses driving
//! the SAME `shell_layout` the window runs, not a test-local reconstitution.
//! (This is why they live in the lib, cfg(test): the point is to run the
//! shipping code; the spike's bin-local tests retire with the bin.)
//!
//! Ported from the spike's tests: `egui_frames_run` (8 frames, seeded
//! document survives the round trip), with M-b's browser pane,
//! `click_toggles_view_state` (a real pointer event on a real checkbox),
//! and - with M-c's property pages - `click_apply_edits_operation_through_
//! commands` (the draft/Apply law) plus the DragValue parity test: the drag
//! path and the typed path commit the same staged field the imgui
//! InputDouble did (divergence D5's proof). `history_key_queues_command`
//! proves the keyboard layer. M-d adds the file law:
//! `blank_startup_renders_and_frames_honestly` (the window opens empty -
//! the seed is gone from `Shell::new`) and `file_ops_round_trip_through_
//! commands` (Save/New/Open through the queue; the rfd modals are
//! hand-test-only). Since M-e the viewport itself is interactive: the
//! gesture tests drive `crate::interact::pick_at` and the camera arms
//! through egui's own pointer events - click-and-pick, the travel
//! threshold, MMB/RMB, wheel, the Face radio and anchor picking. The
//! snapshot was the churn gate for the per-panel migration steps; since
//! #255 the pixel goldens ride the release lane (`#[ignore]`, run and
//! regenerated only at a cut) and `tests/structural.rs` asserts their
//! intent as text in the default suite. M1.4's
//! graphical tool editor adds the first floating surface: the flow test
//! drives the window (Edit opens, drag stages, Apply lands, Delete
//! ghost-closes) and `tool_editor_snapshot` pins its pixels beside the
//! shell's, which the closed-by-default window must leave untouched.
//!
//! The harness runs the managed-texture path: the viewport leaf shows a flat
//! placeholder instead of View3D's offscreen render, because the GPU
//! registration lives in Shell::draw/drain_textures and is proven by the
//! headless run - repeating it here would add wgpu nondeterminism to pixels
//! that carry no layout information. Everything else is shipping code: theme,
//! dock splits, panes, the command queue.
//!
//! Provenance: our own tests, specced from the egui_kittest and kittest
//! vendored sources; no HSMWorks material involved.

use egui::{ColorImage, Event, Key, Modifiers, TextureOptions, Vec2};
use egui_dock::DockState;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, Height, OpParams, SrcRef};
use camcraft_app::commands::theme_set;
use camcraft_app::schema::{ThemeRef, WCS_SLOTS};
use camcraft_app::units::{self, Unit};

use crate::interact;
use crate::state::{CommandKind, DragMode, GeoPick, PostView, State, TreeMenuTarget, UiTarget};

use super::contrast;
use super::icons::{Icon, IconSet};
use super::panes::{self, Cmd};
use super::painted;
use super::shell::{Tab, build_dock, headless, shell_layout};
use super::viewcube;
use super::{theme, world};

#[test]
fn empty_egui_meshes_are_filtered_before_gpu_submission() {
    let empty = egui::ClippedPrimitive {
        clip_rect: egui::Rect::EVERYTHING,
        primitive: egui::epaint::Primitive::Mesh(egui::Mesh::default()),
    };
    assert!(!super::shell::renderable_paint_job(&empty));
    let mut mesh = egui::Mesh::default();
    mesh.add_colored_rect(
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(10.0, 10.0)),
        egui::Color32::WHITE,
    );
    let nonempty = egui::ClippedPrimitive {
        clip_rect: egui::Rect::EVERYTHING,
        primitive: egui::epaint::Primitive::Mesh(mesh),
    };
    assert!(super::shell::renderable_paint_job(&nonempty));
}

mod face_origin;
mod spindle_row;
mod stock_size;
mod stock_fit_sides;
mod raw_stock_row;
mod layout;
mod machines_pane;
mod machines_edit;
mod unsaved;
mod post_refusal;
mod gcode_viewer;
mod first_setup;
mod structural;
// #661: every operation kind wears its own SVG-mastered glyph.
mod op_icon_table;
// #330: a clicked tilted face is turned flat onto the plane.
mod plane_face_lay;
// #554: the plane row's click takes the face, an axis row's the edge.
mod plane_face_pick;
// #692: one pick rule on the op page, the setup page and a smooth wall.
mod one_pick_rule;
// #692: the user-facing pick-through toggle (off by default) reaches the far side.
mod pick_through;
// #664: a left Setups page disarms its picks.
mod setup_picks_disarm;
// #555: real clicks pick the three points of the plane.
mod points_vertex_pick;
mod one_wcs;
// #343: the per-setup browser tree.
mod setup_tree;
mod mcp_live;
// #389: per-setup post options and the tool-list export.
mod setup_post_options;
// #155: Pause / Home entries in the operation list.
mod program_entry;
// #371: the tool editor's Body section.
mod tool_body;
// #93: the tool editor's spun-profile (Profile) section.
mod tool_profile;
// #793: the tool editor's holder library picker.
mod tool_holder;
// #889: the Tool library page's "Import tool holders" toggle.
mod tool_library_import;
// #93 slice 2: the Profile tab's drawn cutter.
mod tool_cutter;
// #93 slice 3: the drawn cutter edited by its parameters.
mod tool_cutter_params;
// #441: the Theme ribbon door; the Stock page shows stock only.
mod theme_door;
mod recent_door;
// #645: the browser's "Show origin names" switch and the per-user view file.
mod view_names;
mod prefs_door;
// #509: the Fixtures page rows fit the pane.
mod fixture_rows;
// #864: a reduced-fidelity model says so on its page.
mod model_fidelity;
// #1054: the STEP facet budget on the model page.
mod step_budget;
// #709 hand check: both points of a two-point axis stay marked.
mod two_point_marks;
// #1104: the Setups page lists setups only.
mod setups_declutter;

/// What the per-frame state of `Shell` is, minus the GPU half: the fields
/// `shell_layout` touches, so the harness exercises the window's real body.
struct ShellUi {
    state: State,
    dock: DockState<Tab>,
    viewport_points: [f32; 2],
    left_press: Option<[f32; 2]>,
    viewport_tex: Option<egui::TextureHandle>,
    /// The icon set, built lazily like `Shell`'s - the registration is
    /// CPU-side (`ctx.load_texture`), so the harness proves the PNG path,
    /// not just the painted fallback.
    icons: Option<IconSet>,
    themed: bool,
}

impl ShellUi {
    /// The birth `Shell::new` has since M-d: a plain empty document, no
    /// seed, no startup command - the window's real shape, rendered.
    fn blank() -> Self {
        Self {
            state: State::new(),
            dock: build_dock(),
            viewport_points: [1280.0, 649.0],
            left_press: None,
            viewport_tex: None,
            icons: None,
            themed: false,
        }
    }

    /// `blank` plus the demo document through the real command machinery
    /// (world.rs), then FrameAll off the queue - the harness default,
    /// because the property pages need content to hold up.
    fn seeded() -> Self {
        let mut sh = Self::blank();
        world::seed(&mut sh.state);
        sh.state.pending.push_back(CommandKind::FrameAll);
        sh.state.drain_actions();
        sh
    }
}

/// One frame under Shell::draw's law: first the commands the panels queued
/// last frame, then the whole shell. The theme and the viewport texture are
/// applied lazily because the harness owns its `Context` and hands it over
/// only through the `Ui` - same init order the window follows in resumed().
fn frame(ui: &mut egui::Ui, sh: &mut ShellUi) {
    let ctx = ui.ctx().clone();
    if !sh.themed {
        theme::apply(&ctx);
        sh.themed = true;
    }
    sh.state.drain_actions();
    let img = ColorImage::new([8, 8], vec![egui::Color32::from_rgb(24, 26, 31); 64]);
    let tex = sh
        .viewport_tex
        .get_or_insert_with(|| ctx.load_texture("egui-viewport", img, TextureOptions::NEAREST))
        .id();
    if sh.icons.is_none() {
        sh.icons = Some(IconSet::load(&ctx));
    }
    shell_layout(
        ui,
        &mut sh.state,
        &mut sh.dock,
        tex,
        sh.icons.as_ref(),
        &mut sh.viewport_points,
        &mut sh.left_press,
    );
}

/// `frame` WITHOUT the drain: the real shell layout, but queued dialog
/// arms never execute (rfd modals are hand-test-only - same reason
/// `shortcuts_only` exists, for tests that need the whole ribbon on
/// screen). Callers assert queue membership and clear after.
fn frame_undrained(ui: &mut egui::Ui, sh: &mut ShellUi) {
    let ctx = ui.ctx().clone();
    if !sh.themed {
        theme::apply(&ctx);
        sh.themed = true;
    }
    let img = ColorImage::new([8, 8], vec![egui::Color32::from_rgb(24, 26, 31); 64]);
    let tex = sh
        .viewport_tex
        .get_or_insert_with(|| ctx.load_texture("egui-viewport", img, TextureOptions::NEAREST))
        .id();
    if sh.icons.is_none() {
        sh.icons = Some(IconSet::load(&ctx));
    }
    shell_layout(
        ui,
        &mut sh.state,
        &mut sh.dock,
        tex,
        sh.icons.as_ref(),
        &mut sh.viewport_points,
        &mut sh.left_press,
    );
}

/// The window at hand-test size, first frame settled (the builder's own
/// run_ok stabilises fonts and the theme).
fn shell_harness() -> Harness<'static, ShellUi> {
    Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::seeded())
}

/// Rack catalog tools into the document through the command queue (the
/// M10.1 tool law: a kind is only created against a tool that can cut
/// it - the seeded demo carries one flat end mill, so a test reaching
/// any other family puts its fitting tool on the rack first).
fn rack(state: &mut State, names: &[&str]) {
    for name in names {
        let tool = camcraft_app::tools::catalog()
            .into_iter()
            .find(|(n, _)| n == name)
            .map(|(_, t)| t)
            .expect("catalog entry");
        state.pending.push_back(CommandKind::AddTool { name: (*name).into(), tool });
    }
    state.drain_actions();
}

/// Every lathe kind's family, from the catalog: insert, blade, thread
/// insert, form gauge, boring bar.
const LATHE_RACK: &[&str] = &["TN-1600-0R8", "GT-0200", "TD-0866-60", "FG-0600-04", "BR-1000"];

/// A frame body with ONLY the keyboard layer: no shell, and crucially no
/// `drain_actions` - `State::run`'s dialog arms open real rfd modals, so
/// the file-key proof here is queue membership, never an executed command.
fn shortcuts_only(ui: &mut egui::Ui, st: &mut State) {
    panes::shortcuts(ui, st);
}

/// ...same, with a text field owning focus (the live shell always has one
/// reachable - imgui's io.WantTextInput gate mirrored by `typing`).
fn shortcuts_with_field(ui: &mut egui::Ui, st: &mut State) {
    let mut scratch = String::new();
    ui.add(egui::TextEdit::singleline(&mut scratch).desired_width(80.0))
        .request_focus();
    panes::shortcuts(ui, st);
}

/// Port of the spike's `egui_frames_run`: real frames of the real shell.
/// A panic anywhere in the layout, the theme or the command drain fails the
/// test; the world check afterwards proves eight passes of the shell -
/// which rebuild their widget state every frame - left the document intact.
#[test]
fn egui_frames_run() {
    let mut harness = shell_harness();
    harness.run_steps(8);

    let st = &harness.state().state;
    let moves = model::find_by_name(&st.world, "Contour1")
        .and_then(|e| model::toolpath_of(&st.world, &e))
        .map_or(0, |tp| tp.path.moves.len());
    assert!(
        moves > 0,
        "Contour1's cached toolpath vanished across frames (status: {})",
        st.status
    );
}

/// The keyboard layer M-a does ship: Ctrl+Z queues Undo through
/// `shortcuts`, exactly as a menu click would - the command reaches
/// `state.pending`, and the NEXT frame's drain executes it (the shell's
/// queue law, not a direct history poke).
#[test]
fn history_key_queues_command() {
    let mut harness = shell_harness();
    harness.run();
    // Freshly seeded: the redo lane is empty (executing clears it).
    assert!(
        !harness.state().state.history.can_redo(),
        "a plain run must not move the history"
    );
    // ModifiersChanged first: InputState tracks i.modifiers from that event
    // (the live window gets it from winit's ModifiersChanged), so the
    // injection must send the same pair or the Cmd check reads no modifier.
    harness
        .input_mut()
        .events
        .push(Event::ModifiersChanged(Modifiers::COMMAND));
    harness.input_mut().events.push(Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    // Whether the pass that drains the queue lands in this run or the next
    // is egui's multi-pass business; the observable law is that Ctrl+Z
    // walked the history through the command machinery.
    harness.run_steps(2);
    assert!(
        harness.state().state.history.can_redo(),
        "Ctrl+Z did not reach the undo history"
    );
    assert!(
        !harness
            .state()
            .state
            .pending
            .iter()
            .any(|c| matches!(c, CommandKind::Undo)),
        "the queued command outlived the drain - queue and shell disagree"
    );
}

/// Port of the spike's `click_toggles_view_state` (M-b's browser pane):
/// the global view toggles are immediate UI state (AGENTS.md: not
/// undoable), reached here through a real pointer event, not a poke.
#[test]
fn click_toggles_view_state() {
    let mut harness = shell_harness();
    assert!(!harness.state().state.show_all_paths);
    harness.get_by_label("Show all toolpaths").click();
    harness.run();
    assert!(harness.state().state.show_all_paths);
    // The display mode follows the same law through the same door: a real
    // pointer event, view state (never a command, never the undo stack).
    assert_eq!(harness.state().state.display, crate::state::DisplayMode::ShadedEdges);
    harness.get_by_label("Wireframe").click();
    harness.run();
    assert_eq!(harness.state().state.display, crate::state::DisplayMode::Wireframe);
    assert!(harness.state().state.pending.is_empty(), "a view toggle queues nothing");
}

/// #434: the Projection radios flip the viewport's projection mode -
/// view state on the same law as the Solids radios (never a command,
/// never the undo stack), reached through a real pointer event.
#[test]
fn projection_radios_flip_the_viewport_mode() {
    let mut harness = shell_harness();
    assert_eq!(
        harness.state().state.cam.projection,
        crate::viewport::ProjectionMode::Perspective,
        "the house default stays perspective"
    );
    harness.get_by_label("Ortho").click();
    harness.run();
    assert_eq!(harness.state().state.cam.projection, crate::viewport::ProjectionMode::Orthographic);
    harness.get_by_label("Perspective").click();
    harness.run();
    assert_eq!(harness.state().state.cam.projection, crate::viewport::ProjectionMode::Perspective);
    assert!(harness.state().state.pending.is_empty(), "a mode switch queues nothing");
}

#[test]
fn browser_show_and_hide_all_toolpaths_update_operation_eyes() {
    let mut harness = shell_harness();
    let operation_names: Vec<String> = model::named_entities(&harness.state().state.world)
        .into_iter()
        .filter(|(_, entity)| model::is_operation(&harness.state().state.world, entity))
        .map(|(name, _)| name)
        .collect();
    assert!(!operation_names.is_empty());

    harness.get_by_label("Show all toolpaths").click();
    harness.run();
    let state = &harness.state().state;
    assert!(state.show_all_paths);
    assert!(state.hidden_paths.is_empty());

    harness.get_by_label("Show all toolpaths").click();
    harness.run();
    let state = &harness.state().state;
    assert!(!state.show_all_paths);
    assert!(operation_names.iter().all(|name| state.hidden_paths.contains(name)));
}

/// M-d's startup law, proven in pixels: the window opens on a plain
/// empty document - the seed left `Shell::new` for real file ops - and
/// the whole shell renders it without a hiccup. Then a bare FrameAll on
/// the blank world must refuse by name ("nothing to frame", the camera
/// untouched) instead of fitting a bogus box: Prime Directive on the one
/// command the empty startup actually receives.
#[test]
fn blank_startup_renders_and_frames_honestly() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::blank());
    harness.run_steps(3);
    let st = &harness.state().state;
    assert_eq!(st.status, "ready", "startup must not run any command");
    assert!(
        model::named_entities(&st.world).is_empty(),
        "the window must start on an empty document"
    );
    harness
        .state_mut().state.pending
        .push_back(CommandKind::FrameAll);
    harness.run();
    assert_eq!(harness.state().state.status, "nothing to frame");
}

/// M-d's file law through the machinery the ribbon drives: the queue's
/// dialog-free path (the `*File` arms - the same `project::save` /
/// `project::load` the rfd dialogs invoke once a path is picked; the
/// modal itself is hand-test-only). Save, New, Open: New must retire
/// the undo timeline (state.rs), and the reopened document must carry
/// the saved operation's parameters back verbatim.
#[test]
fn file_ops_round_trip_through_commands() {
    let mut harness = shell_harness();
    let path = std::env::temp_dir().join(format!("camcraft-egui-md-{}.camproj", std::process::id()));
    let saved_title = format!("CamCraft - {}", path.file_name().unwrap().to_string_lossy());
    assert_eq!(harness.state().state.project_path, None);
    // The seed is unsaved work: the title carries the '*' (#107).
    assert_eq!(harness.state().state.window_title(), "*CamCraft - Untitled");

    harness
        .state_mut().state.pending
        .push_back(CommandKind::SaveProjectFile(path.clone()));
    harness.run();
    assert!(
        harness.state().state.status.starts_with("saved"),
        "save did not report success (status: {})",
        harness.state().state.status
    );
    assert_eq!(harness.state().state.project_path.as_ref(), Some(&path));
    assert_eq!(harness.state().state.window_title(), saved_title);

    // Save uses the captured path directly; this must not open a native dialog.
    harness.state_mut().state.pending.push_back(CommandKind::SaveProjectDialog);
    harness.run();
    assert_eq!(harness.state().state.status, format!("saved {}", path.display()));
    assert_eq!(harness.state().state.project_path.as_ref(), Some(&path));

    // A failed save must leave the last successful filename in place.
    let bad_path = path.join("failed.camproj"); // `path` is a file, not a directory.
    harness
        .state_mut().state.pending
        .push_back(CommandKind::SaveProjectFile(bad_path));
    harness.run();
    assert!(harness.state().state.status.starts_with("save failed:"));
    assert_eq!(harness.state().state.project_path.as_ref(), Some(&path));
    assert_eq!(harness.state().state.window_title(), saved_title);

    harness.state_mut().state.pending.push_back(CommandKind::NewDocument);
    harness.run();
    let st = &harness.state().state;
    assert!(
        model::find_by_name(&st.world, "Contour1").is_none(),
        "New left entities behind (status: {})",
        st.status
    );
    assert!(!st.history.can_undo(), "New must retire the timeline");
    assert_eq!(st.status, "new document");
    assert_eq!(st.project_path, None);
    assert_eq!(st.window_title(), "CamCraft - Untitled");

    harness
        .state_mut().state.pending
        .push_back(CommandKind::OpenProjectFile(path.clone()));
    harness.run();
    let st = &harness.state().state;
    let climb = model::find_by_name(&st.world, "Contour1")
        .and_then(|e| model::operation_of(&st.world, &e))
        .map(|op| match op.params {
            OpParams::Contour { climb, .. } => climb,
            _ => false,
        });
    assert_eq!(
        climb,
        Some(true),
        "the reopened document lost Contour1's saved params (status: {})",
        st.status
    );
    assert!(st.status.starts_with("opened"), "open status: {}", st.status);
    assert_eq!(st.project_path.as_ref(), Some(&path));
    assert_eq!(st.window_title(), saved_title);
    let _ = std::fs::remove_file(&path);
}

/// M1.7b's two faces: the ribbon cell (and Ctrl+P) queue the dialog arm -
/// membership only, the modal is hand-test - while the `*File` arm runs
/// the validation gate for real: a generated, fresh, silent document
/// posts one wrapped program, a stale one is refused BY NAME and the
/// previous program on disk is not touched (the gate writes nothing).
#[test]
fn post_cell_queues_and_the_file_arm_runs_the_gate() {
    let (cells, _, _, _) = panes::ribbon_cells(
        egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280.0, super::shell::RIBBON_H)),
        &theme::current(),
        &panes::ribbon_plan(camcraft_app::machine::Kind::Mill, &[]),
    );
    let post = cells
        .iter()
        .find(|c| c.cmd == Cmd::Post)
        .expect("the File group carries a Post cell");
    {
        let mut harness = Harness::builder()
            .with_size(Vec2::new(1280.0, 720.0))
            .build_ui_state(frame_undrained, ShellUi::seeded());
        harness.run();
        let at = post.rect.center();
        set_pos(&mut harness, at);
        button_at(&mut harness, egui::PointerButton::Primary, at, true);
        button_at(&mut harness, egui::PointerButton::Primary, at, false);
        harness.run_steps(2);
        assert!(
            harness.state().state.pending.iter().any(|c| matches!(
                c,
                CommandKind::PostProgramDialog
            )),
            "the Post cell must fire through the actions law (pending: {:?})",
            harness.state().state.pending
        );
    }

    let mut harness = shell_harness();
    let path = std::env::temp_dir().join(format!("camcraft-egui-post-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);

    // Phase 0 - the warned face of the gate, on the seed as shipped: its
    // contour takes the full block height, and plunging that rim beside
    // 30 mm of uncut wall is a ShankHit family the verifier already
    // cached. Fresh and generated is NOT enough - the post blocks by
    // name and writes nothing.
    harness
        .state_mut().state.pending
        .push_back(CommandKind::PostProgramFile(path.clone()));
    harness.run();
    {
        let st = &harness.state().state;
        assert!(
            st.status.contains("post refused") && st.status.contains("post blocked")
                && st.status.contains("Contour1") && st.status.contains("shank"),
            "a warned path must block the gate (status: {})",
            st.status
        );
        assert!(!path.exists(), "a blocked post is never written");
    }

    // Phase 1 - the same contour re-cut as a 0.5 mm top skim is legal
    // geometry and a silent path: edit, regenerate, post, all through
    // the queue.
    {
        let mut skim = {
            let st = &harness.state().state;
            let e = model::find_by_name(&st.world, "Contour1").expect("the seed's op");
            model::operation_of(&st.world, &e).expect("op").params.clone()
        };
        if let OpParams::Contour { bottom_z, .. } = &mut skim {
            *bottom_z = Height::fixed(29.5);
        }
        harness.state_mut().state.pending.push_back(CommandKind::EditOperationParams {
            name: "Contour1".into(),
            params: skim,
        });
        harness
            .state_mut().state.pending
            .push_back(CommandKind::GenerateOperation("Contour1".into()));
    }
    harness
        .state_mut().state.pending
        .push_back(CommandKind::PostProgramFile(path.clone()));
    harness.run();
    let st = &harness.state().state;
    assert!(st.status.starts_with("posted"), "skim post: {}", st.status);
    let text = std::fs::read_to_string(&path).expect("a posted program");
    assert!(
        text.starts_with('%')
            && text.contains("G90 G54")
            && text.contains("T01 M6")
            && text.trim_end().ends_with('%'),
        "one wrapped Fanuc program for the skim: {text}"
    );
    let before = text.clone();

    // The gate's refusal face, through the queue: an un-regenerated
    // parameter edit makes Contour1 stale, and a stale document must
    // not reach the machine.
    let mut edited_params = {
        let st = &harness.state().state;
        let e = model::find_by_name(&st.world, "Contour1").expect("the seed's op");
        model::operation_of(&st.world, &e).expect("op").params.clone()
    };
    // (A stepdown edit would be stale but silent here - the skim's
    // 0.5 mm span fits under any stepdown - so the witness is the
    // plunge feed, which rides every Z word's F.)
    if let OpParams::Contour { plunge_feed, .. } = &mut edited_params {
        *plunge_feed = 150.0;
    }
    harness.state_mut().state.pending.push_back(CommandKind::EditOperationParams {
        name: "Contour1".into(),
        params: edited_params,
    });
    harness
        .state_mut().state.pending
        .push_back(CommandKind::PostProgramFile(path.clone()));
    harness.run();
    let st = &harness.state().state;
    assert!(
        st.status.contains("post refused")
            && st.status.contains("Contour1")
            && st.status.contains("regenerate"),
        "a stale post must refuse by name (status: {})",
        st.status
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("still there"),
        before,
        "a refusal rewrites nothing"
    );

    // Regenerate through the same queue: fresh again, posts again.
    harness
        .state_mut().state.pending
        .push_back(CommandKind::GenerateOperation("Contour1".into()));
    harness
        .state_mut().state.pending
        .push_back(CommandKind::PostProgramFile(path.clone()));
    harness.run();
    assert!(
        harness.state().state.status.starts_with("posted"),
        "regen status: {}",
        harness.state().state.status
    );
    assert_ne!(
        std::fs::read_to_string(&path).expect("rewritten"),
        before,
        "the re-posted program carries the new stepdown's passes"
    );
    let _ = std::fs::remove_file(&path);
}

/// Port of the spike's `click_apply_edits_operation_through_commands`
/// (M-c: the properties pane is the real one now). The draft law end to
/// end: the checkbox edits the staged `op_ui` only, Apply queues
/// EditOperationParams, and the NEXT drain writes the world - the history
/// afterwards is the proof, not any intermediate queue state (egui may run
/// the state closure across several passes per `run`).
#[test]
fn click_apply_edits_operation_through_commands() {
    let mut harness = shell_harness();
    let staged_climb = |h: &Harness<'static, ShellUi>| match h.state().state.op_ui.params {
        OpParams::Contour { climb, .. } => climb,
        _ => false,
    };
    assert!(staged_climb(&harness));
    // M1.4 sectioning (restored after M9.1 B): the checkbox lives on the
    // Feeds tab, and a tab cell is the way in - navigation first, then the
    // click law itself.
    harness.get_by_label("Feeds").click();
    harness.run();
    harness.get_by_label("Climb milling").scroll_to_me();
    harness.run();
    harness.get_by_label("Climb milling").click();
    harness.run();
    assert!(!staged_climb(&harness), "checkbox did not reach the staged params");
    harness.get_by_label("Apply").scroll_to_me();
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    let st = &harness.state().state;
    let live = model::find_by_name(&st.world, "Contour1")
        .and_then(|e| model::operation_of(&st.world, &e))
        .map(|op| match op.params {
            OpParams::Contour { climb, .. } => climb,
            _ => true,
        });
    assert_eq!(live, Some(false), "Apply did not reach the world");
    assert!(
        !st.pending
            .iter()
            .any(|c| matches!(c, CommandKind::EditOperationParams { .. })),
        "the queued command outlived the drain - queue and panels disagree"
    );
    assert!(
        !st.history.can_redo() && st.history.can_undo(),
        "Apply must sit on the undo timeline"
    );
}

/// M3.1 C: the batch fix lives in the status strip, and appears exactly
/// when it means something - the moment a stale row exists to fix. The
/// click is one queue, one undo unit, and the button retires with the
/// staleness.
#[test]
fn status_bar_offers_regenerate_all_when_something_is_stale() {
    let mut harness = shell_harness();
    let edited = {
        let st = &harness.state().state;
        let e = model::find_by_name(&st.world, "Contour1").expect("the seed's op");
        let mut p = model::operation_of(&st.world, &e).expect("op").params;
        if let OpParams::Contour { plunge_feed, .. } = &mut p {
            *plunge_feed = 150.0;
        }
        p
    };
    harness
        .state_mut().state.pending
        .push_back(CommandKind::EditOperationParams { name: "Contour1".into(), params: edited });
    harness.run();
    {
        let st = &harness.state().state;
        let e = model::find_by_name(&st.world, "Contour1").expect("op");
        assert_eq!(camcraft_app::operation::status_of(&st.world, &e), camcraft_app::operation::OpStatus::Stale);
    }

    harness.get_by_label("Regenerate all stale (1)").click();
    harness.run();
    assert_eq!(harness.state().state.status, "regenerated 1 operation(s)");
    let st = &harness.state().state;
    let e = model::find_by_name(&st.world, "Contour1").expect("op");
    assert_eq!(camcraft_app::operation::status_of(&st.world, &e), camcraft_app::operation::OpStatus::Ok);
    // The affordance follows the state: no stale rows, no button.
    assert!(
        harness.query_by_label("Regenerate all stale (1)").is_none(),
        "the button outlived its staleness"
    );
}

/// The Contour pane's "Stock to leave" DragValue (on the Passes tab, which
/// the caller opens). The field-label row pins it - robust to whatever
/// other numbers the tab grows: the number is the DragValue that
/// sits directly below its `field()` label (tree nodes are submission order
/// and the page flows top-down; the role query is DragValues only - the
/// painted strips carry no a11y marks).
fn stock_to_leave_rect(h: &Harness<'static, ShellUi>) -> egui::Rect {
    let lab = h.get_by_label("Stock to leave").rect();
    h.get_all_by_role(egui::accesskit::Role::SpinButton)
        .map(|n| n.rect())
        .filter(|r| (r.center().y - lab.center().y).abs() < 12.0)
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .expect("the Stock to leave field draws its DragValue on its label's row")
}

fn stock_to_leave_center(h: &Harness<'static, ShellUi>) -> egui::Pos2 {
    stock_to_leave_rect(h).center()
}

/// DragValue parity (divergence D5's proof, M-c): the drag path and the
/// typed path commit the SAME staged field the imgui InputDouble did.
/// Neither may touch the world by itself - numbers stage into `op_ui`
/// and ride an EditOperationParams only when Apply is clicked.
#[test]
fn drag_and_type_edit_the_same_staged_number() {
    let mut harness = shell_harness();
    // "Stock to leave" is on the Passes tab - navigate there first; the
    // helper still pins the DragValue by its field label's row, and the
    // scroll brings it on screen before measuring.
    harness.get_by_label("Passes").click();
    harness.run();
    harness.get_by_label("Stock to leave").scroll_to_me();
    harness.run();
    let leave = |h: &Harness<'static, ShellUi>| match h.state().state.op_ui.params {
        OpParams::Contour {
            stock_to_leave,
            climb,
            ..
        } => {
            assert!(climb, "the drag/type paths must not disturb other staged fields");
            stock_to_leave
        }
        _ => panic!("the demo document must stage Contour params"),
    };
    let first = leave(&harness);
    let live0 = model::find_by_name(&harness.state().state.world, "Contour1")
        .and_then(|e| model::operation_of(&harness.state().state.world, &e))
        .map(|op| match op.params {
            OpParams::Contour { stock_to_leave, .. } => stock_to_leave,
            _ => f64::NAN,
        })
        .expect("Contour1 exists with contour params");

    // The drag: press, one moved frame with the button down, release.
    // A same-frame click+move would become focus instead (clicked()
    // requests it before the drag branch), so the gesture is spread
    // across frames - the imgui InputDouble stepped on arrows and
    // wheel; the DragValue's counterpart is the horizontal drag.
    let c = stock_to_leave_center(&harness);
    harness.drag_at(c);
    harness.run();
    harness.hover_at(c + Vec2::new(30.0, 0.0));
    harness.run();
    harness.drop_at(c + Vec2::new(30.0, 0.0));
    harness.run();
    assert!(
        (leave(&harness) - (first + 3.0)).abs() < 0.05,
        "drag path: 30 px at speed 0.1 did not step {first} to {} (got {})",
        first + 3.0,
        leave(&harness)
    );

    // The typed entry: click focuses (the DragValue re-renders itself as
    // a TextEdit with select-all), typing replaces, and
    // update_while_editing commits the parse live. The click pins the row
    // by rect; while editing the widget may widen, so the row's top edge
    // is the stable locator for the keystroke's node (the text event goes
    // to the focused widget - any node on that row carries it).
    let want = stock_to_leave_rect(&harness);
    harness
        .get_all_by_role(egui::accesskit::Role::SpinButton)
        .find(|n| n.rect() == want)
        .unwrap()
        .click();
    harness.run();
    harness
        .get_all_by_role(egui::accesskit::Role::SpinButton)
        .find(|n| (n.rect().top() - want.top()).abs() < 1.0)
        .unwrap()
        .type_text("25");
    harness.run();
    assert!(
        (leave(&harness) - 25.0).abs() < 1e-9,
        "typed path: 25 did not commit (got {})",
        leave(&harness)
    );

    // And the world is untouched until Apply: no command was queued,
    // the live op still carries the seed value.
    let st = &harness.state().state;
    assert!(
        !st.pending
            .iter()
            .any(|c| matches!(c, CommandKind::EditOperationParams { .. })),
        "a staged number edit must not queue a command by itself"
    );
    let live = model::find_by_name(&st.world, "Contour1")
        .and_then(|e| model::operation_of(&st.world, &e))
        .map(|op| match op.params {
            OpParams::Contour { stock_to_leave, .. } => stock_to_leave,
            _ => f64::NAN,
        })
        .expect("Contour1 exists with contour params");
    assert!(
        live == live0 && live != 25.0,
        "staged edits leaked into the world: live {live0} became {live}"
    );
}

/// M1.4's sectioning law, as a drift guard in the tips-match style (the
/// tab bar restored after M9.1 B): every named field of every tabled kind
/// renders on exactly ONE tab - exactly once while its tab is open, absent
/// from the tree otherwise - and a kind's tab bar shows exactly the
/// sections it has fields for (no dead tabs). If a field moves sections, a
/// label changes, or a guard drops a field, this fails. The registry-wide
/// half (every KindInfo kind, field labels discovered, not tabled) is
/// `every_registry_kind_gets_its_tab_bar`.
#[test]
fn every_field_lives_on_exactly_one_section() {
    let mut harness = shell_harness();
    let all = super::op_tabs::SECTIONS;
    // Kind order == KindInfo::ALL table order == the ribbon's TYPES;
    // `retype` is the law the quick-adds and the retype menu's command
    // run, so the draft takes the same shape the buttons drive it into.
    let table = [
        (
            "Face",
            vec![
                ("Geometry", vec!["Click selects:"]),
                ("Heights", vec!["Clearance Z", "Cut Z", "Levels"]),
                ("Passes", vec!["Stepover (Y spacing)"]),
                ("Feeds", vec!["Feed", "Plunge feed", "Climb milling"]),
            ],
        ),
        (
            "Contour",
            vec![
                ("Geometry", vec!["Click selects:", "Side", "Tolerance"]),
                (
                    "Heights",
                    vec![
                        "Clearance Z",
                        "Retract Z (between passes)",
                        "Top Z",
                        "Bottom Z",
                        "Stepdown",
                    ],
                ),
                ("Passes", vec!["Stock to leave"]),
                ("Lead in / Lead out", vec!["Lead in/out", "Entry", "Level to level", "Between levels", "Between passes", "Plunge", "Ramp / helix / arc feed", "Entry plunge feed", "Lead-in feed", "Engagement", "Lead out", "Before lead-in", "After lead-out", "Vertical lead-in height", "Overlap past the start", "Compensation", "Tab count", "Tab width", "Tab height"]),
                ("Feeds", vec!["Feed", "Plunge feed", "Climb milling"]),
            ],
        ),
        (
            "Pocket",
            vec![
                ("Geometry", vec!["Click selects:"]),
                ("Heights", vec!["Clearance Z", "Levels", "Top Z", "Bottom Z", "Stepdown"]),
                ("Passes", vec!["Stepover", "Stock to leave", "Strategy"]),
                ("Lead in / Lead out", vec!["Entry", "Level to level", "Between levels", "Between passes", "Plunge", "Ramp / helix / arc feed", "Entry plunge feed", "Lead-in feed", "Engagement", "Minimum radius", "Helix pitch"]),
                ("Feeds", vec!["Feed", "Plunge feed", "Climb milling"]),
            ],
        ),
        (
            "Slot",
            vec![
                ("Geometry", vec!["Click selects:", "A X", "A Y", "B X", "B Y", "Slot width"]),
                ("Heights", vec!["Clearance Z", "Top Z", "Bottom Z", "Stepdown"]),
                ("Passes", vec!["Stock to leave", "Stepover"]),
                ("Lead in / Lead out", vec!["Entry", "Level to level", "Between levels", "Between passes"]),
                ("Feeds", vec!["Feed", "Plunge feed", "Climb milling"]),
            ],
        ),
        (
            "Trace",
            vec![
                ("Geometry", vec!["Click selects:"]),
                ("Heights", vec!["Clearance Z", "Levels", "Top Z", "Bottom Z", "Stepdown"]),
                ("Feeds", vec!["Feed", "Plunge feed", "Climb milling"]),
            ],
        ),
        (
            // The roughing twins (M2.4 A): the pocket's pane with the
            // Strategy combo retired - the kind IS the strategy, so the
            // choice that would undo the name is not offered. Geometry
            // is the bare pick law like Pocket's: stock silhouette by
            // default, picks may bound it (the rim law's closed-chain
            // refusal is proved over a Selection src). Adaptive shows
            // the engagement door the name pins open; Offset has no
            // such field. "Retract Z" and "Helix radius" are
            // guard-rendered (the draft has them off), so they are not
            // in these rows.
            "2D Adaptive",
            vec![
                ("Geometry", vec!["Click selects:"]),
                (
                    "Heights",
                    vec!["Clearance Z", "Levels", "Top Z", "Bottom Z", "Stepdown"],
                ),
                ("Passes", vec!["Stepover", "Stock to leave", "Optimal load (% of D)"]),
                ("Lead in / Lead out", vec!["Entry", "Level to level", "Between levels", "Between passes"]),
                ("Feeds", vec!["Feed", "Plunge feed", "Climb milling"]),
            ],
        ),
        (
            "2D Offset",
            vec![
                ("Geometry", vec!["Click selects:"]),
                (
                    "Heights",
                    vec!["Clearance Z", "Levels", "Top Z", "Bottom Z", "Stepdown"],
                ),
                ("Passes", vec!["Stepover", "Stock to leave"]),
                ("Lead in / Lead out", vec!["Entry", "Level to level", "Between levels", "Between passes"]),
                ("Feeds", vec!["Feed", "Plunge feed", "Climb milling"]),
            ],
        ),
        (
            // The engrave twin (M2.4 B): Trace's row verbatim - the
            // shared arm renders the shared shape; only the height
            // editor's ids carry the catalog name ("Retract Z" rides
            // the hop's guard, off in the draft, as Trace's row notes).
            "Engrave",
            vec![
                ("Geometry", vec!["Click selects:"]),
                ("Heights", vec!["Clearance Z", "Levels", "Top Z", "Bottom Z", "Stepdown"]),
                ("Feeds", vec!["Feed", "Plunge feed", "Climb milling"]),
            ],
        ),
        (
            // The chamfer ride (M2.4 B): the cone's two authored
            // numbers and the contour's side/tolerance pair live in
            // Geometry; there is NO Passes group - the floor is
            // derived (has_section's exclusion) and a depth field
            // could only disagree with width x tan(angle). The corner
            // Z read is derived text, not a field; "Retract Z" rides
            // the hop's guard.
            "2D Chamfer",
            vec![
                (
                    "Geometry",
                    vec!["Click selects:", "Chamfer width", "Chamfer angle", "Side", "Tolerance"],
                ),
                ("Heights", vec!["Clearance Z", "Levels", "Top Z"]),
                ("Feeds", vec!["Feed", "Plunge feed", "Climb milling"]),
            ],
        ),
        (
            // The circular ladder (M2.4 B): the geometry is authored
            // like Slot's (a centre and a radius, no pick to disagree
            // with the circles - "Click selects:" still heads the
            // group, the shared pick law). Heights and Passes are the
            // pocket's shape; the vault is one checkbox. "Helix lead
            // radius" is guard-rendered (the draft enters by plunge),
            // so it is not in this row.
            "Circular",
            vec![
                (
                    "Geometry",
                    vec!["Click selects:", "Center X", "Center Y", "Radius", "Vault - dome floor"],
                ),
                (
                    "Heights",
                    vec!["Clearance Z", "Levels", "Top Z", "Bottom Z", "Stepdown"],
                ),
                ("Passes", vec!["Stock to leave", "Stepover"]),
                ("Lead in / Lead out", vec!["Entry", "Level to level", "Between levels", "Between passes"]),
                ("Feeds", vec!["Feed", "Plunge feed", "Climb milling"]),
            ],
        ),
        (
            // The thread mill (M2.4 C) seats here, closing the 2D
            // mill's run: the hole list again - one helix per hole -
            // and a Passes group with the one number the drill's
            // cycle never spoke (radial infeed). The pitch is the
            // TOOL's, and the pane says so in prose, not a field.
            "Thread",
            vec![
                ("Geometry", vec!["Click selects:", "Holes", "Detect holes"]),
                (
                    "Heights",
                    vec![
                        "Clearance Z",
                        "R plane (feed starts here)",
                        "Levels",
                        "Hop below clearance",
                        "Override the hole floors",
                    ],
                ),
                ("Passes", vec!["Radial infeed per pass"]),
                ("Feeds", vec!["Feed", "Plunge feed", "Climb milling"]),
            ],
        ),
        (
            // A drill has no climb (the cycle owns the retract), no
            // Lead in / Lead out (no wall to enter), and its Passes are the
            // cycle's own numbers - the section law this row pins.
            "Drill",
            vec![
                ("Geometry", vec!["Click selects:", "Holes", "Detect holes"]),
                (
                    "Heights",
                    vec![
                        "Clearance Z",
                        "R plane (feed starts here)",
                        "Levels",
                        "Hop below clearance",
                        "Override the hole floors",
                    ],
                ),
                ("Passes", vec!["Cycle", "Peck depth", "Dwell at bottom"]),
                ("Feeds", vec!["Feed", "Plunge feed"]),
            ],
        ),
        (
            // The boring ride (M2.4 C) joins drilling beside its
            // cousin: the drill's hole-list shape, the pocket's
            // Passes (a leave and a chord tolerance), and the
            // stepdown's ladder spoken on one circle per hole.
            "Bore",
            vec![
                ("Geometry", vec!["Click selects:", "Holes", "Detect holes"]),
                (
                    "Heights",
                    vec![
                        "Clearance Z",
                        "R plane (feed starts here)",
                        "Levels",
                        "Hop below clearance",
                        "Override the hole floors",
                        "Stepdown",
                    ],
                ),
                ("Passes", vec!["Stock to leave", "Tolerance"]),
                ("Lead in / Lead out", vec!["Level to level", "Between levels", "Between passes"]),
                ("Feeds", vec!["Feed", "Plunge feed", "Climb milling"]),
            ],
        ),
        (
            // The 3-D pocket picks no geometry (the mesh silhouette is
            // the region - islands are computed, never selected): its
            // Geometry group is the bare pick law, and its Passes carry
            // the entry ladder's own fields. "Retract Z", "Helix lead
            // radius" and "Optimal load (% of D)" are guard-rendered
            // (retype's defaults have none of them on), so they are not
            // in this table.
            "3D Pocket",
            vec![
                ("Geometry", vec!["Click selects:"]),
                (
                    "Heights",
                    vec![
                        "Clearance Z",
                        "Levels",
                        "Hop below clearance",
                        "Top Z",
                        "Bottom Z",
                        "Stepdown",
                    ],
                ),
                ("Passes", vec![
                    "Stepover",
                    "Stock to leave",
                    "Strategy",
                    "Offset shells",
                    "Rest roughing",
                    "Feed optimization",
                ]),
                ("Lead in / Lead out", vec!["Entry", "Level to level", "Between levels", "Between passes"]),
                ("Feeds", vec!["Feed", "Plunge feed", "Climb milling"]),
            ],
        ),
        (
            // The finish picks no geometry (the whole model is its
            // region) and has no stepdown: tolerance authors the level
            // spacing itself, and - under the raster patterns - the line
            // spacing too (the cos-law, M2.2b). The Stepover row renders
            // because retype's default pattern is Parallel; its override
            // checkbox is the field's guard, not a field of its own.
            "3D Finish",
            vec![
                ("Geometry", vec!["Click selects:"]),
                (
                    "Heights",
                    vec![
                        "Clearance Z",
                        "Levels",
                        "Hop below clearance",
                        "Top Z",
                        "Bottom Z",
                    ],
                ),
                ("Passes", vec![
                    "Pattern",
                    "Stepover",
                    "Minimum step",
                    "Ramp between lines",
                    "Tolerance",
                ]),
                ("Feeds", vec!["Feed", "Plunge feed", "Climb milling"]),
            ],
        ),
        (
            // The deburr ride (M2.5 E B): one authored number, the
            // break width, in Geometry with the derived tip-depth
            // note; the window's two planes stand in Heights (their
            // labels name the TIP's law, not a cut floor), and the
            // feeds are the ride's - no climb row, the cone follows
            // the edge wherever the weld puts it. There is NO Passes
            // group: one edge, one pass - the loci are what the weld
            // exposes, and a stepover could only disagree with the
            // taxonomy (has_section's exclusion, Chamfer2D's answer
            // once more).
            "Deburr",
            vec![
                ("Geometry", vec!["Break width"]),
                ("Heights", vec!["Clearance Z", "Window Top Z", "Window Bottom Z"]),
                ("Feeds", vec!["Feed", "Plunge feed"]),
            ],
        ),
    ];
    // One tab at a time: open each section in turn (the view-state field
    // the tab click writes) and read the tree. A field must be in the tree
    // exactly once on its own tab (a field double-guarded twice on one tab
    // shows as count 2) and absent on every other - an out-of-set tab
    // falls back to Geometry, the page's resolution law.
    for (ki, (kind_name, sections)) in table.iter().enumerate() {
        let cur = harness.state().state.op_ui.params.clone();
        let retyped = crate::operations::retype(&cur, ki);
        harness.state_mut().state.op_ui.params = retyped;
        let mut placement: Vec<(&str, usize)> = Vec::new();
        for (sname, labels) in sections {
            let si = all
                .iter()
                .position(|s| s == sname)
                .expect("the table names the SECTIONS");
            for l in labels.iter() {
                placement.push((*l, si));
            }
        }
        for (si, tab_name) in all.iter().enumerate() {
            harness.state_mut().state.op_tab = si;
            harness.run();
            let has_tab = sections.iter().any(|(n, _)| n == tab_name);
            assert_eq!(
                harness.query_by_label(tab_name).is_some(),
                has_tab,
                "{kind_name}: the tab bar must show '{tab_name}' iff the kind has fields for it",
            );
            let effective = if has_tab { si } else { 0 };
            for (label, want) in &placement {
                let n = harness.query_all_by_label(label).count();
                assert_eq!(
                    n,
                    usize::from(*want == effective),
                    "{kind_name}: '{label}' renders {n} time(s) on tab {si}; it belongs to tab \
                     {want} - exactly once there, nowhere else",
                );
            }
        }
    }
}

/// A tab click is navigation, nothing more: the pane's view-state field
/// moves, no command is queued, and the section's fields replace the
/// previous ones (the view-state law from M1.4's toggle decision). The
/// open tab survives reselecting the same operation (the drafts resync,
/// the tab does not reset) and a kind change keeps the same section when
/// the new kind has it, else falls back to the first tab.
#[test]
fn tab_click_navigates_and_queues_nothing() {
    let mut harness = shell_harness();
    harness.run();
    assert!(harness.query_by_label("Side").is_some(), "Geometry opens by default");
    assert!(harness.query_by_label("Clearance Z").is_none(), "Heights waits behind its tab");
    harness.get_by_label("Heights").click();
    harness.run();
    {
        let st = &harness.state().state;
        assert_eq!(st.op_tab, 1, "the Heights cell did not open the Heights section");
        assert!(st.pending.is_empty(), "opening a section must not queue a command");
    }
    assert!(harness.query_by_label("Clearance Z").is_some());
    assert!(harness.query_by_label("Side").is_none(), "the closed tab's fields leave the tree");

    // Reselecting the same op: the drafts resync, the tab stays.
    harness.state_mut().state.sync_ops_ui();
    harness.run();
    assert_eq!(harness.state().state.op_tab, 1, "a resync must not reset the open tab");
    assert!(harness.query_by_label("Clearance Z").is_some());

    // Lead in / Lead out is the level-ladder kinds': open it on the Contour, then show
    // a Face (no ladder to link) - the tab falls back to Geometry; Heights stays.
    harness.get_by_label("Lead in / Lead out").click();
    harness.run();
    assert_eq!(harness.state().state.op_tab, 3);
    let cur = harness.state().state.op_ui.params.clone();
    harness.state_mut().state.op_ui.params = crate::operations::retype(&cur, 0);
    harness.run();
    assert_eq!(harness.state().state.op_tab, 0, "a missing section falls back to the first tab");
    assert!(harness.query_by_label("Lead in / Lead out").is_none(), "Face has no Lead tab");
    harness.get_by_label("Heights").click();
    harness.run();
    let cur = harness.state().state.op_ui.params.clone();
    harness.state_mut().state.op_ui.params = crate::operations::retype(&cur, 1);
    harness.run();
    assert_eq!(harness.state().state.op_tab, 1, "a section the new kind has stays open");
    assert!(harness.state().state.pending.is_empty(), "tab navigation never queues");
}

/// Every text the properties pane renders right now (labels and checkbox
/// captions), from the operation heading's left edge rightward - the pane
/// column, so the browser and ribbon text stay out.
fn pane_texts(h: &Harness<'static, ShellUi>) -> Vec<String> {
    use egui::accesskit::Role;
    use egui_kittest::kittest::NodeT;
    let left = h.get_by_label_contains("Operation: ").rect().left() - 1.0;
    h.query_all_by(|n| matches!(n.role(), Role::Label | Role::CheckBox))
        .filter(|n| n.rect().left() >= left)
        .filter_map(|n| {
            let a = n.accesskit_node();
            if a.role() == Role::Label { a.value() } else { a.label() }
        })
        // The property grid's checkboxes carry their caption as the row's
        // label; the box itself has no text.
        .filter(|t| !t.is_empty())
        .collect()
}

/// Whether `params_fields` draws anything for `sec` with the harness's
/// current draft - rendered off-screen in a throwaway context, so the
/// probe can ask a section the tab bar would never open.
fn section_draws(state: &mut State, sec: usize) -> bool {
    let ctx = egui::Context::default();
    let mut drew = false;
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        let before = ui.cursor().min;
        super::params_page::params_fields(ui, state, sec);
        drew = ui.cursor().min != before;
    });
    // Nothing paints this throwaway pass: the font atlas upload is dropped
    // on purpose (epaint refuses a silently dropped delta).
    out.textures_delta.clear();
    drew
}

/// The registry half of the tab law: the drift guard above tables the
/// milling kinds' field labels by hand; this one walks EVERY KindInfo kind
/// and discovers them. Per kind: the tab bar shows exactly `has_section`'s
/// tabs, and `has_section` matches what the per-section guards really draw
/// (a tab with no fields is dead, a section with fields but no tab loses
/// them - both fail; Geometry always carries the pick block); and every
/// pane text that is not chrome (present on all tabs) appears on exactly
/// one tab. A field added under an existing section guard - in any kind -
/// is covered with no edit here.
#[test]
fn every_registry_kind_gets_its_tab_bar() {
    let mut harness = shell_harness();
    harness.run();
    let secs = super::op_tabs::SECTIONS;
    for (ki, k) in camcraft_app::operation::KindInfo::ALL.iter().enumerate() {
        let cur = harness.state().state.op_ui.params.clone();
        let retyped = crate::operations::retype(&cur, ki);
        let kind = retyped.kind();
        harness.state_mut().state.op_ui.params = retyped;
        // Geometry (0) is skipped: the page's pick block makes it never dead.
        for (si, sname) in secs.iter().enumerate().skip(1) {
            let drew = section_draws(&mut harness.state_mut().state, si);
            let has = super::op_tabs::has_section(kind, si);
            assert_eq!(
                drew, has,
                "{}: '{sname}' draws fields = {drew}, but has_section says {has} - the tab law \
                 and the section guards disagree",
                k.label,
            );
        }
        let mut per_tab: Vec<(usize, Vec<String>)> = Vec::new();
        for (si, sname) in secs.iter().enumerate() {
            harness.state_mut().state.op_tab = si;
            harness.run();
            let st = &harness.state().state;
            assert!(st.pending.is_empty(), "{}: opening a tab queues nothing", k.label);
            let has = super::op_tabs::has_section(kind, si);
            assert_eq!(
                harness.query_all_by_label(sname).next().is_some(),
                has,
                "{}: the '{sname}' tab must follow has_section({si})",
                k.label,
            );
            if has {
                assert!(harness.query_by_label("Tool").is_some(), "{}: the pane opens on the tool row", k.label);
                per_tab.push((si, pane_texts(&harness)));
            }
        }
        // Chrome: what every open tab shares (heading, tool/source rows,
        // tab captions, footer). Everything else is a section's own.
        let chrome: Vec<&String> = per_tab[0]
            .1
            .iter()
            .filter(|t| per_tab.iter().all(|(_, ts)| ts.contains(t)))
            .collect();
        for (si, texts) in &per_tab {
            let own: Vec<&String> = texts.iter().filter(|t| !chrome.contains(t)).collect();
            assert!(!own.is_empty(), "{}: the '{}' tab is dead - nothing of its own", k.label, secs[*si]);
            for t in own {
                let on: Vec<&str> = per_tab
                    .iter()
                    .filter(|(_, ts)| ts.contains(t))
                    .map(|(s, _)| secs[*s])
                    .collect();
                assert_eq!(on.len(), 1, "{}: '{t}' renders on tabs {on:?} - one tab per field", k.label);
            }
        }
    }
}

// ------------------------------------------------------- gesture plumbing
//
// The harness has no mouse loop, so pointer gestures are built from egui's
// own event types - the same RawInput stream the winit backend feeds the
// live window. One event per frame: press and release are separate frames,
// exactly like a human click, and `run_steps(2)` afterwards lets the shell's
// queue law execute what the gesture staged.

fn set_pos(h: &mut Harness<'static, ShellUi>, p: egui::Pos2) {
    h.input_mut().events.push(Event::PointerMoved(p));
    h.run();
}

fn button_at(h: &mut Harness<'static, ShellUi>, b: egui::PointerButton, p: egui::Pos2, pressed: bool) {
    h.input_mut().events.push(Event::PointerButton {
        pos: p,
        button: b,
        pressed,
        modifiers: Modifiers::NONE,
    });
    h.run();
}

/// A plain (button-1) click at `p`, then the frames that drain its command.
fn click_viewport(h: &mut Harness<'static, ShellUi>, p: egui::Pos2) {
    set_pos(h, p);
    button_at(h, egui::PointerButton::Primary, p, true);
    button_at(h, egui::PointerButton::Primary, p, false);
    h.run_steps(2);
}

/// The pane's screen center, once a frame has laid it out.
fn vp_center(sh: &ShellUi) -> egui::Pos2 {
    let ([x0, y0], [x1, y1]) = sh.state.viewport_rect;
    egui::pos2((x0 + x1) * 0.5, (y0 + y1) * 0.5)
}

/// M-e's core claim in the egui shell: a click inside the pane runs the
/// SHARED `interact::pick_at` law - status, hover, and a Select command
/// that moves the tree highlight off the operation the seed had selected.
#[test]
fn click_in_viewport_selects_model() {
    let mut harness = shell_harness();
    let c = vp_center(harness.state());
    click_viewport(&mut harness, c);
    let st = &harness.state().state;
    assert_eq!(st.status, "selected Block", "the center ray must hit the block");
    assert!(
        !st.pending
            .iter()
            .any(|c| matches!(c, CommandKind::Select(_))),
        "the queued command outlived the drain"
    );
    let block = model::find_by_name(&st.world, "Block").expect("seed has Block");
    assert!(model::is_selected(&st.world, &block), "click did not select the model");
    let op = model::find_by_name(&st.world, "Contour1").expect("seed has Contour1");
    assert!(
        !model::is_selected(&st.world, &op),
        "a model click must move the highlight off the operation"
    );
}

/// The hot-window gate, the negative way: a click far outside the pane's
/// rect must not touch the pick machinery at all - no status, no queue.
/// (The corner is ribbon paint; paint is not the viewport.)
#[test]
fn click_outside_the_viewport_is_not_a_pick() {
    let mut harness = shell_harness();
    harness.run();
    let status0 = harness.state().state.status.clone();
    click_viewport(&mut harness, egui::pos2(4.0, 4.0));
    let st = &harness.state().state;
    assert_eq!(st.status, status0, "a stray click ran a pick");
    assert!(
        !st.pending
            .iter()
            .any(|c| matches!(c, CommandKind::Select(_))),
        "the stray click queued a command"
    );
}

/// The travel threshold is THE law separating click from orbit: past
/// CLICK_TRAVEL_PX the gesture is a drag - the camera turns, the release
/// picks nothing, and no command rides the queue.
#[test]
fn drag_promotes_past_the_travel_threshold() {
    let mut harness = shell_harness();
    harness.run();
    let c = vp_center(harness.state());
    let status0 = harness.state().state.status.clone();
    let (yaw0, pitch0) = (harness.state().state.cam.yaw, harness.state().state.cam.pitch);
    set_pos(&mut harness, c);
    button_at(&mut harness, egui::PointerButton::Primary, c, true);
    let d = c + egui::vec2(25.0, 0.0);
    set_pos(&mut harness, d);
    {
        let st = harness.state();
        assert_eq!(st.state.drag, Some(DragMode::Orbit), "25 px must promote to orbit");
        assert_ne!(st.state.cam.yaw, yaw0, "orbit must turn yaw");
        assert_eq!(st.state.cam.pitch, pitch0, "a pure horizontal drag keeps pitch");
    }
    button_at(&mut harness, egui::PointerButton::Primary, d, false);
    harness.run_steps(2);
    let st = &harness.state().state;
    assert_eq!(st.drag, None, "the release must end the drag");
    assert_eq!(st.status, status0, "a drag must not pick");
    assert!(
        !st.pending
            .iter()
            .any(|c| matches!(c, CommandKind::Select(_))),
        "the drag queued a Select"
    );
}

/// Middle orbits, right pans, and neither is a pick - the drag arms of the
/// shared law, driven through egui's own button events.
#[test]
fn mmb_orbits_and_rmb_pans() {
    let mut harness = shell_harness();
    let c = vp_center(harness.state());
    let status0 = harness.state().state.status.clone();
    set_pos(&mut harness, c);
    let (yaw0, t0) = (harness.state().state.cam.yaw, harness.state().state.cam.target);
    button_at(&mut harness, egui::PointerButton::Middle, c, true);
    let m1 = c + egui::vec2(20.0, 10.0);
    set_pos(&mut harness, m1);
    {
        let st = harness.state();
        assert_eq!(st.state.drag, Some(DragMode::Orbit));
        assert_ne!(st.state.cam.yaw, yaw0, "MMB drag must orbit");
        assert_eq!(st.state.cam.target, t0, "orbit must not move the target");
    }
    button_at(&mut harness, egui::PointerButton::Middle, m1, false);
    assert_eq!(harness.state().state.drag, None, "release must end the drag");

    button_at(&mut harness, egui::PointerButton::Secondary, c, true);
    // yaw1: the MMB orbit left its mark - the pan must not add another.
    let yaw1 = harness.state().state.cam.yaw;
    let m2 = c + egui::vec2(15.0, 0.0);
    set_pos(&mut harness, m2);
    {
        let st = harness.state();
        assert_eq!(st.state.drag, Some(DragMode::Pan));
        assert_ne!(st.state.cam.target, t0, "RMB drag must pan the target");
        assert_eq!(st.state.cam.yaw, yaw1, "pan must not rotate");
    }
    button_at(&mut harness, egui::PointerButton::Secondary, m2, false);
    harness.run_steps(2);
    let st = &harness.state().state;
    assert_eq!(st.drag, None);
    assert_eq!(st.status, status0, "camera gestures must not report picks");
    assert!(st.pending.is_empty(), "camera gestures are not commands");
}

/// Wheel = dolly, one notch of the Line unit to one step of
/// `dolly_cursor`, positive toward the cursor - the same units the imgui
/// shell's raw `mouse_wheel` feeds. Pure camera state: nothing queued.
#[test]
fn wheel_dollies_toward_the_cursor() {
    let mut harness = shell_harness();
    let c = vp_center(harness.state());
    let status0 = harness.state().state.status.clone();
    set_pos(&mut harness, c);
    let d0 = harness.state().state.cam.dist;
    harness.input_mut().events.push(Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, 1.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::NONE,
    });
    harness.run_steps(2);
    let st = &harness.state().state;
    assert!(
        st.cam.dist < d0 * 0.95,
        "one notch in must shorten dist (got {} vs {})",
        st.cam.dist,
        d0
    );
    assert!(st.pending.is_empty());
    assert_eq!(st.status, status0);
}

/// The Geometry section's radio row drives `state.geo_pick` - the same
/// immediate-mode law as the imgui radio, no command - and a Face-mode
/// click on a vertical block face queues SetOperationGeometry for the open
/// operation. The seed's Contour1 already carries the TOP face; the click
/// must REPLACE it with the side face (a plain click is a set, not an
/// add), naming both the face and the operation in the status.
#[test]
fn face_mode_click_replaces_geometry_of_the_open_op() {
    let mut harness = shell_harness();
    harness.run();
    harness.get_by_label("Faces").click();
    harness.run();
    assert_eq!(harness.state().state.geo_pick, GeoPick::Face);

    // The four vertical faces of the seed's 60x60x30 block; the most
    // front-facing one whose projection sits clear inside the pane is the
    // deterministic click target (a convex body's front faces tile the
    // silhouette, so that pixel ray enters that face - never the top).
    let ([x0, y0], [x1, y1]) = harness.state().state.viewport_rect;
    let (w, hgt) = (x1 - x0, y1 - y0);
    let mvp = harness.state().state.cam.mvp((w / hgt) as f64);
    let eye = harness.state().state.cam.eye_f32();
    let sides = [
        ([60.0, 30.0, 15.0], [1.0, 0.0, 0.0]),
        ([0.0, 30.0, 15.0], [-1.0, 0.0, 0.0]),
        ([30.0, 60.0, 15.0], [0.0, 1.0, 0.0]),
        ([30.0, 0.0, 15.0], [0.0, -1.0, 0.0]),
    ];
    let mut best: Option<(f32, egui::Pos2)> = None;
    for (c, n) in sides {
        let facing = ((f64::from(eye[0]) - c[0]) * n[0]
            + (f64::from(eye[1]) - c[1]) * n[1]
            + (f64::from(eye[2]) - c[2]) * n[2]) as f32;
        let Some(uv) = interact::project_uv(&mvp, c) else { continue };
        if !(uv[0] > 0.1 && uv[0] < 0.9 && uv[1] > 0.15 && uv[1] < 0.85 && facing > 0.0) {
            continue;
        }
        let p = egui::pos2(x0 + uv[0] * w, y0 + uv[1] * hgt);
        if best.is_none_or(|(bf, _)| facing > bf) {
            best = Some((facing, p));
        }
    }
    let p = best.expect("the block shows a vertical face to the default camera").1;
    click_viewport(&mut harness, p);
    let st = &harness.state().state;
    // The pick's "added to" line rides one frame, then the drain's
    // execution status replaces it - seeing the executed status is the
    // proof the command went through the machinery, not a field poke.
    assert_eq!(st.status, "geometry selection updated for Contour1");
    assert!(
        !st.pending
            .iter()
            .any(|c| matches!(c, CommandKind::SetOperationGeometry { .. })),
        "the queued command outlived the drain"
    );
    let op = model::find_by_name(&st.world, "Contour1").expect("seed has Contour1");
    let items = match model::operation_of(&st.world, &op).map(|o| o.src) {
        Some(SrcRef::Selection(items)) => items,
        other => panic!("Contour1's source became {other:?}"),
    };
    assert_eq!(items.len(), 1, "a plain click sets the list, not grows it");
    assert_eq!(items[0].entity, "Block");
    let FeatureSel::Face(fid) = items[0].feature else {
        panic!("a face click must queue a face, got {:?}", items[0].feature);
    };
    // And it must be the SIDE face the pixel was chosen for - the seed's
    // top face has normal z = 1; a vertical face's is ~0. This is what
    // distinguishes "replaced with the click" from "no-op on the old pick".
    let block = model::find_by_name(&st.world, "Block").expect("seed has Block");
    let faces = model::features_of(&st.world, &block).expect("Block has features");
    let face = faces.faces.iter().find(|f| f.id == fid).expect("picked face exists");
    assert!(face.normal[2].abs() < 0.3, "the click landed on a horizontal face");
}

/// Anchor pick mode through the viewport: a click ON a marker queues the
/// SAME `SetWcs` the pane's grid buttons queue, at that anchor's exact
/// position. `pick_anchor` is immediate UI state, like the pick-mode radio
/// above - not undoable, not queued.
#[test]
fn anchor_click_queues_setwcs() {
    let mut harness = shell_harness();
    let slot = WCS_SLOTS[0];
    let pos0 = {
        let e = model::find_by_name(&harness.state().state.world, slot).expect("seed sets G54");
        let p = model::transform_of(&harness.state().state.world, &e).unwrap_or_default().pos;
        [p.x, p.y, p.z]
    };
    // The first anchor that projects clearly inside the laid-out pane.
    let ([x0, y0], [x1, y1]) = harness.state().state.viewport_rect;
    let (w, hgt) = (x1 - x0, y1 - y0);
    let mvp = harness.state().state.cam.mvp((w / hgt) as f64);
    let (want, p) = interact::anchor_points(&harness.state().state)
        .into_iter()
        .filter_map(|(pt, _)| {
            let uv = interact::project_uv(&mvp, pt)?;
            (uv[0] > 0.02 && uv[0] < 0.98 && uv[1] > 0.02 && uv[1] < 0.98)
                .then_some((pt, egui::pos2(x0 + uv[0] * w, y0 + uv[1] * hgt)))
        })
        .next()
        .expect("the anchor grid faces the default camera");
    assert!(
        (want[0] - pos0[0]).abs() + (want[1] - pos0[1]).abs() + (want[2] - pos0[2]).abs() > 1e-6,
        "the anchor coincides with the seed's origin - the assertion would prove nothing"
    );

    harness.state_mut().state.wcs_ui.pick_anchor = true;
    click_viewport(&mut harness, p);
    let st = &harness.state().state;
    assert!(
        !st.pending
            .iter()
            .any(|c| matches!(c, CommandKind::SetWcs { .. })),
        "the queued command outlived the drain"
    );
    let e = model::find_by_name(&st.world, slot).expect("G54 exists");
    let q = model::transform_of(&st.world, &e).unwrap_or_default().pos;
    let pos = [q.x, q.y, q.z];
    assert!(
        (pos[0] - want[0]).abs() < 1e-6
            && (pos[1] - want[1]).abs() < 1e-6
            && (pos[2] - want[2]).abs() < 1e-6,
        "the anchor click did not place {slot} at the marker (got {pos:?}, wanted {want:?})"
    );
}

/// The pane half of the same law - the regression this pins: the egui port
/// kept `interact.rs`'s markers and viewport pick but lost the Origin
/// page's widgets (they lived in the imgui panes, retired un-ported at
/// M-g), so nothing could set `pick_anchor` (the crosses never rendered)
/// and the 3x3 grid was gone. The page must show the compass grid, place
/// the slot on the SAME point the markers draw from when a cell is
/// clicked (through SetWcs - the draft law), bracket the chosen cell, and
/// arm pick mode from the checkbox.
#[test]
fn origin_page_shows_the_anchor_grid_and_arms_pick_mode() {
    let mut harness = shell_harness();
    harness.state_mut().state.ui_target = UiTarget::Wcs;
    harness.run();
    // The draft default highlight is center (anchor_xy 4), so that cell
    // already reads bracketed - imgui's law, pinned here too.
    for name in ["NW", "N", "NE", "W", "C", "E", "SW", "S", "SE"] {
        let label = if name == "C" { "[C]" } else { name };
        assert!(
            harness.query_by_label(label).is_some(),
            "the compass cell {name} must be on the Origin page"
        );
    }
    assert!(!harness.state().state.wcs_ui.pick_anchor, "pick mode starts off");

    harness.get_by_label("SE").click();
    harness.run();
    let want = interact::anchor_points(&harness.state().state)[8].0;
    let e = model::find_by_name(&harness.state().state.world, WCS_SLOTS[0])
        .expect("the seed places G54");
    let q = model::transform_of(&harness.state().state.world, &e).unwrap_or_default().pos;
    let got = [q.x, q.y, q.z];
    assert!(
        got.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-6),
        "the grid click must place the slot on the anchor the markers draw ({q:?} vs {want:?})"
    );
    assert_eq!(harness.state().state.wcs_ui.anchor_xy, 8, "the click names the cell");
    // The chosen cell reads bracketed, imgui's highlight law.
    harness.run();
    assert!(
        harness.query_by_label("[SE]").is_some(),
        "the selected cell must show its brackets"
    );

    // The checkbox is the one writer of pick mode - without it the
    // viewport crosses are unreachable (this half of the regression).
    harness.get_by_label("Pick markers in viewport").click();
    harness.run();
    assert!(
        harness.state().state.wcs_ui.pick_anchor,
        "the checkbox must arm the marker overlay"
    );
}

/// The exit contract, lib side (the integration tests/egui_app.rs covers the
/// `--ui egui` dispatch; this one proves the headless body directly,
/// including the View3D readback and the fallback-adapter law).
#[test]
fn headless_run_succeeds() {
    headless::run().map_err(|err| panic!("egui headless run failed: {err}")).ok();
}

/// The churn gate for the migration: one rendered frame of the whole shell,
/// compared to `tests/snapshots/egui_app.png` (relative to this crate).
/// Release lane since #255: PRs never change the PNG; it is regenerated
/// once at a cut with `UPDATE_SNAPSHOTS=1` (with `--ignored`, CWD
/// crates/ui); a failure writes `.new.png`/`.diff.png` beside it for eyes. Two Windows-refreshed goldens
/// below use a Linux-only threshold for isolated antialiasing differences.
#[test]
#[ignore = "release lane (#255): pixel snapshot golden, run and regenerated only at minor/major cuts (gate -Tier release); structural twin in tests/structural.rs"]
fn egui_shell_snapshot() {
    let _g = snapshot_guard();
    let mut harness = shell_harness();
    harness.run_steps(3);
    harness.snapshot("egui_app");
}

/// The pixels are process-global: `theme::current()` is ONE live table
/// for the whole test binary, and the light golden flips it for its
/// frames (the sync law is the shipping code, not a test-local poke).
/// The dark snapshot tests (and their structural twins) read that same
/// table through their frames, so all of them share this lock - without it a parallel dark frame
/// could catch the light table and fail its own golden, and vice versa.
/// Any other test whose frames move the world's theme (the Theme door's
/// switch, #605) writes that table too and must take this lock as well.
static SNAPSHOT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn snapshot_guard() -> std::sync::MutexGuard<'static, ()> {
    SNAPSHOT_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// M6.1 D's paired golden: the light column rendered through the
/// switcher's own door - the reference rides the world as a `ThemeRef`,
/// the frame-edge sync lands it - so the pixels prove the PATH, not
/// just the table (ROADMAP: "the light theme ships alongside as a
/// second golden set"). Regenerate deliberately with
/// UPDATE_SNAPSHOTS=1; the restore at the end leaves the house look
/// every other frame expects.
#[test]
#[ignore = "release lane (#255): pixel snapshot golden, run and regenerated only at minor/major cuts (gate -Tier release); structural twin in tests/structural.rs"]
fn egui_shell_snapshot_light() {
    let _g = snapshot_guard();
    let mut harness = shell_harness();
    theme_set(&harness.state_mut().state.world, &ThemeRef::Named("light".to_owned()));
    harness.run_steps(3);
    assert_eq!(theme::current(), theme::Tokens::LIGHT, "the frame-edge sync rode the world's reference");
    // The ROADMAP's sentence, checked on the table that is on screen:
    // a theme that cannot be read is a visible refusal, not a style
    // preference - and the light column is not a refusal.
    assert_eq!(contrast::readable(&theme::current()), None, "the light golden's own table reads");
    // The golden was refreshed on Windows (#66). On Linux its antialiasing
    // differs at isolated pixels (none exceed 1.0); keep the other OSes'
    // 0.6 threshold and leave the image itself as the shared reference.
    harness.snapshot_options(
        "egui_app_light",
        &egui_kittest::SnapshotOptions::new()
            .threshold(egui_kittest::OsThreshold::<f32>::default().linux(1.0)),
    );
    theme_set(&harness.state_mut().state.world, &ThemeRef::Dark);
    harness.run_steps(1); // leave LIVE dark for whatever frame follows.
}

/// M1.4: the graphical tool editor, end to end through the real drain. The
/// dock page is the LIBRARY (no field widgets, so every label queried here
/// resolves in exactly one surface); `Edit` opens the window on the world's
/// numbers; a drag stages into `tool_ui` WITHOUT touching the world (the
/// draft law); `Apply` lands it through `ChangeTool`; and the dock's
/// `Delete` proves the ghost law - the named tool is gone, so the window
/// is gone the next frame.
#[test]
fn tool_editor_window_edits_live_and_stages_through_commands() {
    let mut harness = shell_harness();
    // Browser rows are painted strips; retargeting is the established
    // navigation idiom (state_mut like the gesture tests).
    harness.state_mut().state.ui_target = UiTarget::Tool("EM-0600-4FL".to_owned());
    harness.run();
    harness.run();
    let world_d = |h: &Harness<'static, ShellUi>| {
        model::find_by_name(&h.state().state.world, "EM-0600-4FL")
            .and_then(|e| model::tool_of(&h.state().state.world, &e))
            .map(|t| t.diameter)
    };
    assert!(
        harness.query_by_label("Diameter").is_none(),
        "the dock library must not own the editor's labels"
    );
    harness.get_by_label("Edit").click();
    harness.run();
    harness.run();
    assert_eq!(harness.state().state.tool_editor.as_deref(), Some("EM-0600-4FL"));
    assert!(harness.state().state.pending.is_empty(), "opening must queue nothing");
    let d0 = world_d(&harness).expect("the seed owns EM-0600-4FL");
    assert!(
        (harness.state().state.tool_ui.diameter - d0).abs() < 1e-12,
        "the window must open on the world's diameter"
    );
    // The window's first SpinButton is Diameter (the library renders no
    // DragValues, so "first" is deterministic): 30 px at speed 0.1.
    let c = harness
        .get_all_by_role(egui::accesskit::Role::SpinButton)
        .next()
        .expect("the editor window owns DragValues")
        .rect()
        .center();
    harness.drag_at(c);
    harness.run();
    harness.hover_at(c + Vec2::new(30.0, 0.0));
    harness.run();
    harness.drop_at(c + Vec2::new(30.0, 0.0));
    harness.run();
    assert!(
        (harness.state().state.tool_ui.diameter - (d0 + 3.0)).abs() < 0.05,
        "the drag must stage the draft"
    );
    assert!(
        (world_d(&harness).expect("still there") - d0).abs() < 1e-12,
        "a drag must not touch the world"
    );
    harness.get_by_label("Apply").click();
    harness.run();
    assert!(
        (world_d(&harness).expect("still there") - (d0 + 3.0)).abs() < 0.05,
        "Apply must land the staged number (status: {})",
        harness.state().state.status
    );
    // Ghost law: deleting the tool from the library closes the window.
    harness.get_by_label("Delete").click();
    harness.run();
    assert!(
        harness.state().state.tool_editor.is_none(),
        "the window must not outlive its tool"
    );
    assert!(
        harness.query_by_label("Shank diameter").is_none(),
        "the window's widgets must go with it"
    );
    assert!(world_d(&harness).is_none(), "Delete must empty the library too");
}

/// The editor's pixels are their own churn gate, so the shell snapshot can
/// keep its "window closed by default" law: fields left, banded to-scale
/// profile right, floating over the dock. Regenerate with UPDATE_SNAPSHOTS=1.
#[test]
#[ignore = "release lane (#255): pixel snapshot golden, run and regenerated only at minor/major cuts (gate -Tier release); structural twin in tests/structural.rs"]
fn tool_editor_snapshot() {
    let _g = snapshot_guard();
    let mut harness = shell_harness();
    harness.state_mut().state.ui_target = UiTarget::Tool("EM-0600-4FL".to_owned());
    harness.run();
    harness.run();
    harness.get_by_label("Edit").click();
    harness.run_steps(3);
    harness.snapshot_options(
        "egui_tool_editor",
        &egui_kittest::SnapshotOptions::new()
            .threshold(egui_kittest::OsThreshold::<f32>::default().linux(1.5)),
    );
}

// The S0 one-wgpu proof retired with the imgui renderer at M-g: with
// `dear_imgui_wgpu` gone there is a single wgpu in the tree by
// construction, and nothing left to coerce.

// ------------------------------------------------------- M-f: the actions law

/// The keyboard law M-f closes: every file key the ribbon tips advertise
/// must queue exactly its command, and must stay silent while a text field
/// owns focus. Driven through the keyboard layer alone (`shortcuts_only`),
/// so the proof is queue membership - never a drain that would raise rfd.
#[test]
fn file_keys_queue_their_commands() {
    let ctrl = Modifiers { command: true, ..Default::default() };
    let ctrl_shift = Modifiers { command: true, shift: true, ..Default::default() };
    let mut harness = Harness::builder()
        .with_size(Vec2::new(400.0, 200.0))
        .build_ui_state(shortcuts_only, State::new());

    // Ctrl+<key> arms exactly the command its tip promises. The shift
    // variant must split the two Open arms, and it must not double-fire.
    let expect = |h: &mut Harness<'static, State>,
                      mods: Modifiers,
                      key: Key,
                      want: fn(&CommandKind) -> bool,
                      name: &str| {
        h.state_mut().pending.clear();
        // ModifiersChanged first: i.modifiers only tracks that event (the
        // law proven in `history_key_queues_command`).
        h.input_mut().events.push(Event::ModifiersChanged(mods));
        h.input_mut().events.push(Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: mods,
        });
        h.run();
        let p = &h.state().pending;
        assert_eq!(p.len(), 1, "{name} armed {} commands, expected exactly one", p.len());
        assert!(want(&p[0]), "{name} armed {p:?}");
    };
    expect(&mut harness, ctrl, Key::N, |c| matches!(c, CommandKind::NewDocument), "Ctrl+N");
    expect(
        &mut harness,
        ctrl,
        Key::O,
        |c| matches!(c, CommandKind::OpenModelDialog),
        "Ctrl+O",
    );
    expect(
        &mut harness,
        ctrl_shift,
        Key::O,
        |c| matches!(c, CommandKind::OpenProjectDialog),
        "Ctrl+Shift+O",
    );
    expect(
        &mut harness,
        ctrl,
        Key::S,
        |c| matches!(c, CommandKind::SaveProjectDialog),
        "Ctrl+S",
    );
    expect(
        &mut harness,
        ctrl_shift,
        Key::S,
        |c| matches!(c, CommandKind::SaveProjectAsDialog),
        "Ctrl+Shift+S",
    );
    expect(
        &mut harness,
        ctrl,
        Key::P,
        |c| matches!(c, CommandKind::PostProgramDialog),
        "Ctrl+P",
    );

    // The focus gate: the same Ctrl+O over a focused text field arms
    // nothing (a dialog must never spawn mid-type - imgui's io.WantTextInput).
    let mut typing = Harness::builder()
        .with_size(Vec2::new(400.0, 200.0))
        .build_ui_state(shortcuts_with_field, State::new());
    typing.run();
    typing.state_mut().pending.clear();
    typing.input_mut().events.push(Event::ModifiersChanged(ctrl));
    typing.input_mut().events.push(Event::Key {
        key: Key::O,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: ctrl,
    });
    typing.run();
    assert!(
        typing.state().pending.is_empty(),
        "Ctrl+O armed a dialog over a focused text field: {:?}",
        typing.state().pending
    );
}

/// #434: bare P flips perspective/ortho in place - a viewport-layer
/// key like F, view state through `actions`, so it arms no command,
/// and Ctrl+P keeps its post dialog exactly as `file_keys_queue_their_commands`
/// proves. The typing gate applies to it the same way.
#[test]
fn bare_p_flips_projection_without_a_command() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(400.0, 200.0))
        .build_ui_state(shortcuts_only, State::new());
    harness.run();
    let press_p = |h: &mut Harness<'static, State>, mods: Modifiers| {
        h.input_mut().events.push(Event::ModifiersChanged(mods));
        h.input_mut().events.push(Event::Key {
            key: Key::P,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: mods,
        });
        h.run();
    };
    let mode = |h: &Harness<'static, State>| h.state().cam.projection;
    assert_eq!(mode(&harness), crate::viewport::ProjectionMode::Perspective);
    press_p(&mut harness, Modifiers::NONE);
    assert_eq!(mode(&harness), crate::viewport::ProjectionMode::Orthographic, "P to ortho");
    press_p(&mut harness, Modifiers::NONE);
    assert_eq!(mode(&harness), crate::viewport::ProjectionMode::Perspective, "P back to perspective");
    assert!(harness.state().pending.is_empty(), "the flip armed a command");
    // Ctrl+P stays on the post lane: the key splits, the flip stays out.
    press_p(&mut harness, Modifiers::COMMAND);
    assert_eq!(mode(&harness), crate::viewport::ProjectionMode::Perspective, "Ctrl+P must not flip");
    assert!(
        harness.state().pending.iter().any(|c| matches!(c, CommandKind::PostProgramDialog)),
        "Ctrl+P lost its post: {:?}",
        harness.state().pending
    );
}

/// Save As has its own ribbon command, so it can ask for a fresh filename
/// even after an ordinary Save has established the current project path.
#[test]
fn save_as_ribbon_cell_queues_its_own_dialog() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame_undrained, ShellUi::seeded());
    let (cells, _, _, _) = panes::ribbon_cells(
        egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280.0, super::shell::RIBBON_H)),
        &theme::current(),
        &panes::ribbon_plan(camcraft_app::machine::Kind::Mill, &[]),
    );
    let save_as = cells.iter().find(|cell| cell.cmd == Cmd::SaveAs).expect("Save As ribbon cell");
    let at = save_as.rect.center();
    set_pos(&mut harness, at);
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    let pending = &harness.state().state.pending;
    assert_eq!(pending.len(), 1, "Save As queued {pending:?}");
    assert!(matches!(pending.front(), Some(CommandKind::SaveProjectAsDialog)));
}

/// The painted ribbon cell is a click on the actions law: `ribbon_cells`
/// (the single layout source) locates the Pocket cell, a real pointer
/// press/release there must run `actions::add_operation`, and the shell's
/// next-frame drain must land the operation in the world. Proves the ribbon
/// fires the shared layer, not a forked poke - the seam M-a left open.
#[test]
fn ribbon_click_creates_through_the_actions_law() {
    let mut harness = shell_harness();
    // The bar carries Create cells only through pins (the ribbon law:
    // unpinned kinds live behind their section door), so the
    // actions-law proof rides a pinned Pocket - pin the kind through
    // the app's own seam and click the cell the live ribbon draws.
    camcraft_app::ribbon::pinned_set(&harness.state().state.world, &[2]);
    harness.run();
    let (cells, _, _, _) = panes::ribbon_cells(
        egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280.0, super::shell::RIBBON_H)),
        &theme::current(),
        &panes::ribbon_plan(camcraft_app::machine::Kind::Mill, &[2]),
    );
    let pocket = cells
        .iter()
        .find(|c| c.cmd == Cmd::Create(2))
        .expect("the pinned Pocket stands on the bar");
    let at = pocket.rect.center();
    set_pos(&mut harness, at);
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    let st = &harness.state().state;
    assert!(
        model::find_by_name(&st.world, "2D Pocket1").is_some(),
        "the Pocket click did not create through the actions layer (status: {})",
        st.status
    );
    assert!(
        matches!(&st.ui_target, UiTarget::Operation(n) if n == "2D Pocket1"),
        "quick-add must leave its target on the new operation"
    );
}

/// The quick-add naming law (M9.1): a press names its operation after the
/// kind's own label, and the number runs past the TAKEN names, not the
/// count - a document holding "2D Pocket1" and "2D Pocket3" must land the
/// next Pocket on "2D Pocket2", so no press ever looks dead. The names may
/// carry spaces ("2D Pocket1"): the label is quoted verbatim, exactly what
/// the ribbon cell and the retype menu row say.
#[test]
fn quick_add_names_past_taken_names_with_the_kind_label() {
    let mut harness = shell_harness();
    {
        let sh = harness.state_mut();
        for name in ["2D Pocket1", "2D Pocket3"] {
            sh.state.pending.push_back(CommandKind::CreateOperation {
                name: name.into(),
                tool: "EM-0600-4FL".into(),
                src: SrcRef::Stock,
                params: crate::operations::quick_params(2),
            });
        }
    }
    harness.run_steps(2);
    crate::actions::add_operation(&mut harness.state_mut().state, 2);
    harness.run_steps(2);
    let st = &harness.state().state;
    assert!(
        model::find_by_name(&st.world, "2D Pocket2").is_some(),
        "the hole should fill, not widen (status: {})",
        st.status
    );
    assert!(
        matches!(&st.ui_target, UiTarget::Operation(n) if n == "2D Pocket2"),
        "the press click-throughs to the op it just named: {:?}",
        st.ui_target
    );
}

// ------------------------------------------------------------ M6.2: sections

fn mill_plan_at(w: f32) -> (Vec<panes::Cell>, Vec<panes::Drop>) {
    mill_plan_pinned_at(w, &[])
}

/// The same layout, over a pinned list - the plan is a pure function
/// of (machine, pins), so this is what the live ribbon draws.
fn mill_plan_pinned_at(w: f32, pinned: &[usize]) -> (Vec<panes::Cell>, Vec<panes::Drop>) {
    let strip = egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(w, super::shell::RIBBON_H));
    let (cells, _, _, drops) = panes::ribbon_cells(
        strip,
        &theme::current(),
        &panes::ribbon_plan(camcraft_app::machine::Kind::Mill, pinned),
    );
    (cells, drops)
}

/// The document's operation rows, counted (the dropdown test proves
/// reach by exactly one more, whatever names they take).
fn op_count(state: &State) -> usize {
    model::named_entities(&state.world)
        .into_iter()
        .filter(|(_, e)| model::is_operation(&state.world, e))
        .count()
}

fn create_at(cmds: &[panes::Cell]) -> Vec<usize> {
    cmds.iter()
        .filter_map(|c| match c.cmd {
            Cmd::Create(i) => Some(i),
            _ => None,
        })
        .collect()
}

/// The reach law (M6.2): whatever the machine, every kind it can run
/// has exactly one home this frame - inline in its section, or behind
/// that section's door - and a kind it cannot run has none (the
/// hiding law; a dark cell would be a lie with a new taxonomy).
#[test]
fn every_kind_has_a_home_on_every_machine() {
    use camcraft_app::machine::Kind;
    for machine in [Kind::Mill, Kind::Lathe, Kind::MillTurn] {
        for w in [1280.0, 900.0] {
            let strip =
                egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(w, super::shell::RIBBON_H));
            let pal = theme::current();
            let plan = panes::ribbon_plan(machine, &[]);
            let (cells, _, _, drops) = panes::ribbon_cells(strip, &pal, &plan);
            let inline = create_at(&cells);
            let behind: Vec<usize> = drops
                .iter()
                .flat_map(|d| d.rows.iter())
                .filter_map(|(c, _)| match c {
                    Cmd::Create(i) => Some(*i),
                    _ => None,
                })
                .collect();
            for (i, k) in camcraft_app::operation::KindInfo::ALL.iter().enumerate() {
                let homes = inline.iter().filter(|j| **j == i).count()
                    + behind.iter().filter(|j| **j == i).count();
                // The command door's own word (M13.5 B): the lathe's
                // Drill cell has its home behind the Drilling door.
                let want = usize::from(k.admits(machine));
                assert_eq!(
                    homes, want,
                    "kind {i} ({}) has {homes} homes on a {} at {w}px, wants {want}",
                    k.label,
                    machine.word()
                );
            }
        }
    }
}

/// The fold is structural now (the ribbon law, retiring M6.2's width
/// law): whatever the window's width, a section's kinds live behind
/// their door - no unpinned Create cell stands on the bar, every
/// section the machine can run answers with its button, and no door
/// fronts an empty list. The user asked for exactly this: a crowd
/// that only folds when squeezed is still a crowd.
#[test]
fn every_section_lives_behind_its_door_at_every_width() {
    use camcraft_app::operation::Section;
    for w in [2400.0, 1280.0, 900.0] {
        let (cells, doors) = mill_plan_at(w);
        assert!(
            create_at(&cells).is_empty(),
            "unpinned kinds never stand inline (at {w}px)"
        );
        assert_eq!(
            doors.iter().map(|d| d.section).collect::<Vec<_>>(),
            vec![Section::Mill2D, Section::Mill3D, Section::Drilling, Section::MultiAxis],
            "a mill document shows its four sections as doors (at {w}px)"
        );
        for d in &doors {
            assert!(!d.rows.is_empty(), "a door must front a real list");
            assert!(
                cells.iter().any(|c| c.cmd == Cmd::More(d.section)),
                "each door's button must be on the ribbon"
            );
        }
    }
}

/// The dropdown is not scenery: pressing a folded door opens it (the
/// state names the section), and pressing a row creates exactly that
/// operation through the actions law - then the door closes on its
/// own way out.
#[test]
fn the_dropdown_reaches_a_folded_kind() {
    let mut harness = shell_harness();
    harness.run();
    let (_, drops) = mill_plan_at(1280.0);
    let drop = drops
        .iter()
        .find(|d| !d.rows.is_empty())
        .expect("a folded door at house width");
    let ops_before = op_count(&harness.state().state);
    let at = drop.button.center();
    set_pos(&mut harness, at);
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    assert_eq!(
        harness.state().state.ribbon_more,
        Some(drop.section),
        "pressing the door must open it"
    );
    let row_at = drop.rows[0].1.center();
    let Cmd::Create(want) = drop.rows[0].0 else {
        panic!("a door's first row must create a kind");
    };
    set_pos(&mut harness, row_at);
    button_at(&mut harness, egui::PointerButton::Primary, row_at, true);
    button_at(&mut harness, egui::PointerButton::Primary, row_at, false);
    harness.run_steps(2);
    assert_eq!(
        op_count(&harness.state().state),
        ops_before + 1,
        "the row must create its kind (status: {})",
        harness.state().state.status
    );
    // The count alone would pass even if the row created the WRONG
    // kind (a label/order drift) - the quick-add lands as
    // `UiTarget::Operation`, which reads as "a dialog asking me to
    // pick it again" if it does. Name it and type it: M9.1 names the
    // new op after its kind's label, so the created row carries the
    // registry's own word for `want`, not a generic "Operation-n".
    let st = &harness.state().state;
    let label = camcraft_app::operation::KindInfo::ALL[want].label;
    let created = model::named_entities(&st.world)
        .into_iter()
        .find(|(n, _)| n.starts_with(label))
        .unwrap_or_else(|| panic!("the click created a {label}-named entity"));
    assert_eq!(
        model::operation_of(&st.world, &created.1)
            .expect("created entity is an operation")
            .params
            .kind()
            .ordinal(),
        want,
        "the row must create exactly the kind it was labeled with"
    );
    assert_eq!(
        harness.state().state.ribbon_more, None,
        "a choice closes the door"
    );
}

/// The gated-row law (the ribbon's never-silent half): a fresh
/// document holds no tool, so no operation can start - but a click
/// on a dark row must still ANSWER: the refusal named on the status
/// line, pointing at the fix, and the door closing on the way out.
/// The silent dead pixel this replaces is what made the folded
/// ribbon read as broken. `Shell::new`'s blank startup is the
/// document this describes.
#[test]
fn a_gated_dropdown_row_click_answers_and_closes() {
    let mut harness = Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::blank());
    harness.run();
    let (_, drops) = mill_plan_at(1280.0);
    let drop = drops
        .iter()
        .find(|d| !d.rows.is_empty())
        .expect("a folded door at house width");
    let at = drop.button.center();
    set_pos(&mut harness, at);
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    assert_eq!(
        harness.state().state.ribbon_more,
        Some(drop.section),
        "the door opens over a blank document too"
    );
    let row_at = drop.rows[0].1.center();
    set_pos(&mut harness, row_at);
    button_at(&mut harness, egui::PointerButton::Primary, row_at, true);
    button_at(&mut harness, egui::PointerButton::Primary, row_at, false);
    harness.run_steps(2);
    let st = &harness.state().state;
    assert!(
        st.status.contains("no tool") && st.status.contains("Tools"),
        "a gated click must name the reason and the fix (status: {})",
        st.status
    );
    assert_eq!(op_count(st), 0, "a gated click creates nothing");
    assert_eq!(st.ribbon_more, None, "the answered click closes the door");
}

/// The pin gesture, at the level its law lives on (M6.2): the
/// right-click changes the document's pinned list and says so. The
/// pins are saved with the project, so the pin is a command on the
/// history (issue #143): the first undo takes the pin back, the second
/// takes the cut back, and redo re-applies them in order.
#[test]
fn right_click_pins_and_undo_takes_the_pin_back() {
    use camcraft_app::operation::Section;
    let mut harness = shell_harness();
    harness.run();
    let (_cells, drops) = mill_plan_at(1280.0);
    let door = drops
        .iter()
        .find(|d| d.section == Section::Mill2D)
        .expect("the 2-D section stands behind its door");
    let pocket_row = door
        .rows
        .iter()
        .find(|(c, _)| *c == Cmd::Create(2))
        .map(|(_, r)| *r)
        .expect("Pocket stands behind the 2-D door");
    let at = door.button.center();
    // Cut first, through the door, so there is history to replay
    // under the pin (the ribbon law's route to an unpinned kind).
    set_pos(&mut harness, at);
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    let at = pocket_row.center();
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    assert!(
        model::find_by_name(&harness.state().state.world, "2D Pocket1").is_some(),
        "the door's row must have cut through the actions law (status: {})",
        harness.state().state.status
    );
    // Open the door again and right-click the row: pinned. The pin
    // gesture answers at the row as it does at a cell - the folded
    // half of the ribbon carries the whole law.
    let at = door.button.center();
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    let at = pocket_row.center();
    button_at(&mut harness, egui::PointerButton::Secondary, at, true);
    button_at(&mut harness, egui::PointerButton::Secondary, at, false);
    harness.run_steps(1);
    assert_eq!(
        camcraft_app::ribbon::pinned_of(&harness.state().state.world),
        vec![2],
        "the right-click must pin"
    );
    assert!(
        harness.state().state.status.contains("pinned"),
        "the toggle must say so: {}",
        harness.state().state.status
    );
    // Undo: the pin is the newest change, so it goes first and the cut
    // stays (issue #143 - a pin is a command, not a side write).
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    assert!(
        camcraft_app::ribbon::pinned_of(&harness.state().state.world).is_empty(),
        "the first undo takes the pin back"
    );
    assert!(
        model::find_by_name(&harness.state().state.world, "2D Pocket1").is_some(),
        "the first undo leaves the cut"
    );
    // The second undo takes the cut back; redo twice restores both.
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    assert!(
        model::find_by_name(&harness.state().state.world, "2D Pocket1").is_none(),
        "the second undo takes the operation back"
    );
    harness.state_mut().state.pending.push_back(CommandKind::Redo);
    harness.state_mut().state.pending.push_back(CommandKind::Redo);
    harness.run();
    assert!(
        model::find_by_name(&harness.state().state.world, "2D Pocket1").is_some(),
        "redo re-cuts the pocket"
    );
    assert_eq!(
        camcraft_app::ribbon::pinned_of(&harness.state().state.world),
        vec![2],
        "redo re-pins it"
    );
    // Right-click again - now at the inline cell the pin created (one
    // home: the bar, not the door too): unpinned, the door takes the
    // kind back.
    let (pinned_cells, _) = mill_plan_pinned_at(1280.0, &[2]);
    let at = pinned_cells
        .iter()
        .find(|c| c.cmd == Cmd::Create(2))
        .expect("the pinned Pocket stands on the bar")
        .rect
        .center();
    button_at(&mut harness, egui::PointerButton::Secondary, at, true);
    button_at(&mut harness, egui::PointerButton::Secondary, at, false);
    harness.run_steps(1);
    assert!(camcraft_app::ribbon::pinned_of(&harness.state().state.world).is_empty());
}

/// The plan side of the pin law: pins lead the Create groups in pin
/// order, the bar's op budget is MAX_PINS deep, and a pin has exactly
/// one home - the kind leaves its section's door as it steps onto the
/// bar (one home, the ribbon law's corollary). A pin to work the
/// machine cannot run is kept in the list and hidden on screen
/// (survive-not-shown).
#[test]
fn the_pinned_group_leads_and_respects_the_machine() {
    use camcraft_app::machine::Kind;
    let plan = panes::ribbon_plan(Kind::Mill, &[2, 5]);
    assert_eq!(plan[2].title, "Pinned");
    assert_eq!(
        plan[2].cells,
        vec![Cmd::Create(2), Cmd::Create(5)],
        "pin order is ribbon order"
    );
    assert!(
        plan[3..].iter().all(|g| !g.cells.contains(&Cmd::Create(2))),
        "the pinned Pocket has one home: the bar, not its door too"
    );
    let orphan = panes::ribbon_plan(Kind::Mill, &[28]);
    assert!(
        !orphan.iter().any(|g| g.title == "Pinned"),
        "a lathe pin on a mill document is hidden, not darkened"
    );
    let both = panes::ribbon_plan(Kind::MillTurn, &[28]);
    assert_eq!(both[2].cells, vec![Cmd::Create(28)]);
    // The budget (the ribbon law): a list longer than the bar's op
    // budget renders the first MAX_PINS admitted pins inline.
    let five = panes::ribbon_plan(Kind::MillTurn, &[0, 1, 2, 3, 5]);
    assert_eq!(
        five[2].cells,
        vec![
            Cmd::Create(0),
            Cmd::Create(1),
            Cmd::Create(2),
            Cmd::Create(3)
        ],
        "the bar keeps four pins - the fifth waits behind its door"
    );
    assert!(
        five[3..].iter().all(|g| ![0usize, 1, 2, 3]
            .iter()
            .any(|i| g.cells.contains(&Cmd::Create(*i)))),
        "a kind standing on the bar is not also behind a door - \
         and kind 5, over the budget, has its one home behind its door"
    );
    assert!(
        five[3..].iter().any(|g| g.cells.contains(&Cmd::Create(5))),
        "the fifth pin waits behind its section door, reachable"
    );
}

/// The stale door self-heals, in the fold law's new shape: width can
/// no longer retire a door (they stand at every size), but the
/// machine can - mount a lathe and the mill's doors leave the ribbon,
/// so an open one must not outlive its button. The next frame finds
/// no button and drops the state itself.
#[test]
fn changing_the_machine_heals_a_stale_door() {
    use camcraft_app::machine::Kind;
    let mut harness = shell_harness();
    harness.run();
    let (_, drops) = mill_plan_at(1280.0);
    let section = drops
        .iter()
        .find(|d| !d.rows.is_empty())
        .expect("a section door on the mill ribbon")
        .section;
    harness.state_mut().state.ribbon_more = Some(section);
    harness.run();
    assert_eq!(
        harness.state().state.ribbon_more,
        Some(section),
        "still real while the document mills"
    );
    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::LoadBuiltinMachine("Lathe".to_owned()));
    harness.run();
    assert_eq!(
        camcraft_app::machine::kind_of(&harness.state().state.world),
        Kind::Lathe,
        "the mount must have landed (status: {})",
        harness.state().state.status
    );
    assert_eq!(
        harness.state().state.ribbon_more, None,
        "the mill's door left the ribbon, so the open state must be gone too"
    );
}

/// The machine door, end to end (the ribbon law): the door stands on
/// the Setup group, wears the running workspace as its caption, and
/// opens the list of all three. The current mode is inert - pressing
/// it closes the list and mounts nothing - and pressing another
/// switches the document through the command queue, so undo switches
/// back exactly. The operations follow the mount: a mill kind goes
/// dark on the lathe and lives again after the undo (the family gate
/// `a_lathe_machine_darkens_the_mill_cells` proves for the file
/// picker, proven here for the door the user asked for).
#[test]
fn the_machine_door_switches_workspaces_and_rides_undo() {
    use camcraft_app::machine::Kind;
    let mut harness = shell_harness();
    harness.run();
    let (cells, _) = mill_plan_at(1280.0);
    let button = cells
        .iter()
        .find(|c| c.cmd == Cmd::Machine)
        .expect("the machine door stands on the bar")
        .rect;
    let rows = panes::machine_rows(button, theme::current().row_height, 200.0);
    assert_eq!(rows.len(), 4, "Milling, Turning, Mill-turning, Plasma cutting");
    // Open the door.
    let at = button.center();
    set_pos(&mut harness, at);
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    assert!(
        harness.state().state.ribbon_mode_open,
        "pressing the door must open the list"
    );
    // The first row is the mode already running: a choice, but a
    // silent one - the list closes and no machine mounts.
    let before = harness.state().state.history.can_undo();
    let at = rows[0].1.center();
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    assert!(
        !harness.state().state.ribbon_mode_open && harness.state().state.history.can_undo() == before,
        "the current row closes without mounting (status: {})",
        harness.state().state.status
    );
    assert_eq!(
        camcraft_app::machine::kind_of(&harness.state().state.world),
        Kind::Mill,
        "the mill keeps running"
    );
    // Reopen and switch to Turning: the queue carries the mount, the
    // world follows, and the status names the workspace.
    let at = button.center();
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    let at = rows[1].1.center();
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    assert_eq!(
        camcraft_app::machine::kind_of(&harness.state().state.world),
        Kind::Lathe,
        "the Turning row must mount the library lathe (status: {})",
        harness.state().state.status
    );
    assert!(
        // The mode gesture's own line ("machine mode: Turning") rides
        // the queue; the mount's landing message then speaks with the
        // machine's own voice - which is the truth the status line owes.
        harness.state().state.status.contains("Lathe"),
        "the switch must say what mounted: {}",
        harness.state().state.status
    );
    assert!(!harness.state().state.ribbon_mode_open, "a choice closes the list");
    // The operations follow the workspace (the user's ask): the mill
    // kinds go dark behind the doors...
    assert!(
        !panes::enabled(Cmd::Create(1), &harness.state().state),
        "a mill operation must be dark on a lathe document"
    );
    // ...and Ctrl+Z's law holds: undo is the switch back.
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run_steps(2);
    assert_eq!(
        camcraft_app::machine::kind_of(&harness.state().state.world),
        Kind::Mill,
        "undo must restore the mill (status: {})",
        harness.state().state.status
    );
    assert!(
        panes::enabled(Cmd::Create(1), &harness.state().state),
        "and the mill kinds live again"
    );
}

/// The tip table, pinned row-for-row. The strings were imgui's verbatim
/// until M6.2 restructured the ribbon (sections, the pin gesture, the
/// overflow doors); that day the verbatim promise retired (the promise,
/// not the guard - a tip is still a promise). Every advertised key is
/// live: `shortcuts` implements the keys, and the right-click names
/// itself because the cell answers to it.
#[test]
fn tips_match_the_imgui_table() {
    let with_tool = ShellUi::seeded().state;
    let empty = State::new();
    use camcraft_app::operation::Section;
    let pin = "\tRight-click to pin/unpin";
    let rows: &[(Cmd, &str)] = &[
        (Cmd::New, "New document\tCtrl+N"),
        (Cmd::Model, "Open a mesh model\tCtrl+O"),
        (Cmd::Project, "Open a project\tCtrl+Shift+O"),
        (Cmd::Save, "Save the project\tCtrl+S"),
        (Cmd::SaveAs, "Save the project under a new name\tCtrl+Shift+S"),
        (Cmd::Post, "Post NC program (validation gate), then review it\tCtrl+P"),
        (Cmd::Setups, "Setups: create, activate and delete the document's setups"),
        (Cmd::Stock, "Stock definition"),
        (Cmd::Origin, "Work offset (WCS)"),
        (Cmd::Tools, "Tool library"),
        (
            Cmd::Machine,
            "Switch the workspace: Milling, Turning, Mill-turning or Plasma cutting (mounts the kind's built-in machine, undoable)",
        ),
        (Cmd::Undo, "Undo\tCtrl+Z"),
        (Cmd::Redo, "Redo\tCtrl+Y"),
        (Cmd::Run, "Generate the selected operation"),
        (Cmd::Delete, "Delete the selection\tDel"),
        (Cmd::Frame, "Frame all\tF"),
        (Cmd::Paths, "Show every cached toolpath"),
        (Cmd::Simulate, "Simulate tool moves (transport bar)"),
        (Cmd::Deviate, "Judge the program's stock against the model (deviation map)"),
        (
            Cmd::View,
            "Show or hide the browser, properties and machines panes, or reset the layout",
        ),
        (Cmd::Theme, "Pick the document's theme and colour-vision mode (undoable)"),
    ];
    for (c, want) in rows {
        assert_eq!(&panes::tip_of(*c, &with_tool), want, "tip drifted for {c:?}");
    }
    // The Create rows spell themselves from the registry (M6.2: thirty
    // kinds, no hand-typed eight-cell block left), each plus the pin
    // gesture the cell answers to - and on this mill document, the
    // turning half says so and names the fix, the pair the command
    // door enforces (the defense-in-depth branch of `enabled`).
    for (i, t) in panes::TYPES.iter().enumerate() {
        let is_turn = camcraft_app::operation::KindInfo::ALL[i].family()
            == camcraft_app::operation::Family::Turn;
        let tool_law = camcraft_app::operation::tool_gate::create_refusal(
            &with_tool.world,
            &camcraft_app::operation::KindInfo::ALL[i],
        );
        let is_plasma = camcraft_app::operation::KindInfo::ALL[i].family()
            == camcraft_app::operation::Family::Plasma;
        let want = if is_turn {
            "The document's machine is a mill - turning operations do not drive it; mount a lathe or a mill-turn (Setup > Machine)".to_owned()
        } else if is_plasma {
            // #670: the torch's work has no mill-turn to fall back on.
            "The document's machine is a mill - plasma cutting operations do not drive it; mount a plasma table (Setup > Machine)".to_owned()
        } else if let Some(reason) = tool_law {
            // M10.1 B: the demo's one flat end mill cannot cut every
            // mill kind - those cells say which tool kind is missing.
            reason
        } else if i == 0 {
            // imgui's own spelling of the first kind survives verbatim
            // (a Face mill, not a "Face operation") - the one string
            // the registry label did not replace.
            format!("Add a Face mill operation{pin}")
        } else {
            format!("Add a {t} operation{pin}")
        };
        assert_eq!(panes::tip_of(Cmd::Create(i), &with_tool), want, "tip drifted for Create({i})");
    }
    // The doors name their section and what stands behind it.
    for s in Section::ORDER {
        assert_eq!(
            panes::tip_of(Cmd::More(s), &with_tool),
            format!("The rest of the {} kinds", s.label()),
            "door tip drifted for {s:?}"
        );
    }
    // Every Create button shares the swap while the tool list is empty.
    for i in 0..panes::TYPES.len() {
        assert_eq!(
            panes::tip_of(Cmd::Create(i), &empty),
            "No tool yet - add one (Setup > Tools)",
            "the no-tool tip must name the fix (Create({i}))"
        );
    }
}

// ------------------------------------------------------------- M1.1: units

/// The Units radio on the setup page is the whole lens control: it rides
/// `SetUnits` through the queue (a command, not a field poke), undo puts
/// the millimeter default back, and a document never changes because of
/// it - the world's numbers stay exactly as they were.
#[test]
fn units_radio_flips_the_lens_and_undo_puts_it_back() {
    let mut harness = shell_harness();
    harness.state_mut().state.ui_target = UiTarget::Stock;
    harness.run();
    harness.run();
    assert_eq!(harness.state().state.units(), Unit::Millimeters);

    harness.get_by_label("Inches").click();
    harness.run();
    assert_eq!(
        units::of(&harness.state().state.world),
        Unit::Inches,
        "the click must reach the world through SetUnits"
    );

    // A second click on the already-shown choice queues nothing: a no-op
    // does not belong in the history (SetUnits validates at construction).
    let before = harness.state().state.history.can_redo();
    harness.get_by_label("Inches").click();
    harness.run();
    assert_eq!(harness.state().state.history.can_redo(), before, "no-op must be inert");

    // Ctrl+Z back to millimeters - same keyboard lane as every command.
    harness
        .input_mut()
        .events
        .push(Event::ModifiersChanged(Modifiers::COMMAND));
    harness.input_mut().events.push(Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    harness.run_steps(2);
    assert_eq!(
        units::of(&harness.state().state.world),
        Unit::Millimeters,
        "undo restores the lens"
    );
}

/// The inch lens in action, through the real editor window: the DragValue
/// moves per dragged pixel the SAME millimeter distance it moves in mm
/// mode (speed ÷ 25.4 - a lens, not a gearbox), a typed `0.5` stores
/// exactly 12.7 on the micron lattice, and Apply lands snapped numbers
/// through `ChangeTool` like always.
#[test]
fn inch_mode_drag_keeps_px_parity_and_types_onto_the_lattice() {
    let mut harness = shell_harness();
    harness.state_mut().state.ui_target = UiTarget::Stock;
    harness.run();
    harness.get_by_label("Inches").click();
    harness.run();
    assert_eq!(harness.state().state.units(), Unit::Inches);

    harness.state_mut().state.ui_target = UiTarget::Tool("EM-0600-4FL".to_owned());
    harness.run();
    harness.get_by_label("Edit").click();
    harness.run();
    harness.run();
    let world_d = |h: &Harness<'static, ShellUi>| {
        model::find_by_name(&h.state().state.world, "EM-0600-4FL")
            .and_then(|e| model::tool_of(&h.state().state.world, &e))
            .map(|t| t.diameter)
    };
    let d0 = world_d(&harness).expect("the seed owns EM-0600-4FL");

    let c = harness
        .get_all_by_role(egui::accesskit::Role::SpinButton)
        .next()
        .expect("the editor window owns DragValues")
        .rect()
        .center();
    harness.drag_at(c);
    harness.run();
    harness.hover_at(c + Vec2::new(30.0, 0.0));
    harness.run();
    harness.drop_at(c + Vec2::new(30.0, 0.0));
    harness.run();
    // The same 0.05 tolerance the mm drag test carries, and the same
    // reason: egui's smart aim rounds a drag toward the nicest number of
    // the SHOWN unit inside a one-physical-pixel radius, which by the
    // speed-÷25.4 law is the identical ±0.1 mm in both modes (here it
    // pulled 9.000 to 0.355 in = 9.017 mm, 0.017 off).
    assert!(
        (harness.state().state.tool_ui.diameter - (d0 + 3.0)).abs() < 0.05,
        "inch px-parity: 30 px must move 3.0 mm from {d0}, got {}",
        harness.state().state.tool_ui.diameter
    );
    assert!(
        (world_d(&harness).expect("still there") - d0).abs() < 1e-12,
        "a drag must not touch the world"
    );

    harness
        .get_all_by_role(egui::accesskit::Role::SpinButton)
        .next()
        .unwrap()
        .click();
    harness.run();
    harness
        .get_all_by_role(egui::accesskit::Role::SpinButton)
        .next()
        .unwrap()
        .type_text("0.5");
    harness.run();
    assert_eq!(
        harness.state().state.tool_ui.diameter,
        12.7,
        "a typed half inch must store exactly 12.7 mm"
    );
    harness.get_by_label("Apply").click();
    harness.run();
    assert_eq!(
        world_d(&harness).expect("still there"),
        12.7,
        "Apply must land the snapped millimeters (status: {})",
        harness.state().state.status
    );
}

// --------------------------------------------- view cube and nav bar (M1.5)
//
// The furniture is painted, not widdgitised (the icon law's vector path), so
// these tests aim raw pointer events at rects the hud module recomputes from
// the laid-out pane - the same arithmetic the shell's own routing uses.

/// The furniture over the laid-out pane - `shell_layout`'s call, reproduced.
fn hud(sh: &ShellUi) -> viewcube::Hud {
    let ([x0, y0], [x1, y1]) = sh.state.viewport_rect;
    viewcube::Hud::new(egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1)))
        .expect("the 1280-wide dock leaves the viewport room for the furniture")
}

fn face_center(h: &mut Harness<'static, ShellUi>, q: [f64; 3]) -> egui::Pos2 {
    let sh = h.state();
    viewcube::point_screen(&sh.state.cam, &hud(sh), q)
}

/// The cube's core claim: clicking a face's center turns the camera to that
/// standard view, immediately and with no command - and because the press is
/// never armed, the release picks nothing behind the cube.
#[test]
fn view_cube_face_click_orients_and_arms_nothing() {
    let mut harness = shell_harness();
    harness.run();
    let status0 = harness.state().state.status.clone();
    let (target0, dist0) = (
        harness.state().state.cam.target,
        harness.state().state.cam.dist,
    );
    let p = face_center(&mut harness, [0.0, 0.0, 1.0]);
    set_pos(&mut harness, p);
    button_at(&mut harness, egui::PointerButton::Primary, p, true);
    {
        let sh = harness.state();
        assert!(
            (sh.state.cam.pitch - std::f64::consts::FRAC_PI_2).abs() < 1e-9,
            "the TOP face must stand the camera over the part, pitch {}",
            sh.state.cam.pitch
        );
        assert_eq!((sh.state.cam.target, sh.state.cam.dist), (target0, dist0));
        assert!(sh.left_press.is_none(), "the furniture must never arm a press");
        assert!(sh.state.drag.is_none());
        assert!(sh.state.pending.is_empty(), "orientation queues nothing");
    }
    button_at(&mut harness, egui::PointerButton::Primary, p, false);
    harness.run_steps(2);
    let sh = harness.state();
    assert_eq!(sh.state.status, status0, "a cube click is not a pick");
    assert!(sh.left_press.is_none());
}

/// A press on the cube that is then dragged: oriented at the press, and the
/// motion must not promote to orbit - the gesture stays absorbed end to end.
#[test]
fn a_drag_started_on_the_cube_is_absorbed() {
    let mut harness = shell_harness();
    harness.run();
    let status0 = harness.state().state.status.clone();
    let p = face_center(&mut harness, [0.0, 0.0, 1.0]);
    set_pos(&mut harness, p);
    button_at(&mut harness, egui::PointerButton::Primary, p, true);
    let yaw0 = harness.state().state.cam.yaw;
    let d = p + egui::vec2(40.0, 0.0);
    set_pos(&mut harness, d);
    {
        let sh = harness.state();
        assert!(sh.state.drag.is_none(), "the cube must not promote a held press");
        assert_eq!(sh.state.cam.yaw, yaw0, "moving after a cube press must not orbit");
    }
    button_at(&mut harness, egui::PointerButton::Primary, d, false);
    harness.run_steps(2);
    let sh = harness.state();
    assert_eq!(sh.state.status, status0, "the release behind the cube must not pick");
    assert!(sh.state.pending.is_empty());
}

/// The nav column's five buttons: the exact house iso (after an orbit moved
/// it off), the two toggles flipping their fields twice, and Fit riding the
/// same queued `FrameAll` the F key pushes.
#[test]
fn the_nav_bar_drives_view_state() {
    let mut harness = shell_harness();
    harness.run();
    let p = {
        let h = hud(harness.state());
        h.nav_rect(viewcube::Nav::Frame).center()
    };

    // Fit rides the same queued `FrameAll` as the F key - proven through
    // the drain, like every other command test: asserting a command on
    // its press frame is egui-pass fragile (a later pass's drain can
    // execute it before we look), while the executed effect is the law.
    // Park the camera far off the framing, click, and it must be back.
    let framed = {
        let sh = harness.state();
        (sh.state.cam.target, sh.state.cam.dist)
    };
    harness.state_mut().state.cam.target = [400.0, 400.0, 400.0];
    harness.state_mut().state.cam.dist = 999.0;
    click_viewport(&mut harness, p);
    {
        let sh = harness.state();
        assert_eq!(
            (sh.state.cam.target, sh.state.cam.dist),
            framed,
            "Fit must run the same reframe the F key does"
        );
        assert!(
            !sh.state
                .pending
                .iter()
                .any(|c| matches!(c, CommandKind::FrameAll)),
            "the queued command outlived the drain"
        );
    }

    // Orbit away, then let Iso bring the house angle back exactly.
    let c = vp_center(harness.state());
    set_pos(&mut harness, c);
    button_at(&mut harness, egui::PointerButton::Primary, c, true);
    set_pos(&mut harness, c + egui::vec2(30.0, 0.0));
    button_at(&mut harness, egui::PointerButton::Primary, c + egui::vec2(30.0, 0.0), false);
    harness.run_steps(2);
    assert_ne!(harness.state().state.cam.yaw, 0.9, "the orbit must move yaw");
    let p = {
        let h = hud(harness.state());
        h.nav_rect(viewcube::Nav::Iso).center()
    };
    click_viewport(&mut harness, p);
    {
        let sh = harness.state();
        assert_eq!(sh.state.cam.yaw, 0.9, "Iso is the house yaw, exactly");
        assert_eq!(sh.state.cam.pitch, 0.5);
    }

    // Wireframe: the same field the Solids radios own, flipped twice.
    let p = {
        let h = hud(harness.state());
        h.nav_rect(viewcube::Nav::Wire).center()
    };
    click_viewport(&mut harness, p);
    assert_eq!(harness.state().state.display, crate::state::DisplayMode::Wireframe);
    click_viewport(&mut harness, p);
    assert_eq!(
        harness.state().state.display,
        crate::state::DisplayMode::ShadedEdges
    );

    // Paths and Axes flip their booleans, twice each, queueing nothing -
    // counting from their defaults (State::new: axes on, paths off).
    for (kind, want_paths, want_axes) in [
        (viewcube::Nav::Paths, true, true),
        (viewcube::Nav::Paths, false, true),
        (viewcube::Nav::Axes, false, false),
        (viewcube::Nav::Axes, false, true),
    ] {
        let p = {
            let h = hud(harness.state());
            h.nav_rect(kind).center()
        };
        click_viewport(&mut harness, p);
        let sh = harness.state();
        assert_eq!(sh.state.show_all_paths, want_paths);
        assert_eq!(sh.state.show_world_axes, want_axes);
        assert!(sh.state.pending.is_empty(), "{kind:?} queued a command");
    }
}

/// The furniture never interferes with the pane's own gestures: the center
/// click still picks (the seeded Block is under it), and the cube sits where
/// it cannot (top-right, away from the block's projection).
#[test]
fn the_cube_leaves_the_viewport_gestures_alone() {
    let mut harness = shell_harness();
    let c = vp_center(harness.state());
    let h = hud(harness.state());
    assert!(!h.contains(c), "the block under the pane center is not furniture");
    click_viewport(&mut harness, c);
    assert_eq!(
        harness.state().state.status,
        "selected Block",
        "the ordinary pick law must survive the cube"
    );
}

// ---------------------------------------------------------- M1.6: drilling

/// The 40x40x8 block bored through by a 16-facet bore - the app's own
/// recognition fixture, imported here so the UI law runs against the
/// same pixels a STEP file hands us.
fn bored_block() -> camcraft_app::geometry::Mesh {
    use camcraft_app::geometry::{Point3, Triangle};
    let tau = std::f64::consts::TAU;
    let bp = |i: usize, z: f64| {
        let a = tau * (i % 16) as f64 / 16.0;
        Point3::new(12.0 * a.cos(), 12.0 * a.sin(), z)
    };
    // Bore vertices pushed radially onto the 40x40 square: the diagonal
    // ones carry a corner, so each side's wall quads share its plane.
    let op = |i: usize, z: f64| {
        let a = tau * (i % 16) as f64 / 16.0;
        let (x, y) = (a.cos(), a.sin());
        let s = x.abs().max(y.abs());
        Point3::new(x * 20.0 / s, y * 20.0 / s, z)
    };
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, facets: &mut Vec<Triangle>| {
        facets.push(Triangle::new(a, b, c));
        facets.push(Triangle::new(a, c, d));
    };
    let mut facets = Vec::new();
    for i in 0..16 {
        let j = i + 1;
        // Caps: annuli wound outward (+z top, -z bottom).
        quad(bp(j, 8.0), bp(i, 8.0), op(i, 8.0), op(j, 8.0), &mut facets);
        quad(bp(i, 0.0), bp(j, 0.0), op(j, 0.0), op(i, 0.0), &mut facets);
        // Bore wall faces the axis; the block wall faces away.
        quad(bp(i, 8.0), bp(j, 8.0), bp(j, 0.0), bp(i, 0.0), &mut facets);
        quad(op(j, 8.0), op(i, 8.0), op(i, 0.0), op(j, 0.0), &mut facets);
    }
    camcraft_app::geometry::Mesh::from_facets(facets)
}

#[test]
fn ribbon_drill_cell_creates_a_drill_operation() {
    let mut harness = shell_harness();
    let kind = camcraft_app::operation::OpType::Drill as usize;
    // The Drill cell stands on the bar only as a pin (the ribbon law:
    // unpinned kinds live behind their section door, reached through
    // `the_dropdown_reaches_a_folded_kind`) - pin the Drill's kind and
    // the default-parameter proof rides the live cell.
    camcraft_app::ribbon::pinned_set(&harness.state().state.world, &[kind]);
    rack(&mut harness.state_mut().state, &["DR-0400"]);
    harness.run();
    let (cells, _, _, _) = panes::ribbon_cells(
        egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280.0, super::shell::RIBBON_H)),
        &theme::current(),
        &panes::ribbon_plan(camcraft_app::machine::Kind::Mill, &[kind]),
    );
    let drill = cells
        .iter()
        .find(|c| c.cmd == Cmd::Create(kind))
        .expect("the pinned Drill stands on the bar");
    let at = drill.rect.center();
    set_pos(&mut harness, at);
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    let st = &harness.state().state;
    let op = model::find_by_name(&st.world, "Drill1")
        .as_ref()
        .and_then(|e| model::operation_of(&st.world, e))
        .expect("the Drill cell created its operation through the actions law");
    // Empty holes at creation is legal (the page shows the list's
    // placeholder); the generator is what refuses, at Generate time.
    let OpParams::Drill { holes, cycle, .. } = &op.params else {
        panic!("the Drill cell must create a Drill operation, got {:?}", op.params.kind())
    };
    assert!(holes.is_empty(), "a fresh drill starts without a hole list");
    assert_eq!(*cycle, camcraft_app::drilling::CycleChoice::Auto);
}

/// M10.1 B/D, the ribbon half of the tool law: the seeded demo racks
/// one flat end mill, so the Drill cell DARKENS - it stays on the bar
/// (darken, not hide: the fix is one library click away), its tip says
/// which tool kind the setup lacks, and a click anyway creates nothing
/// and says why. Racking a drill lights it again.
#[test]
fn a_create_cell_darkens_with_the_tool_laws_reason() {
    let mut harness = shell_harness();
    let kind = camcraft_app::operation::OpType::Drill as usize;
    camcraft_app::ribbon::pinned_set(&harness.state().state.world, &[kind]);
    harness.run();
    let label = camcraft_app::operation::KindInfo::ALL[kind].label;
    let reason = format!(
        "No drill, tap or boring bar in the setup's tools - a {label} operation cuts with one; add it to the tool library - add 'DR-0600' (6 mm drill) from the catalog?"
    );
    {
        let st = &harness.state().state;
        assert!(!st.tool_names().is_empty(), "the darkening is the tool law, not an empty rack");
        assert!(!panes::enabled(Cmd::Create(kind), st), "no fitting tool: the cell is dark");
        assert_eq!(panes::tip_of(Cmd::Create(kind), st), reason, "the tip names the missing kind");
    }
    let (cells, _, _, _) = panes::ribbon_cells(
        egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280.0, super::shell::RIBBON_H)),
        &theme::current(),
        &panes::ribbon_plan(camcraft_app::machine::Kind::Mill, &[kind]),
    );
    let drill = cells
        .iter()
        .find(|c| c.cmd == Cmd::Create(kind))
        .expect("darkened, not hidden: the Drill cell still stands on the bar");
    let at = drill.rect.center();
    set_pos(&mut harness, at);
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    {
        let st = &harness.state().state;
        assert!(model::find_by_name(&st.world, "Drill1").is_none(), "a dark cell creates nothing");
        assert!(st.status.contains(&reason), "the click answers with the reason: {}", st.status);
        // #191: and takes the user to the fix - the tool library.
        assert_eq!(st.ui_target, UiTarget::Tool(String::new()), "the click opens the tool library");
        assert!(st.status.ends_with("opened the tool library"), "{}", st.status);
    }
    rack(&mut harness.state_mut().state, &["DR-0400"]);
    harness.run();
    let st = &harness.state().state;
    assert!(panes::enabled(Cmd::Create(kind), st), "a racked drill lights the cell");
    assert_eq!(
        panes::tip_of(Cmd::Create(kind), st),
        format!("Add a {label} operation\tRight-click to pin/unpin")
    );
}

/// M3.2 stage E, the workspaces-by-kind law: the ribbon follows the
/// machine file's axis inventory. A lathe document darkens every mill
/// cell - honestly disabled with a tip that says why and when - and a
/// mill file lights them again. The UI never invents the kind; it reads
/// `machine::kind_of`, which reads the loaded `.mch`.
#[test]
fn a_lathe_machine_darkens_the_mill_cells() {
    let dir = std::env::temp_dir().join(format!("camcraft_kind_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let write = |name: &str, axes: &str| {
        let path = dir.join(name);
        std::fs::write(
            &path,
            format!(
                r#"{{"general":{{"model":"TestBench","vendor":"CamCraft Testbench"}},
                    "kinematics":{{"default":{{"units":{{"length":"mm"}},"parts":[
                      {axes}
                    ]}}}}}}"#
            ),
        )
        .expect("write machine file");
        path
    };
    let lathe = write(
        "lathe.mch",
        r#"{"type":"linear","name":"X","parts":[{"type":"linear","name":"Z"}]}"#,
    );
    let mill = write(
        "mill.mch",
        r#"{"type":"linear","name":"X","parts":[{"type":"linear","name":"Y","parts":[{"type":"linear","name":"Z"}]}]}"#,
    );

    let mut harness = shell_harness();
    harness.run();
    assert!(
        !harness.state().state.tool_names().is_empty(),
        "the demo document carries tools - the darkening is the machine, not the list"
    );

    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::LoadMachineFile(lathe));
    harness.run();
    let st = &harness.state().state;
    assert!(
        !panes::enabled(Cmd::Create(1), st),
        "a lathe machine must darken the mill cells"
    );
    assert!(
        !panes::enabled(Cmd::Create(0), st),
        "every Create cell, not just contour"
    );
    assert!(
        panes::tip_of(Cmd::Create(1), st).contains("lathe"),
        "the tip says what is dark and why: {:?}",
        panes::tip_of(Cmd::Create(1), st)
    );

    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::LoadMachineFile(mill));
    harness.run();
    let st = &harness.state().state;
    assert!(
        panes::enabled(Cmd::Create(1), st),
        "a Y axis in the file is the mill workspace again"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// M5.2 stage A, the same table read from the Turn side: the demo
/// document carries tools, so the darkening is the machine - and a
/// mounted lathe LIGHTS the turning cell, its tip introducing the
/// operation instead of refusing it, while the mill cells go dark.
/// The ribbon and the command door read the one registry, so the
/// ordinal here is the enum's, not a remembered number.
#[test]
fn a_lathe_document_lights_the_turning_cell() {
    let mut harness = shell_harness();
    rack(&mut harness.state_mut().state, LATHE_RACK);
    harness.run();
    let turn = camcraft_app::operation::OpType::Facing as usize;
    assert!(
        !panes::enabled(Cmd::Create(turn), &harness.state().state),
        "an unmachined document is a mill document: the turning cell is dark"
    );
    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::LoadBuiltinMachine("Lathe".to_string()));
    harness.run();
    let st = &harness.state().state;
    assert!(
        panes::enabled(Cmd::Create(turn), st),
        "the mounted lathe lights the ladder"
    );
    // Stage B's kinds ride the same table row - the family field,
    // not a remembered list: the two profile cells light with it,
    // stage C's three blade cells ride the very same row (M5.2 C),
    // stage D's threading cell arrives on it too (M5.2 D), so do
    // stage E's three contour rides (M5.2 E), so does stage E2's
    // form gauge cell (M5.2 E2), and so does stage F's boring bar
    // cell (M5.2 F).
    for word in [
        camcraft_app::operation::OpType::ProfileRough as usize,
        camcraft_app::operation::OpType::ProfileFinish as usize,
        camcraft_app::operation::OpType::GrooveRough as usize,
        camcraft_app::operation::OpType::GrooveFinish as usize,
        camcraft_app::operation::OpType::Part as usize,
        camcraft_app::operation::OpType::Threading as usize,
        camcraft_app::operation::OpType::Chamfering as usize,
        camcraft_app::operation::OpType::TaperTurning as usize,
        camcraft_app::operation::OpType::MultiPassTurning as usize,
        camcraft_app::operation::OpType::FormTurning as usize,
        camcraft_app::operation::OpType::BoringTurning as usize,
    ] {
        assert!(
            panes::enabled(Cmd::Create(word), st),
            "the mounted lathe lights the {} cell too",
            panes::TYPES[word]
        );
    }
    assert_eq!(
        panes::tip_of(Cmd::Create(turn), st),
        format!("Add a {} operation\tRight-click to pin/unpin", panes::TYPES[turn]),
        "a lit turning cell offers the op, it does not refuse it"
    );
    assert!(
        !panes::enabled(Cmd::Create(0), st),
        "and the same mount shuts the mill cells: since M6.2 the plan hides \
         them entirely, and this is the command door's law that stays false \
         for whatever surface still renders one"
    );
    // (the undo law at this level: one command, both doors - put the
    // mount back through the history and the ribbon follows.)
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    assert!(
        !panes::enabled(Cmd::Create(turn), &harness.state().state),
        "undoing the mount shuts the turning door again"
    );
}

/// M3.3 stage C, the Setup pane's machine group headless: the built-in
/// library mounts by name, and the editor's Apply is a `LoadMachine`
/// at the mount's own path - so undo puts back the exact numbers the
/// library mounted, and a stranger name is refused by name.
#[test]
fn the_machine_group_mounts_the_library_and_the_editor_writes_through_history() {
    let mut harness = shell_harness();
    harness.run();
    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::LoadBuiltinMachine("Lathe".to_string()));
    harness.run();
    let (path, mounted) = camcraft_app::machine::of(&harness.state().state.world);
    assert_eq!(path, "built-in: Lathe", "the mount records through the shared built-in path");
    let mut mounted = mounted.expect("the library lathe is mounted");
    assert_eq!(
        camcraft_app::machine::kind(Some(&mounted)),
        camcraft_app::machine::Kind::Lathe,
        "the declared kind drives the workspaces, not a guess"
    );
    // A declared lathe darkens the mill cells just like the inferred
    // one the test above judges - one law, two roads.
    assert!(!panes::enabled(Cmd::Create(1), &harness.state().state));

    let spindle_max = mounted.spindle_max;
    // The editor's Apply, fed the draft the dialog would have shown
    // with one number changed.
    mounted.spindle_max = Some(4321.0);
    harness.state_mut().state.pending.push_back(CommandKind::EditMachineApply {
        path,
        machine: Box::new(mounted),
    });
    harness.run();
    let (_, m) = camcraft_app::machine::of(&harness.state().state.world);
    assert_eq!(
        m.expect("edited").spindle_max,
        Some(4321.0),
        "Apply reached the world through the history"
    );

    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    let (_, m) = camcraft_app::machine::of(&harness.state().state.world);
    assert_eq!(
        m.expect("mounted").spindle_max,
        spindle_max,
        "undo restores the library's exact numbers"
    );

    // A name the library does not hold is refused by name, and the
    // document stands exactly where it stood.
    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::LoadBuiltinMachine("Drill press".to_string()));
    harness.run();
    assert!(
        harness.state().state.status.contains("Drill press"),
        "the refusal names the stranger: {:?}",
        harness.state().state.status
    );
    let (_, m) = camcraft_app::machine::of(&harness.state().state.world);
    assert_eq!(m.expect("still mounted").spindle_max, spindle_max);
}

#[test]
fn ribbon_pocket3d_cell_creates_a_3d_pocket_operation() {
    let mut harness = shell_harness();
    let kind = camcraft_app::operation::OpType::Pocket3D as usize;
    // Pinned, so the cell stands on the bar (the ribbon law) - the
    // default-parameter proof rides the live cell.
    camcraft_app::ribbon::pinned_set(&harness.state().state.world, &[kind]);
    harness.run();
    let (cells, _, _, _) = panes::ribbon_cells(
        egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280.0, super::shell::RIBBON_H)),
        &theme::current(),
        &panes::ribbon_plan(camcraft_app::machine::Kind::Mill, &[kind]),
    );
    let pocket3d = cells
        .iter()
        .find(|c| c.cmd == Cmd::Create(kind))
        .expect("the pinned 3D Pocket stands on the bar");
    let at = pocket3d.rect.center();
    set_pos(&mut harness, at);
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    let st = &harness.state().state;
    let op = model::find_by_name(&st.world, "3D Pocket1")
        .as_ref()
        .and_then(|e| model::operation_of(&st.world, e))
        .expect("the 3D Pocket cell created its operation through the actions law");
    // The defaults state the level law with: a helix ladder (M11.3) from
    // the stock top, Auto strategy - no geometry pick field exists for this
    // kind because the region is the model's silhouette.
    let OpParams::Pocket3D { strategy, entry, .. } = &op.params else {
        panic!(
            "the 3D Pocket cell must create a 3D Pocket operation, got {:?}",
            op.params.kind()
        )
    };
    assert_eq!(*strategy, camcraft_app::machining2d::PocketStrategy::Auto);
    // M11.3: new pocket-family ops helix in, at 0.4 x the tool radius.
    assert_eq!(*entry, camcraft_app::machining3d::Entry3D::Helix);
    let OpParams::Pocket3D { helix_radius, .. } = &op.params else { unreachable!() };
    let tool_r = model::find_by_name(&st.world, &op.tool)
        .and_then(|e| model::tool_of(&st.world, &e))
        .map(|t| t.diameter * 0.5)
        .expect("the op's tool");
    assert!((helix_radius - ((0.4 * tool_r) * 100.0).round() / 100.0).abs() < 1e-12, "{helix_radius} vs tool r {tool_r}");
}

#[test]
fn ribbon_finish3d_cell_creates_a_3d_finish_operation() {
    let mut harness = shell_harness();
    let kind = camcraft_app::operation::OpType::Finish3D as usize;
    // Pinned, so the cell stands on the bar (the ribbon law) - the
    // default-parameter proof rides the live cell.
    camcraft_app::ribbon::pinned_set(&harness.state().state.world, &[kind]);
    rack(&mut harness.state_mut().state, &["BN-0600"]);
    harness.run();
    let (cells, _, _, _) = panes::ribbon_cells(
        egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280.0, super::shell::RIBBON_H)),
        &theme::current(),
        &panes::ribbon_plan(camcraft_app::machine::Kind::Mill, &[kind]),
    );
    let finish3d = cells
        .iter()
        .find(|c| c.cmd == Cmd::Create(kind))
        .expect("the pinned 3D Finish stands on the bar");
    let at = finish3d.rect.center();
    set_pos(&mut harness, at);
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    let st = &harness.state().state;
    let op = model::find_by_name(&st.world, "3D Finish1")
        .as_ref()
        .and_then(|e| model::operation_of(&st.world, e))
        .expect("the 3D Finish cell created its operation through the actions law");
    // The tolerance is the op's one shaping number (the stepdown law
    // derives from it and the tool), and no hop is on by default - the
    // pocket family's clearance law, quoted by the draft's own default.
    // The pattern defaults to the auto-split, never plain Waterline:
    // waterline alone leaves every flat shelf standing, and a silent
    // miss is the one answer this app never gives.
    let OpParams::Finish3D {
        pattern,
        stepover,
        tolerance,
        retract_z,
        ..
    } = &op.params
    else {
        panic!(
            "the 3D Finish cell must create a 3D Finish operation, got {:?}",
            op.params.kind()
        )
    };
    assert_eq!(*tolerance, 0.05);
    assert_eq!(*retract_z, None);
    assert_eq!(
        *pattern,
        camcraft_app::finishing3d::FinishPattern::Parallel,
        "the fresh finish op must ride flats and walls alike"
    );
    assert_eq!(*stepover, None, "spacing follows the cos-law until asked not to");
}

/// The M2.2b fields obey the draft law exactly as every other pair of
/// fields does: a staged change to pattern and stepover reaches the
/// world only through Apply, only as an EditOperationParams, and undo
/// brings the auto-split default back. (The combo and the checkbox are
/// custom-drawn and unreachable to the harness's click law - staging
/// the draft by hand is this pane's stand-in, the same route
/// `click_apply_edits_operation_through_commands` documents.)
#[test]
fn finish3d_pattern_and_stepover_apply_through_commands() {
    let mut harness = shell_harness();
    rack(&mut harness.state_mut().state, &["BN-0600"]);
    crate::actions::add_operation(&mut harness.state_mut().state, camcraft_app::operation::OpType::Finish3D as usize);
    harness.run_steps(2);
    let staged = |h: &Harness<'static, ShellUi>| match h.state().state.op_ui.params {
        OpParams::Finish3D { pattern, stepover, .. } => (pattern, stepover),
        _ => panic!("the pane must draft a 3D Finish op"),
    };
    let (p0, s0) = staged(&harness);
    assert_eq!(p0, camcraft_app::finishing3d::FinishPattern::Parallel);
    assert_eq!(s0, None);
    {
        let sh = harness.state_mut();
        sh.state.op_tab = 2; // Passes - the ramp checkbox below lives there
        sh.state.sync_ops_ui();
        if let OpParams::Finish3D { pattern, stepover, .. } = &mut sh.state.op_ui.params {
            *pattern = camcraft_app::finishing3d::FinishPattern::Waterline;
            *stepover = Some(1.5);
        }
    }
    harness.run();
    {
        let st = &harness.state().state;
        assert!(
            !st.pending
                .iter()
                .any(|c| matches!(c, CommandKind::EditOperationParams { .. })),
            "staging must not queue - only Apply speaks"
        );
    }
    harness.get_by_label("Apply").scroll_to_me();
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    let st = &harness.state().state;
    let op = model::find_by_name(&st.world, "3D Finish1")
        .and_then(|e| model::operation_of(&st.world, &e));
    let live = op.map(|o| match o.params {
        OpParams::Finish3D { pattern, stepover, .. } => (pattern, stepover),
        _ => panic!("kind drift"),
    });
    assert_eq!(
        live,
        Some((
            camcraft_app::finishing3d::FinishPattern::Waterline,
            Some(1.5),
        )),
        "Apply must carry pattern and stepover to the world"
    );
    // Undo through the keyboard lane - the same pair the history-key
    // test injects; there is no Undo button in the pane to click.
    harness
        .input_mut()
        .events
        .push(Event::ModifiersChanged(Modifiers::COMMAND));
    harness.input_mut().events.push(Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    harness.run_steps(2);
    let st = &harness.state().state;
    let back = model::find_by_name(&st.world, "3D Finish1")
        .and_then(|e| model::operation_of(&st.world, &e))
        .map(|o| match o.params {
            OpParams::Finish3D { pattern, stepover, .. } => (pattern, stepover),
            _ => panic!("kind drift"),
        });
    assert_eq!(
        back,
        Some((camcraft_app::finishing3d::FinishPattern::Parallel, None)),
        "undo must restore the defaults the cell seeded"
    );
    // M2.2c: the Scallop word rides the same command lane. Stage it,
    // apply, and the world reads the crossed diagonals; undo returns
    // to the auto-split default.
    {
        let sh = harness.state_mut();
        sh.state.sync_ops_ui();
        if let OpParams::Finish3D { pattern, .. } = &mut sh.state.op_ui.params {
            *pattern = camcraft_app::finishing3d::FinishPattern::Scallop;
        }
    }
    harness.run();
    harness.get_by_label("Apply").scroll_to_me();
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    let st = &harness.state().state;
    let op = model::find_by_name(&st.world, "3D Finish1")
        .and_then(|e| model::operation_of(&st.world, &e))
        .map(|o| match o.params {
            OpParams::Finish3D { pattern, stepover, .. } => (pattern, stepover),
            _ => panic!("kind drift"),
        });
    assert_eq!(
        op,
        Some((camcraft_app::finishing3d::FinishPattern::Scallop, None)),
        "Apply must carry the scallop word to the world"
    );
    harness
        .input_mut()
        .events
        .push(Event::ModifiersChanged(Modifiers::COMMAND));
    harness.input_mut().events.push(Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    harness.run_steps(2);
    let st = &harness.state().state;
    let back = model::find_by_name(&st.world, "3D Finish1")
        .and_then(|e| model::operation_of(&st.world, &e))
        .map(|o| match o.params {
            OpParams::Finish3D { pattern, stepover, .. } => (pattern, stepover),
            _ => panic!("kind drift"),
        });
    assert_eq!(
        back,
        Some((camcraft_app::finishing3d::FinishPattern::Parallel, None)),
        "undo must invert the scallop apply like any other"
    );
    // M2.2e stage D: the ramp checkbox rides the same command lane, on
    // the same tab (the draft is back on Parallel, so the widget is
    // shown - Waterline would hide it). Stage by click, apply, undo.
    harness.state_mut().state.sync_ops_ui();
    harness.run();
    harness.get_by_label("Ramp between lines").scroll_to_me();
    harness.run();
    harness.get_by_label("Ramp between lines").click();
    harness.run();
    harness.get_by_label("Apply").scroll_to_me();
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    let st = &harness.state().state;
    let ramped = model::find_by_name(&st.world, "3D Finish1")
        .and_then(|e| model::operation_of(&st.world, &e))
        .map(|o| match o.params {
            OpParams::Finish3D { ramp, .. } => ramp,
            _ => panic!("kind drift"),
        });
    assert_eq!(ramped, Some(true), "Apply must carry the ramp checkbox to the world");
    harness
        .input_mut()
        .events
        .push(Event::ModifiersChanged(Modifiers::COMMAND));
    harness.input_mut().events.push(Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    harness.run_steps(2);
    let st = &harness.state().state;
    let back = model::find_by_name(&st.world, "3D Finish1")
        .and_then(|e| model::operation_of(&st.world, &e))
        .map(|o| match o.params {
            OpParams::Finish3D { ramp, .. } => ramp,
            _ => panic!("kind drift"),
        });
    assert_eq!(back, Some(false), "undo restores the seeded no-ramp default");
    // M2.2e stage E: the refinement floor rides the same lane, and
    // like the stepover override it is a checkbox that manufactures
    // its own number - the click alone stages Some(scalar), so the
    // Apply after a bare click carries the seeded floor, and undo
    // returns the march to its own 1/4096 right.
    harness.state_mut().state.sync_ops_ui();
    harness.run();
    // The Passes tab's Auto toggles in row order: Stepover, then
    // Minimum step - unticking the second states the floor.
    harness.get_all_by_label("Auto").nth(1).expect("Minimum step's Auto").scroll_to_me();
    harness.run();
    harness.get_all_by_label("Auto").nth(1).expect("Minimum step's Auto").click();
    harness.run();
    let staged = match &harness.state().state.op_ui.params {
        OpParams::Finish3D { min_step, .. } => *min_step,
        _ => panic!("kind drift"),
    };
    assert!(staged.is_some(), "the floor checkbox staged no number");
    harness.get_by_label("Apply").scroll_to_me();
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    let st = &harness.state().state;
    let floored = model::find_by_name(&st.world, "3D Finish1")
        .and_then(|e| model::operation_of(&st.world, &e))
        .map(|o| match o.params {
            OpParams::Finish3D { min_step, .. } => min_step.is_some(),
            _ => panic!("kind drift"),
        });
    assert_eq!(floored, Some(true), "Apply must carry the floor to the world");
    harness
        .input_mut()
        .events
        .push(Event::ModifiersChanged(Modifiers::COMMAND));
    harness.input_mut().events.push(Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    harness.run_steps(2);
    let st = &harness.state().state;
    let back = model::find_by_name(&st.world, "3D Finish1")
        .and_then(|e| model::operation_of(&st.world, &e))
        .map(|o| match o.params {
            OpParams::Finish3D { min_step, .. } => min_step,
            _ => panic!("kind drift"),
        });
    assert_eq!(back, Some(None), "undo restores the march's own floor");
}

#[test]
fn detect_holes_refuses_recognizes_and_undo_empties_again() {
    let mut harness = shell_harness();
    rack(&mut harness.state_mut().state, &["DR-0400"]);
    crate::actions::add_operation(&mut harness.state_mut().state, camcraft_app::operation::OpType::Drill as usize);
    harness.run_steps(2);
    // The pane now targets the new Drill op; the Geometry group is the
    // Detect button's home, and the draft reads the created params.
    {
        let sh = harness.state_mut();
        sh.state.sync_ops_ui();
    }
    harness.run();

    // 1. Refusal, honest and scoped: the op still stands on the stock,
    // and a stock's box walls are not holes.
    harness.get_by_label("Detect holes").click();
    harness.run_steps(2);
    {
        let st = &harness.state().state;
        assert!(st.pending.is_empty(), "a refused detection must queue nothing");
        assert!(
            st.error_for(crate::state::ErrorScope::Operation)
                .is_some_and(|m| m.contains("no drilled holes")),
            "the refusal must name the fix, got: {}",
            st.status
        );
    }

    // 2. Aim the op at a plate that does have a bore, and click again.
    // The import goes straight through history (the mesh-import command
    // has no queue arm - the file dialog's worker executes it this way
    // too); the retarget rides the queue like any pane edit.
    {
        let sh = harness.state_mut();
        sh.state.pending.push_back(CommandKind::EditOperationLinks {
            name: "Drill1".into(),
            tool: None,
            src: Some(SrcRef::Entity("Plate".into())),
        });
        let (world, history) = (&sh.state.world, &mut sh.state.history);
        history.execute(
            world,
            camcraft_app::commands::ImportModel::from_mesh("Plate", bored_block()),
        );
    }
    harness.run_steps(2);
    harness.get_by_label("Detect holes").click();
    harness.run_steps(2);
    let op_params = {
        let st = &harness.state().state;
        model::find_by_name(&st.world, "Drill1")
            .as_ref()
            .and_then(|e| model::operation_of(&st.world, e))
            .expect("Drill1 survives")
            .params
            .clone()
    };
    let OpParams::Drill { holes, .. } = &op_params else { unreachable!() };
    assert_eq!(holes.len(), 1, "the bore is the plate's only hole");
    assert!(holes[0].through && (holes[0].radius - 12.0).abs() < 1e-6);
    // The draft mirrors what the world now holds - rows show, no lie.
    assert_eq!(harness.state().state.op_ui.params, op_params);

    // 3. Undo retires the detection alone - holes were a command.
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run_steps(2);
    let st = &harness.state().state;
    let op = model::find_by_name(&st.world, "Drill1")
        .as_ref()
        .and_then(|e| model::operation_of(&st.world, e))
        .unwrap();
    let OpParams::Drill { holes, .. } = &op.params else { unreachable!() };
    assert!(holes.is_empty(), "undo must empty the detected list again");
}

/// The M5.2 F2 port of the same law to the turret's side of the
/// document: on a mill-turn machine, Detect holes reads RADIAL
/// walls - the turned solid's cross-bores - into the `cross` list,
/// keeps the Z list empty, and rides history like any detection.
/// On a plain mill the same mesh detects nothing (the radial door
/// stays shut) - the machine, not the model, decides what a live
/// tool can ask for.
#[test]
fn radial_detect_fills_the_cross_list_on_a_millturn() {
    // The app cross suite's bar, ported verbatim - the document
    // slices it and recognizes it by the same laws the app test
    // proves green, so the mesh here is that mesh, not a paraphrase
    // of it. Closures stand in for the app test's free functions;
    // every coordinate and winding is theirs.
    use camcraft_app::geometry::{Point3, Triangle};
    let tau = std::f64::consts::TAU;
    let sp = |along: usize, u: usize, v: usize, s: f64, pu: f64, pv: f64| {
        let mut c = [0.0; 3];
        c[along] = s;
        c[u] = pu;
        c[v] = pv;
        Point3::new(c[0], c[1], c[2])
    };
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    let circle = |cu: f64, cv: f64, r: f64, n: usize| -> Vec<(f64, f64)> {
        (0..n)
            .map(|k| {
                let a = tau * (k as f64) / (n as f64);
                (cu + r * a.cos(), cv + r * a.sin())
            })
            .collect()
    };
    let circle_cw = |cu: f64, cv: f64, r: f64, n: usize| -> Vec<(f64, f64)> {
        let mut c = circle(cu, cv, r, n);
        c.reverse();
        c
    };
    let wall_ring = |along: usize,
                     u: usize,
                     v: usize,
                     poly: &[(f64, f64)],
                     s0: f64,
                     s1: f64,
                     f: &mut Vec<Triangle>| {
        let (s0, s1) = if matches!((u, v, along), (0, 1, 2) | (1, 2, 0) | (2, 0, 1)) {
            (s0, s1)
        } else {
            (s1, s0)
        };
        let n = poly.len();
        for i in 0..n {
            let a = sp(along, u, v, s0, poly[i].0, poly[i].1);
            let b = sp(along, u, v, s0, poly[(i + 1) % n].0, poly[(i + 1) % n].1);
            let c = sp(along, u, v, s1, poly[(i + 1) % n].0, poly[(i + 1) % n].1);
            let d = sp(along, u, v, s1, poly[i].0, poly[i].1);
            f.push(Triangle::new(a, b, c));
            f.push(Triangle::new(a, c, d));
        }
    };
    let disc = |along: usize,
                u: usize,
                v: usize,
                cu: f64,
                cv: f64,
                r: f64,
                n: usize,
                s: f64,
                f: &mut Vec<Triangle>| {
        let p = circle(cu, cv, r, n);
        let ctr = sp(along, u, v, s, cu, cv);
        for i in 0..n {
            f.push(Triangle::new(
                ctr,
                sp(along, u, v, s, p[i].0, p[i].1),
                sp(along, u, v, s, p[(i + 1) % n].0, p[(i + 1) % n].1),
            ));
        }
    };
    let annulus = |along: usize,
                   u: usize,
                   v: usize,
                   cu: f64,
                   cv: f64,
                   hu: f64,
                   hv: f64,
                   r: f64,
                   n: usize,
                   s: f64,
                   f: &mut Vec<Triangle>| {
        let q = |k: usize| -> (f64, f64) {
            let a = tau * (k as f64) / (n as f64);
            let (co, si) = (a.cos(), a.sin());
            let t = (co.abs() / hu).max(si.abs() / hv);
            (cu + co / t, cv + si / t)
        };
        let c = |k: usize| -> (f64, f64) {
            let a = tau * (k as f64) / (n as f64);
            (cu + r * a.cos(), cv + r * a.sin())
        };
        for k in 0..n {
            let qa = sp(along, u, v, s, q(k).0, q(k).1);
            let qb = sp(along, u, v, s, q(k + 1).0, q(k + 1).1);
            let ca = sp(along, u, v, s, c(k).0, c(k).1);
            let cb = sp(along, u, v, s, c(k + 1).0, c(k + 1).1);
            f.push(Triangle::new(qa, ca, cb));
            f.push(Triangle::new(qa, cb, qb));
        }
    };
    let p = Point3::new;
    let mut facets = Vec::new();
    // The four prismatic faces.
    quad(p(-15.0, 5.0, -15.0), p(-15.0, 5.0, 15.0), p(15.0, 5.0, 15.0), p(15.0, 5.0, -15.0), &mut facets);
    quad(p(-15.0, -5.0, -15.0), p(15.0, -5.0, -15.0), p(15.0, -5.0, 15.0), p(-15.0, -5.0, 15.0), &mut facets);
    quad(p(-15.0, -5.0, 15.0), p(15.0, -5.0, 15.0), p(15.0, 5.0, 15.0), p(-15.0, 5.0, 15.0), &mut facets);
    quad(p(-15.0, -5.0, -15.0), p(-15.0, 5.0, -15.0), p(15.0, 5.0, -15.0), p(15.0, -5.0, -15.0), &mut facets);
    // -X face: punched once in the through hole's band, solid
    // across everything else (the blind hole never reaches it).
    quad(p(-15.0, -5.0, -5.0), p(-15.0, -5.0, 15.0), p(-15.0, 5.0, 15.0), p(-15.0, 5.0, -5.0), &mut facets);
    annulus(0, 1, 2, 0.0, -10.0, 5.0, 5.0, 2.0, 24, -15.0, &mut facets);
    // +X face: punched where each hole meets it, solid between.
    annulus(0, 1, 2, 0.0, 10.0, 5.0, 5.0, 2.0, 24, 15.0, &mut facets);
    annulus(0, 1, 2, 0.0, -10.0, 5.0, 5.0, 2.0, 24, 15.0, &mut facets);
    quad(p(15.0, -5.0, -5.0), p(15.0, 5.0, -5.0), p(15.0, 5.0, 5.0), p(15.0, -5.0, 5.0), &mut facets);
    // The bores: blind wall + floor, through wall.
    wall_ring(0, 1, 2, &circle_cw(0.0, 10.0, 2.0, 24), 5.0, 15.0, &mut facets);
    disc(0, 1, 2, 0.0, 10.0, 2.0, 24, 5.0, &mut facets);
    wall_ring(0, 1, 2, &circle_cw(0.0, -10.0, 2.0, 24), -15.0, 15.0, &mut facets);
    let bar = camcraft_app::geometry::Mesh::from_facets(facets);

    let dir = std::env::temp_dir().join(format!("camcraft_cross_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let mt = dir.join("mt.mch");
    std::fs::write(
        &mt,
        r#"{"general":{"model":"TurretBench","kind":"mill-turn"},"kinematics":{"default":{"units":{"length":"mm"},"parts":[]}}}"#,
    )
    .expect("write machine file");

    let mut harness = shell_harness();
    rack(&mut harness.state_mut().state, &["DR-0400"]);
    crate::actions::add_operation(&mut harness.state_mut().state, camcraft_app::operation::OpType::Drill as usize);
    harness.run_steps(2);
    {
        let sh = harness.state_mut();
        sh.state.sync_ops_ui();
        sh.state.pending.push_back(CommandKind::LoadMachineFile(mt.clone()));
        let (world, history) = (&sh.state.world, &mut sh.state.history);
        history.execute(world, camcraft_app::commands::ImportModel::from_mesh("Bar", bar));
        sh.state.pending.push_back(CommandKind::EditOperationLinks {
            name: "Drill1".into(),
            tool: None,
            src: Some(SrcRef::Entity("Bar".into())),
        });
    }
    harness.run_steps(3);

    harness.get_by_label("Detect holes").click();
    harness.run_steps(2);
    let params = {
        let st = &harness.state().state;
        assert!(
            st.error_for(crate::state::ErrorScope::Operation).is_none(),
            "a turret document must not refuse its own cross-bores: {}",
            st.status
        );
        model::find_by_name(&st.world, "Drill1")
            .as_ref()
            .and_then(|e| model::operation_of(&st.world, e))
            .expect("Drill1 survives")
            .params
            .clone()
    };
    let OpParams::Drill { holes, cross, .. } = &params else { unreachable!() };
    assert!(holes.is_empty(), "a radial bar carries no Z holes");
    assert_eq!(cross.len(), 2, "the blind and the through bore: {cross:?}");
    assert!(cross.iter().filter(|h| h.through).count() == 1, "exactly one through");
    assert_eq!(harness.state().state.op_ui.params, params, "the draft shows the rows");

    // Undo retires the detection - cross holes are a command too.
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run_steps(2);
    let st = &harness.state().state;
    let op = model::find_by_name(&st.world, "Drill1")
        .as_ref()
        .and_then(|e| model::operation_of(&st.world, e))
        .unwrap();
    let OpParams::Drill { cross, .. } = &op.params else { unreachable!() };
    assert!(cross.is_empty(), "undo must empty the detected cross list again");
    std::fs::remove_dir_all(&dir).ok();
}

// ------------------------------------------- M1.7c: move simulation

/// The playhead's law, pinned pure (kittest frames advance ~0 seconds,
/// so the arithmetic must be proven directly): time moves only while
/// playing, speed rides dt, the clamp lands exactly on the end and
/// stops there - no wrap - and pathological seconds freeze the
/// playhead rather than poison it.
#[test]
fn playhead_advance_law() {
    use crate::state::Sim;
    let mut s = Sim::new();
    assert!(s.playing, "arming a simulation starts it running");
    s.advance(2.0, 10.0);
    assert_eq!(s.t, 2.0);
    s.playing = false;
    s.advance(2.0, 10.0);
    assert_eq!(s.t, 2.0, "paused time stands still");
    s.playing = true;
    s.speed = 4.0;
    s.advance(2.0, 10.0);
    assert_eq!(s.t, 10.0, "4x of 2 s must land exactly on the end");
    assert!(!s.playing, "the playhead stops at the end - it never wraps");
    s.playing = true; // even a manual re-play must not push t past total
    s.advance(1.0, 10.0);
    assert_eq!(s.t, 10.0);
    let mut n = Sim::new();
    n.advance(f64::NAN, 10.0);
    assert_eq!(n.t, 0.0, "a NaN dt freezes the playhead");
    n.advance(1.0, f64::NAN);
    assert_eq!(n.t, 0.0, "a NaN total freezes it too");
    n.speed = f64::NAN;
    n.advance(1.0, 10.0);
    assert!(n.t.is_finite(), "a poisoned speed must never move t");
}

/// The ribbon gate mirrors the post gate: a document with a generated
/// operation arms the cell, an empty one does not. The transport bar is
/// real shipping UI (Play/Pause by label, Stop disarms), and the whole
/// point is view state - arming queues no command and undo never sees
/// it.
#[test]
fn simulate_cell_arms_transport_and_stop_disarms() {
    let mut harness = shell_harness();
    harness.run();
    assert!(
        panes::enabled(Cmd::Simulate, &harness.state().state),
        "the seeded doc has a generated op - the cell must be live"
    );
    assert!(
        !panes::enabled(Cmd::Simulate, &State::new()),
        "nothing to simulate on an empty document"
    );
    let (cells, _, _, _) = panes::ribbon_cells(
        egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280.0, super::shell::RIBBON_H)),
        &theme::current(),
        &panes::ribbon_plan(camcraft_app::machine::Kind::Mill, &[]),
    );
    let cell = cells
        .iter()
        .find(|c| c.cmd == Cmd::Simulate)
        .expect("the Inspect group carries a Simulate cell");
    let at = cell.rect.center();
    set_pos(&mut harness, at);
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    {
        let st = &harness.state().state;
        assert!(st.sim.is_some(), "the cell must arm the transport");
        assert!(st.sim.as_ref().is_some_and(|s| s.playing), "armed means running");
        assert!(panes::toggled(Cmd::Simulate, st));
        // The command law: nothing was queued and nothing can be undone.
        assert!(st.pending.is_empty());
    }
    harness.get_by_label("Pause").click();
    harness.run_steps(2);
    assert!(
        harness.state().state.sim.as_ref().is_some_and(|s| !s.playing),
        "the transport's Pause must reach the playhead"
    );

    // Play at the end rewinds - no separate back-to-start control.
    {
        let sim = harness.state_mut().state.sim.as_mut().unwrap();
        sim.t = 1.0e9; // parked past the finish, as a finished run is
    }
    harness.run_steps(1); // let the slider/advance settle the parked value
    harness.get_by_label("Play").click();
    harness.run_steps(1);
    {
        let sim = harness.state().state.sim.as_ref().unwrap();
        assert!(sim.playing, "Play at the end restarts the run");
        assert!(sim.t < 1.0, "rewound playhead sits at the start, got {}", sim.t);
    }

    // Stop leaves simulation: the cell un-toggles, the bar is gone.
    harness.get_by_label("Stop").click();
    harness.run_steps(2);
    let st = &harness.state().state;
    assert!(st.sim.is_none());
    assert!(!panes::toggled(Cmd::Simulate, st));
}

/// The Deviate readout's sentence law, pinned pure (M2.3 stage B): the
/// clean program says exactly one thing; an unclean one names each
/// truth it found - count, volume, worst depth and the WHERE - and a
/// located verdict that somehow lost its location admits it with "?"
/// rather than printing a lie. The bar under the viewport is this
/// string and nothing else, so the string carries the whole verdict.
#[test]
fn deviate_readout_sentences_each_truth() {
    use camcraft_app::geometry::Point3;
    use camcraft_app::units::Unit;
    let d = |rest_cells, gouge_cells, max_rest, max_gouge, tol| {
        camcraft_app::stock_sim::Deviation {
            cells_x: 8,
            cells_y: 8,
            cell: 1.0,
            tol,
            dev: Vec::new(),
            rest_cells,
            gouge_cells,
            rest_volume: rest_cells as f64 * max_rest,
            gouge_volume: gouge_cells as f64 * max_gouge,
            max_rest,
            max_gouge,
            max_rest_at: (rest_cells > 0).then(|| Point3::new(1.25, -0.75, 18.0)),
            max_gouge_at: (gouge_cells > 0).then(|| Point3::new(-20.0, 5.5, 16.0)),
        }
    };
    let clean = crate::actions::deviate_readout(&d(0, 0, 0.0, 0.005, 0.01), Unit::Millimeters);
    assert_eq!(clean, "deviate: clean - the program meets the model everywhere");
    // A within-tolerance worst is still the clean sentence - the
    // display tolerance IS the verdict, per DEV_TOL's law.
    assert_eq!(
        crate::actions::deviate_readout(&d(3, 0, 0.004, 0.0, 0.01), Unit::Millimeters),
        clean,
        "rest thinner than tolerance must not shout"
    );
    let rest = crate::actions::deviate_readout(&d(40, 0, 2.0, 0.0, 0.01), Unit::Millimeters);
    assert!(
        rest.starts_with("deviate: REST 40 cells (80.00")
            && rest.contains("worst 2.00 mm at (1.25, -0.75, z 18.00)"),
        "the rest sentence: {rest}"
    );
    assert!(!rest.contains("GOUGE"), "no gouge, no gouge clause: {rest}");
    let both = crate::actions::deviate_readout(&d(1, 7, 0.5, 1.25, 0.01), Unit::Millimeters);
    assert!(both.contains("REST") && both.contains("GOUGE"), "two truths, one line: {both}");
    assert!(both.contains("(-20.00, 5.50, z 16.00)"), "the gouge locates too: {both}");
    // A located aggregate whose point is gone prints "?" - an honest
    // gap, never fabricated coordinates.
    let mut lost = d(0, 4, 0.0, 0.9, 0.01);
    lost.max_gouge_at = None;
    assert!(
        crate::actions::deviate_readout(&lost, Unit::Millimeters).contains("GOUGE 4 cells") && both_needs_question(&lost),
        "the missing WHERE must show as '?'"
    );
    fn both_needs_question(d: &camcraft_app::stock_sim::Deviation) -> bool {
        crate::actions::deviate_readout(d, camcraft_app::units::Unit::Millimeters).ends_with("mm?)")
    }
}

/// The same verdict told through the inch lens: every number divides by
/// 25.4 with two extra decimals (the `len` law), volumes a third more,
/// and the suffix words ride along - the `sim_readout` sibling, the
/// same rule. The mm sentences above are pinned byte-identical.
#[test]
fn deviate_readout_rides_the_units_lens() {
    use camcraft_app::geometry::Point3;
    use camcraft_app::units::Unit;
    let dev = camcraft_app::stock_sim::Deviation {
        cells_x: 8,
        cells_y: 8,
        cell: 1.0,
        tol: 0.01,
        dev: Vec::new(),
        rest_cells: 40,
        gouge_cells: 7,
        rest_volume: 80.0,
        gouge_volume: 8.75,
        max_rest: 2.0,
        max_gouge: 1.25,
        max_rest_at: Some(Point3::new(1.25, -0.75, 18.0)),
        max_gouge_at: Some(Point3::new(-20.0, 5.5, 16.0)),
    };
    let s = crate::actions::deviate_readout(&dev, Unit::Inches);
    assert!(
        s.contains("REST 40 cells (0.00488 in³, worst 0.0787 in at (0.0492, -0.0295, z 0.7087))"),
        "the rest sentence in inches: {s}"
    );
    assert!(
        s.contains("GOUGE 7 cells (0.00053 in³, worst 0.0492 in at (-0.7874, 0.2165, z 0.6299))"),
        "the gouge sentence in inches: {s}"
    );
}

/// The Deviate ribbon cell is Simulate's sibling under the same laws:
/// gate = a generated program exists; a click runs the sim over the
/// live world and stores the verdict as pure view state - no command,
/// nothing undoable - the second click disarms, and the seeded doc's
/// verdict is present (its stock leaves skin, so don't assert a clean
/// sentence here - the shape is the app tests' job).
#[test]
fn deviate_cell_stores_the_verdict_as_view_state() {
    let mut harness = shell_harness();
    harness.run();
    assert!(
        panes::enabled(Cmd::Deviate, &harness.state().state),
        "the seeded doc has a generated op - the cell must be live"
    );
    assert!(
        !panes::enabled(Cmd::Deviate, &State::new()),
        "nothing to judge on an empty document"
    );
    let (cells, _, _, _) = panes::ribbon_cells(
        egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1280.0, super::shell::RIBBON_H)),
        &theme::current(),
        &panes::ribbon_plan(camcraft_app::machine::Kind::Mill, &[]),
    );
    let cell = cells
        .iter()
        .find(|c| c.cmd == Cmd::Deviate)
        .expect("the Inspect group carries a Deviate cell");
    let at = cell.rect.center();
    set_pos(&mut harness, at);
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    {
        let st = &harness.state().state;
        let ps = st.program_sim.as_ref().expect("the cell must arm the verdict");
        assert_eq!(st.status, crate::actions::deviate_readout(&ps.dev, st.units()));
        assert!(!st.status.contains("refused"), "the seed judges: {}", st.status);
        assert!(panes::toggled(Cmd::Deviate, st));
        // The command law: judging the program writes no world state.
        assert!(st.pending.is_empty());
    }
    button_at(&mut harness, egui::PointerButton::Primary, at, true);
    button_at(&mut harness, egui::PointerButton::Primary, at, false);
    harness.run_steps(2);
    let st = &harness.state().state;
    assert!(st.program_sim.is_none(), "the second click disarms");
    assert_eq!(st.status, "deviate off");
    assert!(!panes::toggled(Cmd::Deviate, st));
}

/// The readout string is a pure law too - the bar's whole job is this
/// text, and the seeded contour pins op name, verb progression and the
/// units lens without opening a window.
#[test]
fn sim_readout_names_the_op_and_the_verb() {
    let sh = ShellUi::seeded();
    let sc = super::shell::sim_script(&sh.state);
    assert!(!sc.is_empty() && sc.total > 0.0, "the seed must produce a timed path");
    assert_eq!(sc.ops.len(), 1);
    let s = super::shell::sim_readout(&sc, 0.0, Unit::Millimeters);
    assert!(s.starts_with("Contour1 · STANDBY · "), "before the first move: {s}");
    let r = super::shell::sim_readout(&sc, sc.cum[0] * 0.5, Unit::Millimeters);
    assert!(r.starts_with("Contour1 · RAPID · "), "the contour opens on a rapid: {r}");
    let e = super::shell::sim_readout(&sc, sc.total, Unit::Millimeters);
    assert!(e.starts_with("Contour1 · END · "), "the finish says so: {e}");
    // The first cutting move names its feed in the active unit system.
    let fi = sc
        .segs
        .iter()
        .position(|g| g.secs > 0.0 && g.feed.is_some())
        .expect("a contour has feed moves");
    let start = if fi == 0 { 0.0 } else { sc.cum[fi - 1] };
    let mid = start + (sc.cum[fi] - start) * 0.5;
    let f = super::shell::sim_readout(&sc, mid, Unit::Millimeters);
    assert!(f.starts_with("Contour1 · F ") && f.contains("mm/min"), "feed verb: {f}");
    let i = super::shell::sim_readout(&sc, mid, Unit::Inches);
    assert!(i.contains("in/min"), "the verb rides the units lens: {i}");
    // An empty script has no name to show and must not pretend.
    let none = super::shell::sim_readout(
        &camcraft_app::sim::Script::default(),
        0.0,
        Unit::Millimeters,
    );
    // The empty script is spent by sim.rs law (no segments, done at once),
    // so the verb is END - the bar itself never shows it (the total==0 arm
    // prints "nothing to run"); this row pins the name, not the verb.
    assert!(none.starts_with("(none) · END · "), "degenerate: {none}");
}

#[test]
fn playback_follows_reordered_operations_instead_of_name_order() {
    let mut shell = ShellUi::seeded();
    let original = model::find_by_name(&shell.state.world, "Contour1").expect("seed operation");
    let op = model::operation_of(&shell.state.world, &original).expect("operation settings");
    shell.state.pending.push_back(CommandKind::CreateOperation {
        name: "AContour".into(), tool: op.tool,
        src: op.src, params: op.params,
    });
    shell.state.drain_actions();
    shell.state.pending.push_back(CommandKind::GenerateOperation("AContour".into()));
    shell.state.drain_actions();
    shell.state.pending.push_back(CommandKind::ReorderOperation {
        name: "Contour1".into(), target: "AContour".into(), after: false,
    });
    shell.state.drain_actions();
    assert_eq!(camcraft_app::setup::operation_order(&shell.state.world)
        .into_iter().map(|(name, _)| name).collect::<Vec<_>>(),
        ["Contour1", "AContour"]);
    let script = super::shell::sim_script(&shell.state);
    assert_eq!(script.ops, ["Contour1", "AContour"]);
}

// ------------------------------------------- M1.7d: G-code viewer

#[test]
fn gcode_segment_highlights_the_move_the_scroll_line_posts() {
    // The resolver behind the amber highlight, pure: the segment is the
    // path's own polyline arriving at the cursor line - previous stored
    // position to current; None lines rest the cursor where it was; the
    // first position collapses to its endpoint ring; a closed viewer
    // (hidden is display, M1.7c's law) highlights nothing.
    let mk = |line_pos: Vec<Option<[f64; 3]>>, line: usize, open: bool| {
        let mut st = State::new();
        st.post_view = Some(PostView {
            path: std::path::PathBuf::from("p.nc"),
            program: String::new(),
            line_pos,
            open,
            line,
            blocks: Vec::new(),
            trace: false,
            not_judged: None,
        });
        super::shell::gcode_segment(&st)
    };
    let chain = || {
        vec![
            Some([0.0, 0.0, 10.0]),
            None,
            Some([10.0, 0.0, 10.0]),
            Some([10.0, 0.0, 0.0]),
        ]
    };
    assert_eq!(mk(chain(), 0, true), Some(([0.0, 0.0, 10.0], [0.0, 0.0, 10.0])));
    assert_eq!(
        mk(chain(), 1, true),
        Some(([0.0, 0.0, 10.0], [0.0, 0.0, 10.0])),
        "a comment line rests the cursor on the last move"
    );
    assert_eq!(
        mk(chain(), 2, true),
        Some(([0.0, 0.0, 10.0], [10.0, 0.0, 10.0])),
        "the segment arriving at that line's position"
    );
    assert_eq!(
        mk(chain(), 99, true),
        Some(([10.0, 0.0, 10.0], [10.0, 0.0, 0.0])),
        "a past-the-end cursor clamps to the last move"
    );
    assert_eq!(mk(chain(), 2, false), None);
    assert_eq!(mk(vec![], 0, true), None, "header-only program: nothing to highlight");
    assert_eq!(mk(vec![None, None], 1, true), None);
    // No viewer at all (fresh document) resolves to nothing either.
    assert_eq!(super::shell::gcode_segment(&State::new()), None);
}

#[test]
fn a_successful_post_opens_the_viewer_and_new_document_retires_it() {
    // The real law end to end: the gate's own story (edit, regenerate,
    // skim post) is M1.7b's test at the head of this file - reused here
    // untouched, because what this pins is the viewer half: text and
    // positions from the same engine run, open on success, gone with
    // the document.
    let mut harness = shell_harness();
    let path = std::env::temp_dir().join(format!("camcraft-egui-view-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut skim = {
        let st = &harness.state().state;
        let e = camcraft_app::model::find_by_name(&st.world, "Contour1").expect("the seed's op");
        camcraft_app::model::operation_of(&st.world, &e)
            .expect("op")
            .params
            .clone()
    };
    if let camcraft_app::operation::OpParams::Contour { bottom_z, .. } = &mut skim {
        *bottom_z = camcraft_app::operation::Height::fixed(29.5);
    }
    harness.state_mut().state.pending.push_back(CommandKind::EditOperationParams {
        name: "Contour1".into(),
        params: skim,
    });
    harness
        .state_mut().state.pending
        .push_back(CommandKind::GenerateOperation("Contour1".into()));
    harness
        .state_mut().state.pending
        .push_back(CommandKind::PostProgramFile(path.clone()));
    harness.run();
    {
        let st = &harness.state().state;
        assert!(st.status.starts_with("posted"), "skim post: {}", st.status);
        let v = st.post_view.as_ref().expect("a successful post carries the viewer");
        assert!(v.open, "the post opens the window");
        assert_eq!(
            v.program.lines().count(),
            v.line_pos.len(),
            "one position slot per program line"
        );
        assert!(
            v.line_pos.iter().any(|p| p.is_some()),
            "a contour program has moves to highlight"
        );
        // The cursor starts at the head when the post sets it; frames
        // after the window opens publish a center row. What must hold
        // either way: it is a line the program has, never past the end.
        assert!(v.line < v.program.lines().count(), "cursor in range: {}", v.line);
    }
    let _ = std::fs::remove_file(&path);
    harness.state_mut().state.pending.push_back(CommandKind::NewDocument);
    // The document is unsaved: New waits for the prompt's answer (#107).
    harness
        .state_mut().state.pending
        .push_back(CommandKind::ResolveUnsaved(crate::unsaved::UnsavedChoice::DontSave));
    harness.run();
    assert!(
        harness.state().state.post_view.is_none(),
        "a frozen program is evidence of the old document, never the new one"
    );
}

#[test]
fn the_viewer_window_publishes_the_scroll_cursor() {
    // Paint-side law, no dialog: the window pass writes the middle row
    // back as the sync cursor, and while the window is open that cursor
    // resolves to a highlighted segment. 120 lines at ~400 px cannot
    // all show, so the published row is a real center row - never the
    // head, never past the tail.
    let mut harness = shell_harness();
    harness.state_mut().state.post_view = Some(PostView {
        path: std::path::PathBuf::from("p.nc"),
        program: (0..120)
            .map(|i| format!("G01 X{i}.0 Y0. F200."))
            .collect::<Vec<_>>()
            .join("\n"),
        line_pos: (0..120).map(|i| Some([i as f64, 0.0, 0.0])).collect(),
        open: true,
        line: 0,
        blocks: Vec::new(),
        trace: false,
        not_judged: None,
    });
    harness.run();
    {
        let st = &harness.state().state;
        let v = st.post_view.as_ref().expect("the frame keeps the viewer");
        assert!(v.line > 0 && v.line < 120, "center row published: {}", v.line);
        assert!(super::shell::gcode_segment(st).is_some(), "and it highlights");
    }
    harness.state_mut().state.post_view.as_mut().expect("still there").open = false;
    assert!(
        super::shell::gcode_segment(&harness.state().state).is_none(),
        "closing the window hides the highlight"
    );
}

/// M3.2 stage B, the tree's fixture law: a tagged model reads as work-
/// holding steel - vise glyph, "(fixture)" in the status column - while
/// keeping the row's hideability (a parked vise still gets an eye). The
/// mapping is pure so both branches are checked without a world.
#[test]
fn a_fixture_model_reads_as_a_vise_in_the_tree() {
    let (suffix, kind, icon) = panes::model_row_marks(true, false);
    assert_eq!(suffix, " (fixture)");
    assert_eq!(kind, painted::Kind::Fixture);
    assert_eq!(icon, Icon::Fixture);
    // A plain model row is untouched: the change is marker-driven,
    // never name-driven.
    let (suffix, kind, icon) = panes::model_row_marks(false, false);
    assert_eq!(suffix, "");
    assert_eq!(kind, painted::Kind::Model);
    assert_eq!(icon, Icon::Model);
    // Both markers, in column order: fixture first, then display state.
    let (suffix, _, _) = panes::model_row_marks(true, true);
    assert_eq!(suffix, " (fixture) (hidden)");
    // The eye law: hideable rows own the eye column; fixtures are
    // hideable models.
    assert!(painted::has_eye(painted::Kind::Fixture));
    assert!(painted::has_eye(painted::Kind::Model));
    assert!(painted::has_eye(painted::Kind::Stock));
    assert!(painted::has_eye(painted::Kind::Wcs));
    assert!(!painted::has_eye(painted::Kind::Section));
}

#[test]
fn stock_eye_hides_the_viewport_stock_without_removing_it_from_the_document() {
    let mut harness = shell_harness();
    harness.run_steps(2);
    let stock_name = camcraft_app::setup::stock_entity_name(&harness.state().state.world, None);
    let (_, leaves) = panes::browser_rows(&harness.state().state);
    let row = leaves.iter().position(|leaf| matches!(leaf, panes::Leaf::Stock(_))).expect("stock row");
    let eye = egui::pos2(216.0, 221.0 + (row as f32 + 0.5) * theme::current().row_height);
    assert!(world::collect(&harness.state().state).parts.iter().any(|part| part.stock));

    set_pos(&mut harness, eye);
    button_at(&mut harness, egui::PointerButton::Primary, eye, true);
    button_at(&mut harness, egui::PointerButton::Primary, eye, false);
    harness.run();
    let state = &harness.state().state;
    assert!(state.active_stock_hidden());
    assert!(model::find_by_name(&state.world, &stock_name).is_some());
    assert!(!world::collect(state).parts.iter().any(|part| part.stock));

    button_at(&mut harness, egui::PointerButton::Primary, eye, true);
    button_at(&mut harness, egui::PointerButton::Primary, eye, false);
    harness.run();
    assert!(!harness.state().state.active_stock_hidden());
    assert!(world::collect(&harness.state().state).parts.iter().any(|part| part.stock));
}

#[test]
fn selecting_a_post_script_is_a_command_with_the_pane_labels() {
    // M3.3 A's UI half, headless: the file arm lands a user script in
    // the world singleton and says so on the status line; the
    // built-in arm resets; undo puts the previous choice back; and a
    // script that does not exist is a visible refusal, never a quiet
    // keep-the-old choice.
    use camcraft_app::posting;
    use camcraft_app::schema::PostRef;

    let dir = std::env::temp_dir().join(format!("camcraft-ui-post-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let cps = dir.join("userpost.cps");
    std::fs::write(&cps, "function onOpen() { writeln(\"(UIPOST-3)\"); }").expect("write cps");

    let mut harness = shell_harness();
    harness.run();
    assert_eq!(posting::post_of(&harness.state().state.world), PostRef::Builtin);

    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::SelectPostFile(cps.clone()));
    harness.run();
    {
        let st = &harness.state().state;
        assert!(st.status.contains("post:"), "{}", st.status);
        assert!(matches!(posting::post_of(&st.world), PostRef::File(_)));
    }

    // Undo the selection - the built-in is back, by the history law.
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    assert_eq!(posting::post_of(&harness.state().state.world), PostRef::Builtin);

    // The built-in arm on an already-built-in document refuses visibly
    // (the "already selected" law), and a missing file names itself.
    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::SelectPostBuiltin);
    harness.run();
    assert!(
        harness.state().state.status.contains("post not selected"),
        "a no-op choice is a stated refusal: {}",
        harness.state().state.status
    );
    harness.state_mut().state.pending.push_back(CommandKind::SelectPostFile(
        dir.join("gone.cps"),
    ));
    harness.run();
    let st = &harness.state().state;
    assert!(st.status.contains("post not selected") && st.status.contains("gone.cps"), "{:?}", st.status);

    let _ = std::fs::remove_dir_all(&dir);
}

/// M10.1 C, the ui half of the honest after-state: delete the seed's one
/// tool out from under Contour1 and the op stays in the tree, its row's
/// status column reads BLOCKED with the app's sentence, and the open
/// property page says the same on its status line. Undo mends the world
/// and both marks retire - the ui only renders the app's predicate.
#[test]
fn a_deleted_tool_marks_the_row_and_the_page_blocked() {
    let mut harness = shell_harness();
    harness.run();
    let why = "Contour1's tool 'EM-0600-4FL' is gone; a 2D Contour operation cuts with a flat end mill or bull-nose";
    let row_suffix = |st: &State| {
        let (rows, _) = panes::browser_rows(st);
        rows.iter()
            .find(|r| r.label == "Contour1")
            .map(|r| r.suffix.clone())
            .expect("the op row stays visible")
    };
    assert!(!row_suffix(&harness.state().state).contains("BLOCKED"));
    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::DeleteByName("EM-0600-4FL".into()));
    harness.run();
    let suffix = row_suffix(&harness.state().state);
    assert!(suffix.contains("BLOCKED") && suffix.contains(why), "{suffix}");
    let page = format!("Blocked · {why}");
    assert!(harness.query_by_label_contains(&page).is_some(), "the page names the block");

    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    let suffix = row_suffix(&harness.state().state);
    assert!(!suffix.contains("BLOCKED") && suffix.contains("ok"), "{suffix}");
    assert!(harness.query_by_label_contains("Status: BLOCKED").is_none(), "undo clears the page");
}

#[test]
fn setup_navigation_is_always_reachable_and_operations_keep_a_flat_browser() {
    let plan = panes::ribbon_plan(camcraft_app::machine::Kind::Mill, &[]);
    let setup_group = plan.iter().find(|group| group.title == "Setup").expect("setup group");
    assert_eq!(
        setup_group.cells.as_slice(),
        &[Cmd::Setups, Cmd::Stock, Cmd::Origin, Cmd::Tools, Cmd::Machine],
        "the Setup toolbar opens on the Setups door (#322)"
    );

    // #322: a fresh document shows the manager too - its page is where
    // the first setup is created.
    let blank = State::new();
    let (rows, leaves) = panes::browser_rows(&blank);
    assert!(leaves.iter().any(|leaf| matches!(leaf, panes::Leaf::Setups)));
    assert!(!rows.iter().any(|row| row.label == "Global Setup"));
    assert!(rows.iter().any(|row| row.label == "Operations"));

    let mut with_setup = State::new();
    with_setup.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    with_setup.drain_actions();
    let (rows, leaves) = panes::browser_rows(&with_setup);
    let manager = leaves.iter().position(|leaf| matches!(leaf, panes::Leaf::Setups))
        .expect("the browser shows Setups after the first setup exists");
    assert_eq!(rows[manager].label, "Setups");
    assert_eq!(rows[manager].depth, 1);

    let mut seeded = ShellUi::seeded();
    seeded.state.pending.push_back(CommandKind::RenameOperation {
        name: "Contour1".into(), label: "Finish contour".into(),
    });
    seeded.state.drain_actions();
    let (rows, leaves) = panes::browser_rows(&seeded.state);
    let op = leaves.iter().position(|leaf| matches!(leaf, panes::Leaf::Op(name) if name == "Contour1"))
        .expect("stable operation key in browser");
    assert_eq!(rows[op].label, "Finish contour");
    assert_eq!(rows[op].depth, 2, "operation sits beneath its setup's Operations (#343)");
    assert!(!rows.iter().any(|row| row.label == "Global Setup"));

    let source = model::find_by_name(&seeded.state.world, "Contour1").expect("seed op");
    let definition = model::operation_of(&seeded.state.world, &source).expect("definition");
    seeded.state.pending.push_back(CommandKind::CreateOperation {
        name: "Contour2".into(), tool: definition.tool,
        src: definition.src, params: definition.params,
    });
    seeded.state.drain_actions();
    seeded.state.pending.push_back(CommandKind::ReorderOperation {
        name: "Contour2".into(), target: "Contour1".into(), after: false,
    });
    seeded.state.drain_actions();
    let (rows, leaves) = panes::browser_rows(&seeded.state);
    let order: Vec<_> = leaves.iter().enumerate().filter_map(|(ix, leaf)| match leaf {
        panes::Leaf::Op(key) => Some((key.as_str(), rows[ix].label.as_str())),
        _ => None,
    }).collect();
    assert_eq!(order, [("Contour2", "Contour2"), ("Contour1", "Finish contour")],
        "browser order follows the shared program order, labels stay independent");
}

#[test]
fn dragging_an_operation_row_reorders_within_global_setup() {
    let mut shell = ShellUi::seeded();
    let source = model::find_by_name(&shell.state.world, "Contour1").expect("seed op");
    let definition = model::operation_of(&shell.state.world, &source).expect("definition");
    shell.state.pending.push_back(CommandKind::CreateOperation {
        name: "Contour2".into(), tool: definition.tool,
        src: definition.src, params: definition.params,
    });
    shell.state.drain_actions();
    assert!(model::find_by_name(&shell.state.world, "Contour2").is_some());
    let mut harness = Harness::builder().with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, shell);
    harness.run_steps(2);
    let (_, leaves) = panes::browser_rows(&harness.state().state);
    let first = leaves.iter().position(|leaf| matches!(leaf, panes::Leaf::Op(name) if name == "Contour1"))
        .expect("first row");
    let second = leaves.iter().position(|leaf| matches!(leaf, panes::Leaf::Op(name) if name == "Contour2"))
        .expect("second row");
    assert_eq!(second, first + 1);
    // The browser tree begins at y=200 below its view controls (the
    // projection radios joined them for #434) in the
    // 1280x720 shell. The row indices come from the current flat browser,
    // so the drag follows the rows after setup nesting was removed.
    let row_h = theme::current().row_height;
    let from = egui::pos2(120.0, 221.0 + (second as f32 + 0.5) * row_h);
    let to = egui::pos2(120.0, 221.0 + (first as f32 + 0.25) * row_h);
    set_pos(&mut harness, from);
    button_at(&mut harness, egui::PointerButton::Primary, from, true);
    set_pos(&mut harness, to);
    assert_eq!(harness.state().state.dragging_op.as_deref(), Some("Contour2"));
    button_at(&mut harness, egui::PointerButton::Primary, to, false);
    harness.run_steps(2);
    let ordered: Vec<String> = camcraft_app::setup::operation_order(&harness.state().state.world)
        .into_iter().map(|(name, _)| name).collect();
    assert_eq!(ordered, ["Contour2", "Contour1"]);
}

#[test]
fn copying_and_pasting_an_operation_keeps_settings_and_is_undoable() {
    let mut shell = ShellUi::seeded();
    let original = model::find_by_name(&shell.state.world, "Contour1").expect("source operation");
    let source = model::operation_of(&shell.state.world, &original).expect("source settings");
    shell.state.copy_operation("Contour1");
    shell.state.paste_operation();
    shell.state.drain_actions();

    let pasted = model::find_by_name(&shell.state.world, "Contour1 Copy").expect("pasted operation");
    let mut copied = model::operation_of(&shell.state.world, &pasted).expect("copied settings");
    copied.stamp = source.stamp;
    assert_eq!(copied, source, "tool, source, setup, area, and parameters are copied");
    assert!(model::toolpath_of(&shell.state.world, &pasted).is_none(), "paste needs generation");
    assert_eq!(camcraft_app::setup::operation_order(&shell.state.world)
        .into_iter().map(|(name, _)| name).collect::<Vec<_>>(),
        ["Contour1", "Contour1 Copy"]);

    shell.state.paste_operation();
    shell.state.drain_actions();
    assert!(model::find_by_name(&shell.state.world, "Contour1 Copy 2").is_some());
    shell.state.pending.push_back(CommandKind::Undo);
    shell.state.drain_actions();
    assert!(model::find_by_name(&shell.state.world, "Contour1 Copy 2").is_none());
    shell.state.pending.push_back(CommandKind::Undo);
    shell.state.drain_actions();
    assert!(model::find_by_name(&shell.state.world, "Contour1 Copy").is_none());
    shell.state.pending.push_back(CommandKind::Redo);
    shell.state.drain_actions();
    assert!(model::find_by_name(&shell.state.world, "Contour1 Copy").is_some());
}

#[test]
fn operation_copy_paste_shortcuts_use_the_selected_operation() {
    let mut harness = shell_harness();
    harness.state_mut().state.pending.push_back(CommandKind::Select(Some("Contour1".into())));
    harness.run_steps(2);
    harness.input_mut().events.push(Event::ModifiersChanged(Modifiers::COMMAND));
    harness.input_mut().events.push(Event::Key {
        key: Key::C, physical_key: None, pressed: true, repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    harness.run();
    assert_eq!(harness.state().state.operation_clipboard.as_ref().map(|c| c.label.as_str()), Some("Contour1"));

    harness.input_mut().events.push(Event::Key {
        key: Key::V, physical_key: None, pressed: true, repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    harness.run_steps(2);
    assert!(model::find_by_name(&harness.state().state.world, "Contour1 Copy").is_some());
}

#[test]
fn close_operation_editor_clears_the_page_without_deleting_the_operation() {
    let mut harness = shell_harness();
    harness.state_mut().state.ui_target = UiTarget::Operation("Contour1".into());
    harness.state_mut().state.pending.push_back(CommandKind::Select(Some("Contour1".into())));
    harness.run_steps(2);
    assert_eq!(harness.state().state.selected_operation().as_deref(), Some("Contour1"));
    harness.get_by_label("Close").click();
    harness.run_steps(2);

    let state = &harness.state().state;
    assert_eq!(state.pane_target(), UiTarget::None);
    assert!(state.selected_operation().is_none());
    assert!(state.op_ui.name.is_empty());
    assert!(model::find_by_name(&state.world, "Contour1").is_some());
    assert!(interact::height_planes(state).is_empty());

    harness.state_mut().state.ui_target = UiTarget::Operation("Contour1".into());
    harness.run_steps(2);
    assert_eq!(harness.state().state.op_ui.name, "Contour1", "reopening reloads its draft");
}

#[test]
fn close_property_editors_keep_stock_offsets_models_and_tools() {
    let mut harness = shell_harness();
    let targets = [
        UiTarget::Stock,
        UiTarget::Wcs,
        UiTarget::Model("Block".into()),
        UiTarget::Tool("EM-0600-4FL".into()),
    ];
    for target in targets {
        harness.state_mut().state.ui_target = target;
        harness.run_steps(2);
        harness.get_by_label("Close").click();
        harness.run_steps(2);
        let state = &harness.state().state;
        assert_eq!(state.pane_target(), UiTarget::None);
        assert!(state.selected_operation().is_none());
        for name in ["Stock", "G54", "Block", "EM-0600-4FL", "Contour1"] {
            assert!(model::find_by_name(&state.world, name).is_some(), "Close removed {name}");
        }
    }
}

#[test]
fn close_tool_editor_window_keeps_the_tool() {
    let mut harness = shell_harness();
    harness.state_mut().state.pending.push_back(CommandKind::Select(None));
    harness.state_mut().state.ui_target = UiTarget::None;
    crate::actions::open_tool_editor(&mut harness.state_mut().state, "EM-0600-4FL");
    harness.run_steps(2);
    assert!(harness.state().state.tool_editor.is_some());
    let original_diameter = model::find_by_name(&harness.state().state.world, "EM-0600-4FL")
        .and_then(|entity| model::tool_of(&harness.state().state.world, &entity))
        .expect("seed tool").diameter;
    harness.state_mut().state.tool_ui.diameter = original_diameter + 3.0;
    harness.get_by_label("Close").click();
    harness.run_steps(2);
    let state = &harness.state().state;
    assert!(state.tool_editor.is_none());
    assert!(model::find_by_name(&state.world, "EM-0600-4FL").is_some());
    crate::actions::open_tool_editor(&mut harness.state_mut().state, "EM-0600-4FL");
    assert_eq!(harness.state().state.tool_ui.diameter, original_diameter,
        "reopening discards the uncommitted draft");
}

#[test]
fn operations_heading_menu_pastes_the_copied_operation() {
    let mut harness = shell_harness();
    harness.state_mut().state.copy_operation("Contour1");
    harness.run_steps(2);
    let (_, leaves) = panes::browser_rows(&harness.state().state);
    let row = leaves.iter().position(|leaf| matches!(leaf,
        panes::Leaf::SetupOps(_)))
        .expect("Operations heading");
    let row_height = theme::current().row_height;
    let heading = egui::pos2(90.0, 221.0 + (row as f32 + 0.5) * row_height);
    set_pos(&mut harness, heading);
    button_at(&mut harness, egui::PointerButton::Secondary, heading, true);
    button_at(&mut harness, egui::PointerButton::Secondary, heading, false);
    assert_eq!(harness.state().state.tree_menu, Some(TreeMenuTarget::Operations));

    let paste = egui::pos2(90.0,
        221.0 + (row as f32 + 1.0) * row_height + panes::DROP_GAP + 1.5 * row_height);
    set_pos(&mut harness, paste);
    button_at(&mut harness, egui::PointerButton::Primary, paste, true);
    button_at(&mut harness, egui::PointerButton::Primary, paste, false);
    harness.run_steps(2);
    assert!(model::find_by_name(&harness.state().state.world, "Contour1 Copy").is_some());
}

#[test]
fn operation_type_menu_requires_confirmation() {
    let mut harness = shell_harness();
    harness.run_steps(2);
    let (_, leaves) = panes::browser_rows(&harness.state().state);
    let row = leaves.iter().position(|leaf| matches!(leaf,
        panes::Leaf::Op(name) if name == "Contour1"))
        .expect("operation row");
    let row_height = theme::current().row_height;
    let operation = egui::pos2(90.0, 221.0 + (row as f32 + 0.5) * row_height);
    set_pos(&mut harness, operation);
    button_at(&mut harness, egui::PointerButton::Secondary, operation, true);
    button_at(&mut harness, egui::PointerButton::Secondary, operation, false);
    assert_eq!(harness.state().state.tree_menu,
        Some(TreeMenuTarget::Operation("Contour1".into())));

    // Operation, Rename, Copy, Paste, Clear, Delete, Suppress, After
    // (#110), add pause / home / optional stop (#155), section header, Face.
    let face = egui::pos2(90.0,
        221.0 + (row as f32 + 1.0) * row_height + panes::DROP_GAP + 12.5 * row_height);
    set_pos(&mut harness, face);
    button_at(&mut harness, egui::PointerButton::Primary, face, true);
    button_at(&mut harness, egui::PointerButton::Primary, face, false);
    assert_eq!(harness.state().state.retype_confirm,
        Some(("Contour1".into(), camcraft_app::operation::OpType::Face as usize)));
    let entity = model::find_by_name(&harness.state().state.world, "Contour1").unwrap();
    assert_eq!(model::operation_of(&harness.state().state.world, &entity).unwrap().params.kind(),
        camcraft_app::operation::OpType::Contour);

    button_at(&mut harness, egui::PointerButton::Primary, face, true);
    button_at(&mut harness, egui::PointerButton::Primary, face, false);
    harness.run_steps(2);
    let entity = model::find_by_name(&harness.state().state.world, "Contour1").unwrap();
    assert_eq!(model::operation_of(&harness.state().state.world, &entity).unwrap().params.kind(),
        camcraft_app::operation::OpType::Face);
}

/// #110: the op menu's Suppress row and an after-action row each queue
/// their one undoable command.
#[test]
fn operation_menu_suppresses_and_sets_the_after_action() {
    let mut harness = shell_harness();
    harness.run_steps(2);
    let (_, leaves) = panes::browser_rows(&harness.state().state);
    let row = leaves.iter().position(|leaf| matches!(leaf,
        panes::Leaf::Op(name) if name == "Contour1"))
        .expect("operation row");
    let row_height = theme::current().row_height;
    let operation = egui::pos2(90.0, 221.0 + (row as f32 + 0.5) * row_height);
    let open_menu = |harness: &mut Harness<'static, ShellUi>| {
        set_pos(harness, operation);
        button_at(harness, egui::PointerButton::Secondary, operation, true);
        button_at(harness, egui::PointerButton::Secondary, operation, false);
    };
    let click_row = |harness: &mut Harness<'static, ShellUi>, k: f32| {
        let at = egui::pos2(90.0, 221.0 + (row as f32 + 1.0) * row_height + panes::DROP_GAP + k * row_height);
        set_pos(harness, at);
        button_at(harness, egui::PointerButton::Primary, at, true);
        button_at(harness, egui::PointerButton::Primary, at, false);
        harness.run_steps(2);
    };
    let op = |harness: &Harness<'static, ShellUi>| {
        let world = &harness.state().state.world;
        model::find_by_name(world, "Contour1").and_then(|e| model::operation_of(world, &e)).expect("op")
    };
    // Operation, Rename, Copy, Paste, Clear, Delete, then Suppress.
    open_menu(&mut harness);
    click_row(&mut harness, 6.5);
    assert!(op(&harness).suppressed, "the Suppress row suppresses");
    // ... then the After row, which steps Continue -> Optional -> Program stop.
    open_menu(&mut harness);
    click_row(&mut harness, 7.5);
    assert_eq!(op(&harness).after, camcraft_app::operation::AfterOperation::OptionalStop);
    open_menu(&mut harness);
    click_row(&mut harness, 7.5);
    assert_eq!(op(&harness).after, camcraft_app::operation::AfterOperation::ProgramStop);
}

#[test]
fn rename_button_edits_the_browser_label_through_the_command() {
    let mut harness = shell_harness();
    harness.run();
    harness.get_by_label("Rename").click();
    harness.run();
    assert_eq!(harness.state().state.rename_draft.as_ref().map(|(key, _)| key.as_str()), Some("Contour1"));
    let field = harness.get_all_by_role(egui::accesskit::Role::TextInput)
        .next().expect("rename field under Name");
    field.click();
    harness.run();
    harness.input_mut().events.push(Event::ModifiersChanged(Modifiers::COMMAND));
    harness.input_mut().events.push(Event::Key {
        key: Key::A, physical_key: None, pressed: true, repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    harness.run();
    harness.input_mut().events.push(Event::ModifiersChanged(Modifiers::NONE));
    harness.get_all_by_role(egui::accesskit::Role::TextInput)
        .next().expect("focused rename field")
        .type_text("Finish contour");
    harness.run();
    let apply = harness.get_by_label("Apply name").rect().center();
    harness.get_by_label("Apply name").click();
    harness.run_steps(2);
    let state = &harness.state().state;
    let (rows, leaves) = panes::browser_rows(state);
    let op = leaves.iter().position(|leaf| matches!(leaf, panes::Leaf::Op(key) if key == "Contour1"))
        .expect("operation's stable key");
    assert_eq!(rows[op].label, "Finish contour", "status: {}; draft: {:?}; apply: {apply:?}; pending: {:?}", state.status, state.rename_draft, state.pending);
    assert!(model::find_by_name(&state.world, "Contour1").is_some());
}

/// M13.4: Vertex mode through the real shell - hovering a block corner
/// lights it (the hover carries the vertex), and a click queues the face/
/// edge pick's own SetOperationGeometry with that corner for the open op.
#[test]
fn vertex_mode_hover_and_click_pick_a_corner() {
    let mut harness = shell_harness();
    harness.run();
    harness.get_by_label("Vertices").click();
    harness.run();
    assert_eq!(harness.state().state.geo_pick, GeoPick::Vertex);

    // The seed block's corner nearest the eye that projects inside the
    // pane: a convex body's nearest corner is never hidden.
    let ([x0, y0], [x1, y1]) = harness.state().state.viewport_rect;
    let (w, hgt) = (x1 - x0, y1 - y0);
    let mvp = harness.state().state.cam.mvp((w / hgt) as f64);
    let eye = harness.state().state.cam.eye();
    let mut best: Option<(f64, [f64; 3], egui::Pos2)> = None;
    for x in [0.0, 60.0] {
        for y in [0.0, 60.0] {
            for z in [0.0, 30.0] {
                let c = [x, y, z];
                let Some(uv) = interact::project_uv(&mvp, c) else { continue };
                if !(uv[0] > 0.05 && uv[0] < 0.95 && uv[1] > 0.05 && uv[1] < 0.95) {
                    continue;
                }
                let d = (c[0] - eye[0]).powi(2) + (c[1] - eye[1]).powi(2) + (c[2] - eye[2]).powi(2);
                if best.is_none_or(|(bd, _, _)| d < bd) {
                    best = Some((d, c, egui::pos2(x0 + uv[0] * w, y0 + uv[1] * hgt)));
                }
            }
        }
    }
    let (_, corner, p) = best.expect("the block shows a corner to the default camera");

    // Hover a couple of px off the corner: still within the pick radius.
    set_pos(&mut harness, p + egui::vec2(2.0, 1.0));
    let hovered = harness.state().state.hover.clone().expect("the corner lights under the cursor");
    assert_eq!(hovered.entity, "Block");
    let Some(FeatureSel::Vertex(hid)) = hovered.feature else {
        panic!("Vertex mode hovers a vertex, got {:?}", hovered.feature);
    };

    click_viewport(&mut harness, p);
    let st = &harness.state().state;
    assert_eq!(st.status, "geometry selection updated for Contour1");
    let op = model::find_by_name(&st.world, "Contour1").expect("seed has Contour1");
    let items = match model::operation_of(&st.world, &op).map(|o| o.src) {
        Some(SrcRef::Selection(items)) => items,
        other => panic!("Contour1's source became {other:?}"),
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].feature, FeatureSel::Vertex(hid), "the click picks what the hover lit");
    let block = model::find_by_name(&st.world, "Block").expect("seed has Block");
    let f = model::features_of(&st.world, &block).expect("features");
    let vi = camcraft_app::geometry::feature::resolve_vertex(&f, hid).expect("resolves");
    let loc = model::transform_of(&st.world, &block).unwrap_or_default();
    let v = f.verts[vi as usize];
    let at = [v[0] + loc.pos.x, v[1] + loc.pos.y, v[2] + loc.pos.z];
    assert!(
        (at[0] - corner[0]).abs() + (at[1] - corner[1]).abs() + (at[2] - corner[2]).abs() < 1e-6,
        "picked {at:?}, aimed at {corner:?}"
    );

    // Stage 2 hook: the chip's Origin button places the WCS there.
    harness.get_by_label("Set origin").click();
    harness.run_steps(2);
    let st = &harness.state().state;
    let g54 = model::find_by_name(&st.world, WCS_SLOTS[0]).expect("G54");
    let o = model::transform_of(&st.world, &g54).unwrap_or_default().pos;
    assert!(
        (o.x - corner[0]).abs() + (o.y - corner[1]).abs() + (o.z - corner[2]).abs() < 1e-6,
        "G54 moved to the corner: {o:?}"
    );
}

// ------------------------------------------------ M13.5 B: the post offer

/// The Post node's post offer (M13.5 B; on the setup's Post node since
/// #441): a lathe mounted over a
/// document holding a mill post gets one warning line naming the
/// registry's lathe post, never a switch on the mount itself; the
/// button queues the same `SelectPost` the combo queues, so the post
/// follows and undo puts the mill post back - at which point the
/// offer stands again, and a document that already serves its
/// machine shows no banner at all.
#[test]
fn a_lathe_over_a_mill_post_offers_the_lathe_post_and_rides_undo() {
    use camcraft_app::posting;
    use camcraft_app::schema::PostRef;
    let mut harness = shell_harness();
    // #441: the post group lives on the setup's Post node, not the
    // Stock page.
    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    harness.run_steps(2);
    crate::actions::target(
        &mut harness.state_mut().state,
        UiTarget::SetupNode("Setup1".into(), crate::state::SetupPage::Post),
    );
    harness.run();
    assert!(
        harness.query_by_label_contains("the current post serves").is_none(),
        "a mill document with its mill post is served - no offer"
    );
    harness
        .state_mut()
        .state
        .pending
        .push_back(CommandKind::LoadBuiltinMachine("Lathe".to_owned()));
    harness.run();
    harness.run();
    assert_eq!(
        posting::post_of(&harness.state().state.world),
        PostRef::Builtin,
        "the mount never re-points the post by itself"
    );
    let line = "the current post serves a mill; switch to Fanuc Lathe?";
    assert!(
        harness.query_by_label_contains(line).is_some(),
        "the offer names the lathe post (status: {})",
        harness.state().state.status
    );
    // Bring the button into the scroll area's view before the press,
    // as the pane's other deep rows are pressed.
    harness.get_by_label("Switch post").scroll_to_me();
    harness.run();
    harness.get_by_label("Switch post").click();
    harness.run();
    harness.run();
    assert_eq!(
        posting::post_of(&harness.state().state.world),
        PostRef::Named("Fanuc Lathe".to_owned()),
        "the action switches the post through SelectPost (status: {})",
        harness.state().state.status
    );
    assert!(
        harness.query_by_label_contains("the current post serves").is_none(),
        "a served machine retires the offer"
    );
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    harness.run();
    assert_eq!(
        posting::post_of(&harness.state().state.world),
        PostRef::Builtin,
        "undo restores the mill post (status: {})",
        harness.state().state.status
    );
    assert!(
        harness.query_by_label_contains(line).is_some(),
        "and the offer stands again over the mounted lathe"
    );
}
