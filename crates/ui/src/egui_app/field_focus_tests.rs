//! #2516: on the operation property panel, Tab and Shift+Tab move the
//! focus between the value fields (not onto group headers, combos or the
//! action bar), and Enter commits the typed value to the draft and moves
//! to the next value field of the same group. The value still reaches the
//! document only through Apply's command; a focus move queues nothing.
//! Fields are named by their values, not by positions.
//! Provenance: our own page law; no vendor material consulted.

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::OpParams;

use super::dialog_tests::{opened, page, pocket};
use crate::state::State;

type H = Harness<'static, State>;

fn key(h: &mut H, k: Key, modifiers: Modifiers) {
    h.input_mut().events.push(Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers });
    h.input_mut().events.push(Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers });
    h.run_steps(2);
}

/// The focused node's role and value; a field in edit shows its bare number.
fn focused(h: &H) -> Option<(Role, String)> {
    let out = std::cell::RefCell::new(None);
    let _ = h
        .query_all_by(|n| {
            if n.is_focused() {
                *out.borrow_mut() = Some((n.role(), n.value().unwrap_or_default()));
            }
            false
        })
        .count();
    out.into_inner()
}

fn on_tab(tab: usize) -> H {
    let mut state = opened(pocket(Entry3D::Plunge));
    state.op_tab = tab;
    let mut h = page(state);
    h.run();
    h
}

/// Click the value field that shows `shown` (its unit suffix included).
fn click_field(h: &mut H, shown: &str) {
    h.query_all_by(|n| n.role() == Role::SpinButton && n.value().as_deref() == Some(shown))
        .next()
        .unwrap_or_else(|| panic!("a field showing {shown}"))
        .click();
    h.run_steps(2);
}

fn is_field(h: &H, number: &str) -> bool {
    focused(h) == Some((Role::SpinButton, number.to_owned()))
}

#[test]
fn tab_and_shift_tab_walk_the_value_fields_and_wrap() {
    // Heights: Clearance 25, Top 12, Bottom 9 and the stepdown 1, each in
    // its own group, with combos and group headers between them.
    let mut h = on_tab(super::op_tabs::SEC_HEIGHTS);
    click_field(&mut h, "25.0 mm");
    assert!(is_field(&h, "25.0"), "the click focuses the field: {:?}", focused(&h));
    for next in ["12.0", "9.0", "1.0", "25.0"] {
        key(&mut h, Key::Tab, Modifiers::NONE);
        assert!(is_field(&h, next), "Tab moves to the {next} field, past the combos and headers: {:?}", focused(&h));
    }
    key(&mut h, Key::Tab, Modifiers::SHIFT);
    assert!(is_field(&h, "1.0"), "Shift+Tab from the first field wraps to the last: {:?}", focused(&h));
    key(&mut h, Key::Tab, Modifiers::SHIFT);
    assert!(is_field(&h, "9.0"), "Shift+Tab moves back: {:?}", focused(&h));
    assert!(h.state().pending.is_empty(), "a focus move queues no command");
}

#[test]
fn enter_commits_the_value_and_moves_to_the_next_field_of_its_group() {
    // Feeds: the feed and the plunge feed share one group.
    let mut h = on_tab(super::op_tabs::SEC_FEEDS);
    click_field(&mut h, "800.0 mm/min");
    h.query_all_by(|n| n.is_focused()).next().expect("the feed field").type_text("750");
    h.run();
    key(&mut h, Key::Enter, Modifiers::NONE);
    let draft = match &h.state().op_ui.params {
        OpParams::Pocket { feed, .. } => *feed,
        _ => f64::NAN,
    };
    assert!((draft - 750.0).abs() < 1e-9, "Enter commits the typed feed to the draft ({draft})");
    assert!(is_field(&h, "200.0"), "and moves to the plunge feed: {:?}", focused(&h));
    let live = model::find_by_name(&h.state().world, "Op").and_then(|e| model::operation_of(&h.state().world, &e)).map(|o| o.params);
    assert!(matches!(live, Some(OpParams::Pocket { feed, .. }) if feed == 800.0), "the document waits for Apply's command");
    assert!(h.state().pending.is_empty(), "the focus move queues no command");
    // The moved-to field takes typing at once, as a clicked one does.
    h.query_all_by(|n| n.is_focused()).next().expect("the plunge field").type_text("150");
    h.run();
    key(&mut h, Key::Enter, Modifiers::NONE);
    let plunge = match &h.state().op_ui.params {
        OpParams::Pocket { plunge_feed, .. } => *plunge_feed,
        _ => f64::NAN,
    };
    assert!((plunge - 150.0).abs() < 1e-9, "the typed plunge feed replaces the old one ({plunge})");
    assert!(focused(&h).is_none(), "Enter in the group's last field commits and leaves the focus: {:?}", focused(&h));
}
