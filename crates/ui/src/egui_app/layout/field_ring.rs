//! #2516: keyboard order over the Properties pane's value fields. egui's
//! own Tab walks every focusable widget in draw order - group headers,
//! combos, the action bar's buttons - so from a value field the focus
//! seemed to leave the form. Here the pane's value fields (the form
//! seam's number helpers register them, with the group they sit in) form
//! a ring: from a focused value field, Tab moves to the next field,
//! Shift+Tab to the previous one, and Enter - which commits the value to
//! the draft, as before - moves to the next field of the same group. The
//! value still reaches the document only through the page's command
//! (Apply, `EditOperationParams`); focus is ephemeral UI state and needs
//! no command.
//!
//! Provenance: our own design over egui's `Memory::request_focus` (the
//! vendored egui 0.36 docs); no vendor material consulted.

use egui::{Context, Id, Key, Ui};

/// One frame's ring: the focus and keys read when the pane began, and the
/// value fields registered since, in draw order, each with its group.
#[derive(Clone, Default)]
struct Ring {
    open: bool,
    was: Option<Id>,
    tab: bool,
    shift: bool,
    enter: bool,
    fields: Vec<(Id, Option<Id>)>,
    group: Option<Id>,
    /// The field the last frame moved to, focused when the next frame
    /// begins - before any widget draws, so the field GAINS the focus and
    /// selects its text (a typed number replaces it, as after a click),
    /// and after egui applied its own Shift+Tab (`id_next_frame`).
    pending: Option<Id>,
}

fn key() -> Id {
    Id::new("camcraft.field_ring")
}

fn with<R>(ctx: &Context, f: impl FnOnce(&mut Ring) -> R) -> R {
    ctx.data_mut(|d| f(d.get_temp_mut_or_default::<Ring>(key())))
}

/// Open the ring at the start of the pane: read the focus and the keys
/// before any widget of this frame takes the focus over.
pub fn begin(ui: &Ui) {
    if let Some(to) = with(ui.ctx(), |r| r.pending.take()) {
        ui.memory_mut(|m| m.request_focus(to));
    }
    let was = ui.memory(|m| m.focused());
    // The Tab press's own modifiers say back or forward: the frame's
    // modifier state may already read the Shift as released.
    let (tab, shift, enter) = ui.input(|i| {
        let tab = i.events.iter().find_map(|e| match e {
            egui::Event::Key { key: Key::Tab, pressed: true, modifiers, .. } => Some(modifiers.shift),
            _ => None,
        });
        (tab.is_some(), tab.unwrap_or(false), i.key_pressed(Key::Enter))
    });
    with(ui.ctx(), |r| *r = Ring { open: true, was, tab, shift, enter, ..Ring::default() });
}

/// Register a value field drawn inside the open ring. Outside the pane
/// (no open ring) this does nothing.
pub fn register(ui: &Ui, field: Id) {
    with(ui.ctx(), |r| {
        if r.open {
            let g = r.group;
            r.fields.push((field, g));
        }
    });
}

/// Draw a group's body with `group` as the group of the fields in it.
pub fn in_group<R>(ui: &mut Ui, group: Id, add: impl FnOnce(&mut Ui) -> R) -> R {
    let outer = with(ui.ctx(), |r| r.group.replace(group));
    let out = add(ui);
    with(ui.ctx(), |r| r.group = outer);
    out
}

/// Close the ring at the end of the pane, and move the focus where the
/// key asked: the next or previous field for Tab, the next field of the
/// same group for Enter (none after the group's last field).
pub fn finish(ui: &Ui) {
    let (fields, was, tab, shift, enter) = with(ui.ctx(), |r| {
        r.open = false;
        (std::mem::take(&mut r.fields), r.was, r.tab, r.shift, r.enter)
    });
    if let Some(to) = target(&fields, was, tab, shift, enter) {
        with(ui.ctx(), |r| r.pending = Some(to));
        ui.ctx().request_repaint();
    }
}

/// The field the keys move to from `was`, if any: the pure rule.
fn target(fields: &[(Id, Option<Id>)], was: Option<Id>, tab: bool, shift: bool, enter: bool) -> Option<Id> {
    let at = fields.iter().position(|(id, _)| Some(*id) == was)?;
    let n = fields.len();
    if tab {
        let to = if shift { (at + n - 1) % n } else { (at + 1) % n };
        return (to != at).then(|| fields[to].0);
    }
    if enter {
        let group = fields[at].1;
        return fields.get(at + 1).filter(|(_, g)| *g == group).map(|(id, _)| *id);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring() -> Vec<(Id, Option<Id>)> {
        let (a, b) = (Some(Id::new("A")), Some(Id::new("B")));
        vec![(Id::new(1), a), (Id::new(2), a), (Id::new(3), b), (Id::new(4), b)]
    }

    #[test]
    fn tab_and_shift_tab_walk_the_ring_and_wrap() {
        let f = ring();
        assert_eq!(target(&f, Some(Id::new(2)), true, false, false), Some(Id::new(3)), "Tab crosses into the next group");
        assert_eq!(target(&f, Some(Id::new(4)), true, false, false), Some(Id::new(1)), "Tab wraps");
        assert_eq!(target(&f, Some(Id::new(3)), true, true, false), Some(Id::new(2)), "Shift+Tab goes back");
        assert_eq!(target(&f, Some(Id::new(1)), true, true, false), Some(Id::new(4)), "Shift+Tab wraps");
    }

    #[test]
    fn enter_moves_within_the_group_only() {
        let f = ring();
        assert_eq!(target(&f, Some(Id::new(1)), false, false, true), Some(Id::new(2)));
        assert_eq!(target(&f, Some(Id::new(2)), false, false, true), None, "the group's last field keeps egui's commit");
    }

    #[test]
    fn a_focus_outside_the_ring_or_no_key_moves_nothing() {
        let f = ring();
        assert_eq!(target(&f, Some(Id::new(9)), true, false, false), None, "a combo or a button keeps egui's order");
        assert_eq!(target(&f, None, true, false, false), None);
        assert_eq!(target(&f, Some(Id::new(1)), false, false, false), None);
        assert_eq!(target(&[(Id::new(1), None)], Some(Id::new(1)), true, false, false), None, "one field: nowhere to go");
    }
}
