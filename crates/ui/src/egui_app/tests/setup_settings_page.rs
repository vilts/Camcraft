//! #1440: the setup's Settings page follows the WCS page's layout - titled
//! groups (active, stock source, part orientation, work offset, workholding
//! on a lathe, the setup's actions), equal action buttons, a readout at the
//! foot - and keeps what it said in the degenerate states (no setups, the
//! first setup, an inactive setup, a mill with no workholding row). Structural
//! twin: labels, order and coarse placement, no pixels. Every label and
//! button the page draws is pinned in some state, so dropping one turns a
//! test red. Provenance: our own page law.

use super::*;
use egui_kittest::kittest::NodeT;
use crate::egui_app::setup_pages::{ACTIVE_SETUP_LABEL, WORKHOLDING_POINTER};
use crate::egui_app::setup_settings_page::{FIRST_GROUP, GROUPS, HOLDING_GROUP, NO_SETUPS, ORIENT_HELP};
use crate::egui_app::setup_workholding_row::ROW_LABEL;
use crate::state::SetupPage;

type H = Harness<'static, ShellUi>;

const ALIGN: &str = "    Part alignment";
const TURN: &str = "      Turn";
const ABOUT: &str = "      about";
const RELATIVE: &str = "    Relative to previous setup";
const TURN_PREV: &str = "      Turn from previous";
const PLANE: &str = "    Lay on a plane - pick a face";
const POINTS: &str = "    Lay on a plane - pick three points";
const AXES: &str = "    Part axes - pick an edge or two points for +Y and +X; +Z follows by the right-hand rule";
const OFFSET: &str = "    Work offset";
const FEATURES: &str = "    Set WCS from features - each pick sets a role";
const FIRST_STOCK_NOTE: &str = "The first setup has no setup before it to take stock from.";

/// Tall, so the whole page is on screen and every control is hit-testable.
fn tall() -> H {
    Harness::builder().with_size(Vec2::new(1800.0, 2600.0)).build_ui_state(frame, ShellUi::blank())
}

fn setups(n: usize, active: &str) -> H {
    let mut h = tall();
    for _ in 0..n {
        h.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
        h.state_mut().state.drain_actions();
    }
    h.state_mut().state.pending.push_back(CommandKind::ActivateSetup(Some(active.into())));
    h.state_mut().state.drain_actions();
    h
}

fn open(h: &mut H, setup: &str) {
    crate::actions::target(&mut h.state_mut().state, UiTarget::SetupNode(setup.into(), SetupPage::Settings));
    h.run_steps(3);
}

/// #771: the plane orientation dropdown's current entry is `from`; pick
/// `to` (the other methods' rows wait behind it).
fn plane_method(h: &mut H, from: &str, to: &str) {
    h.get_by(|n| n.role() == egui::accesskit::Role::ComboBox && n.value().as_deref() == Some(from)).click();
    h.run_steps(3);
    h.get_by_label(to).click();
    h.run_steps(3);
}

fn rect(h: &H, label: &str) -> egui::Rect {
    let all: Vec<egui::Rect> = h.query_all_by_label(label).map(|n| n.rect()).collect();
    assert_eq!(all.len(), 1, "{label:?} drawn exactly once: {all:?}");
    all[0]
}

fn top(h: &H, label: &str) -> f32 {
    rect(h, label).top()
}

fn gone(h: &H, label: &str) {
    assert!(h.query_all_by_label(label).next().is_none(), "{label:?} is not drawn here");
}

/// Every label drawn once, each below the group title before it and above
/// the next one, and never above the one before it in the list.
fn assert_group(h: &H, title: &str, next: &str, labels: &[&str]) {
    assert_between(h, title, top(h, next), labels);
}

/// The foot readout's top: the line that starts "Stock entity".
fn foot(h: &H) -> f32 {
    let all: Vec<f32> = h.query_all_by_label_contains("Stock entity").map(|n| n.rect().top()).collect();
    assert_eq!(all.len(), 1, "one readout line: {all:?}");
    all[0]
}

fn assert_between(h: &H, title: &str, hi: f32, labels: &[&str]) {
    let lo = top(h, title);
    let mut last = lo;
    for l in labels {
        let t = top(h, l);
        assert!(t > lo && t < hi, "{l:?} sits in {title:?} (between {lo} and {hi}), at {t}");
        assert!(t + 4.0 >= last, "{l:?} keeps its order in {title:?}: {t} after {last}");
        last = t;
    }
}

fn is_checked(h: &H, label: &str) -> bool {
    h.get_by(|n| n.role() == egui::accesskit::Role::RadioButton && n.label().as_deref() == Some(label))
        .accesskit_node()
        .toggled()
        == Some(egui::accesskit::Toggled::True)
}

/// A second setup that is the active one: every group, in order, with its
/// controls under its own title, and the foot readout.
#[test]
fn the_setup_page_is_laid_out_in_groups_like_the_wcs_page() {
    let mut h = setups(2, "Setup2");
    open(&mut h, "Setup2");
    let tops: Vec<f32> = GROUPS.iter().map(|g| top(&h, g)).collect();
    assert!(tops.windows(2).all(|w| w[0] < w[1]), "groups stack top to bottom: {tops:?}");
    gone(&h, HOLDING_GROUP);
    gone(&h, FIRST_GROUP);
    assert_group(&h, GROUPS[0], GROUPS[1], &[ACTIVE_SETUP_LABEL]);
    assert_group(&h, GROUPS[1], GROUPS[2], &["Its own stock (the Stock node)", "What Setup1 leaves behind"]);
    // #771: the two dropdowns head the group; Axes (the default) shows its rows.
    assert_group(
        &h,
        GROUPS[2],
        GROUPS[3],
        &[
            ORIENT_HELP, "Plane orientation", AXES, "      +Y", "+Y by Edge", "+Y by Two points", "Pick +Y edge", "Flip +Y", "      +X",
            "+X by Edge", "+X by Two points", "Pick +X edge", "Flip +X", "Orientation point", RELATIVE, TURN_PREV,
        ],
    );
    for gone_label in [ALIGN, PLANE, POINTS] {
        gone(&h, gone_label);
    }
    // The other methods draw their own rows behind the dropdown.
    plane_method(&mut h, "Axes", "Face");
    assert_group(&h, GROUPS[2], GROUPS[3], &["Plane orientation", PLANE, "Pick a face", "Face down", "Face to plane level", RELATIVE]);
    plane_method(&mut h, "Face", "Points");
    assert_group(
        &h,
        GROUPS[2],
        GROUPS[3],
        &["Plane orientation", POINTS, "Pick three points", "Points down", "Point 1 to plane level", "3-2-1 frame", RELATIVE],
    );
    plane_method(&mut h, "Points", "Manual");
    assert_group(&h, GROUPS[2], GROUPS[3], &["Plane orientation", ALIGN, TURN, ABOUT, RELATIVE]);
    plane_method(&mut h, "Manual", "Axes");
    assert_group(&h, GROUPS[3], GROUPS[4], &[FEATURES, "Pick for WCS", OFFSET]);
    // The page's old reading order holds: "Set WCS from features" above the G54-G59 picker.
    assert!(top(&h, FEATURES) < top(&h, OFFSET), "features row before the work offset picker");
    assert_between(&h, GROUPS[4], foot(&h), &["Delete Setup2", "Open Setup2 fixtures"]);
    // Stock follows its source: a fresh setup's own stock entity, stated at the foot.
    assert!(foot(&h) > top(&h, "Open Setup2 fixtures"), "the readout is under the groups");
    // The relative Apply is drawn; the alignment one waits behind Manual (#771).
    assert_eq!(h.query_all_by_label("Apply").count(), 1, "relative has Apply");
    plane_method(&mut h, "Axes", "Manual");
    assert_eq!(h.query_all_by_label("Apply").count(), 2, "alignment and relative each have Apply");
    plane_method(&mut h, "Manual", "Axes");
    // The setup actions are two equal buttons on one row, left to right.
    let (del, open_fx) = (rect(&h, "Delete Setup2"), rect(&h, "Open Setup2 fixtures"));
    assert!((del.top() - open_fx.top()).abs() < 1.0 && del.right() < open_fx.left(), "{del:?} {open_fx:?}");
    assert!((del.width() - open_fx.width()).abs() < 1.0, "equal widths: {del:?} {open_fx:?}");
}

/// The first setup has no setup before it: its stock note stands in for the
/// carry choice and the relative row is not drawn.
#[test]
fn the_first_setups_page_has_no_previous_setup_rows() {
    let mut h = setups(1, "Setup1");
    open(&mut h, "Setup1");
    assert!(is_checked(&h, ACTIVE_SETUP_LABEL), "the first setup is the active one");
    assert_group(&h, GROUPS[1], GROUPS[2], &["Its own stock (the Stock node)", FIRST_STOCK_NOTE]);
    for g in GROUPS {
        top(&h, g);
    }
    for absent in [RELATIVE, TURN_PREV, "What Setup1 leaves behind"] {
        gone(&h, absent);
    }
    assert_eq!(h.query_all_by_label("Apply").count(), 0, "no Apply: Axes waits for a pick, the relative row is absent");
    plane_method(&mut h, "Axes", "Manual");
    assert_eq!(h.query_all_by_label("Apply").count(), 1, "only the alignment Apply");
    plane_method(&mut h, "Manual", "Axes");
    assert_between(&h, GROUPS[4], foot(&h), &["Delete Setup1", "Open Setup1 fixtures"]);
}

/// A setup that is not the active one: the radio is unchecked, the page
/// still draws every group, and a click queues the one undoable activation.
#[test]
fn an_inactive_setups_page_draws_its_groups_and_the_radio_activates_it() {
    let mut h = setups(2, "Setup1");
    open(&mut h, "Setup2");
    assert!(!is_checked(&h, ACTIVE_SETUP_LABEL), "Setup2 is not the active one");
    for g in GROUPS {
        top(&h, g);
    }
    h.get_by_label(ACTIVE_SETUP_LABEL).click();
    h.run_steps(3);
    assert_eq!(camcraft_app::setup::active(&h.state().state.world).as_deref(), Some("Setup2"));
    assert!(is_checked(&h, ACTIVE_SETUP_LABEL));
    let st = &mut h.state_mut().state;
    assert!(st.history.undo(&st.world), "one undo step");
    assert_eq!(camcraft_app::setup::active(&st.world).as_deref(), Some("Setup1"));
}

/// With no setups the page is the one "First setup" group, and the button
/// creates the first setup (undoably).
#[test]
fn the_page_with_no_setups_offers_only_the_first_setup() {
    let mut h = tall();
    open(&mut h, "");
    assert_group(&h, FIRST_GROUP, "Create first setup", &[NO_SETUPS, WORKHOLDING_POINTER]);
    assert!(top(&h, "Create first setup") > top(&h, WORKHOLDING_POINTER));
    for g in GROUPS.iter().chain([&HOLDING_GROUP]) {
        gone(&h, g);
    }
    assert!(camcraft_app::setup::names(&h.state().state.world).is_empty());
    h.get_by_label("Create first setup").click();
    h.run_steps(3);
    assert_eq!(camcraft_app::setup::names(&h.state().state.world), ["Setup1"]);
    let st = &mut h.state_mut().state;
    assert!(st.history.undo(&st.world), "one undo step");
    assert!(camcraft_app::setup::names(&st.world).is_empty());
}

/// A turning setup draws the Workholding group between Work offset and the
/// setup's actions, and its controls still queue the undoable command.
#[test]
fn a_lathe_setup_draws_the_workholding_group() {
    let mut h = setups(1, "Setup1");
    open(&mut h, "Setup1");
    gone(&h, HOLDING_GROUP);
    {
        let st = &mut h.state_mut().state;
        let lathe = camcraft_app::machine::Machine { kind: Some("lathe".into()), ..Default::default() };
        let load = camcraft_app::commands::LoadMachine::new(&st.world, "lathe.mch".into(), Some(lathe));
        st.history.execute(&st.world, load.expect("a lathe"));
    }
    h.run_steps(3);
    let row = ROW_LABEL.trim_end();
    assert!(top(&h, GROUPS[3]) < top(&h, HOLDING_GROUP) && top(&h, HOLDING_GROUP) < top(&h, GROUPS[4]), "between offset and actions");
    assert_group(&h, HOLDING_GROUP, GROUPS[4], &[row, "Apply rpm", "      No workholding limit stated"]);
    gone(&h, "Clear rpm");
    h.get_by_label("Apply rpm").click();
    h.run_steps(3);
    assert_group(&h, HOLDING_GROUP, GROUPS[4], &[row, "Apply rpm", "Clear rpm", "      Stated: 2000 rpm"]);
}

/// Arming each pick draws its extra controls inside "Part orientation" /
/// "Work offset" - the rows that appear only once armed.
#[test]
fn the_armed_pick_rows_stay_inside_their_groups() {
    let mut h = setups(2, "Setup2");
    open(&mut h, "Setup2");
    plane_method(&mut h, "Axes", "Face");
    h.get_by_label("Pick a face").click();
    h.run_steps(3);
    assert_group(&h, GROUPS[2], GROUPS[3], &[PLANE, "Stop picking", "Face down", "      Picked: nothing picked yet", "Apply face"]);
    plane_method(&mut h, "Face", "Points");
    h.get_by_label("Pick three points").click();
    h.run_steps(3);
    assert_group(
        &h,
        GROUPS[2],
        GROUPS[3],
        &[POINTS, "Stop picking points", "Points down", "      Point 1: click a corner in the viewport", "      Point 2: -", "      Point 3: -",
          "Remove last", "Clear points", "Apply points"],
    );
    h.get_by_label("Stop picking points").click(); // one armed pick at a time
    h.run_steps(3);
    h.get_by_label("Pick for WCS").click();
    h.run_steps(3);
    assert_group(&h, GROUPS[3], GROUPS[4], &[FEATURES, "Stop picking", "      Picked: nothing picked yet"]);
    // Apply is drawn twice (relative, WCS; the alignment one waits behind
    // the Manual entry, #771); the WCS one is the lowest, inside "Work
    // offset", under its picked line.
    let applies: Vec<f32> = h.query_all_by_label("Apply").map(|n| n.rect().top()).collect();
    assert_eq!(applies.len(), 2, "{applies:?}");
    let wcs = applies.iter().copied().fold(f32::MIN, f32::max);
    assert!(wcs > top(&h, "      Picked: nothing picked yet") - 4.0 && wcs < top(&h, GROUPS[4]), "WCS Apply in its group");
}

/// The two action buttons keep their commands: Delete is one undoable
/// `DeleteSetup`, Open fixtures activates the setup and opens its page.
#[test]
fn the_setup_actions_still_queue_their_commands() {
    let mut h = setups(2, "Setup1");
    open(&mut h, "Setup2");
    h.get_by_label("Delete Setup2").click();
    h.run_steps(3);
    assert_eq!(camcraft_app::setup::names(&h.state().state.world), ["Setup1"], "Delete removed Setup2");
    let st = &mut h.state_mut().state;
    assert!(st.history.undo(&st.world), "one undo step");
    assert_eq!(camcraft_app::setup::names(&st.world).len(), 2, "undo restores it");
}

#[test]
fn open_fixtures_activates_the_setup_and_opens_its_fixtures_page() {
    let mut h = setups(2, "Setup1");
    open(&mut h, "Setup2");
    h.get_by_label("Open Setup2 fixtures").click();
    h.run_steps(3);
    assert_eq!(camcraft_app::setup::active(&h.state().state.world).as_deref(), Some("Setup2"), "activated, as the browser row does");
    assert!(
        matches!(&h.state().state.ui_target, UiTarget::SetupNode(n, SetupPage::Fixtures) if n == "Setup2"),
        "its Fixtures page is open: {:?}",
        h.state().state.ui_target
    );
}
