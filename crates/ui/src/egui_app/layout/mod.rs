//! The dock's layout law: the default three-pane split, the panel
//! toggles the ribbon's View door asks for, and that door's dropdown.
//! The viewport is permanent (the shell's TabViewer refuses to close it
//! or float it); the browser and the properties pane may close, and the
//! door reopens them at their default side or rebuilds the whole
//! default dock. Layout is view state - immediate-mode, never a
//! command, never written to the project file.
//! Provenance: our own code, specced from the egui_dock 0.21 docs and
//! vendored sources; no HSMWorks material involved.
//! The property-page form helpers (`prop_row`, `group`, `auto_value`,
//! `status_banner`, `action_bar`, ...) live in `form` and are re-exported
//! here: one layout module for the dock and the pages alike.

mod controls;
mod form;
pub(crate) use controls::*;
pub(crate) use form::*;
#[cfg(test)]
mod banner_tests;
#[cfg(test)]
mod form_seam_tests;

use egui::{Rect, Sense, Ui, Vec2};
use egui_dock::{DockState, Node, NodePath, Split, SurfaceIndex};

use crate::state::{LayoutRequest, Panel, PanelsOpen, State};

use super::shell::Tab;
use super::theme::Tokens;

/// The browser's share of the window, left (shell.rs's DockLayout ratio).
const BROWSER_FRAC: f32 = 0.17;
/// The viewport's share of what the browser leaves; the properties pane
/// takes the rest at the right.
const VIEWPORT_FRAC: f32 = 0.735;
/// The share the rest of the dock keeps when the properties pane is
/// reopened alone at the far right - 22% of the window for the pane,
/// the default layout's width for it.
const REOPEN_PROPS_FRAC: f32 = 0.78;

/// The shell's default three-pane layout, as egui_dock splits: the browser
/// takes 17% at the left and the properties pane 22% at the right of what
/// remains. A fraction governs the TOP/LEFT child (SplitNode docs), and a
/// `Left` split puts the NEW node left, a `Right` split the OLD one - so
/// the first fraction is the browser's share, the second the viewport's.
pub(crate) fn build_dock() -> DockState<Tab> {
    let mut dock = DockState::new(vec![Tab::Viewport]);
    let [vp, _browser] =
        dock.split(NodePath::MAIN_ROOT, Split::Left, BROWSER_FRAC, Node::leaf(Tab::Browser));
    dock.split(
        NodePath::new(SurfaceIndex::main(), vp),
        Split::Right,
        VIEWPORT_FRAC,
        Node::leaf(Tab::Properties),
    );
    dock
}

/// The permanence law: the viewport never closes and never floats; the
/// other panes may do both. One predicate, read by the shell's TabViewer
/// for `is_closeable` and `allowed_in_windows` alike.
pub(crate) fn closeable(tab: &Tab) -> bool {
    !matches!(tab, Tab::Viewport)
}

pub(crate) fn tab_of(p: Panel) -> Tab {
    match p {
        Panel::Browser => Tab::Browser,
        Panel::Properties => Tab::Properties,
        Panel::Machines => Tab::Machines,
    }
}

/// Whether the dock holds `tab` anywhere - main surface or a float.
pub(crate) fn is_open(dock: &DockState<Tab>, tab: &Tab) -> bool {
    dock.find_tab(tab).is_some()
}

/// Whether `tab` draws this frame (#667): in the dock, the active tab
/// of its leaf, and that leaf not collapsed. A tab behind another in its
/// leaf, or closed, does not.
pub(crate) fn is_shown(dock: &DockState<Tab>, tab: &Tab) -> bool {
    dock.find_tab(tab)
        .is_some_and(|p| dock.leaf(p.node_path()).is_ok_and(|l| l.active == p.tab && !l.collapsed))
}

/// The closable panels' presence, for the View door's checkmarks.
pub(crate) fn panels_open(dock: &DockState<Tab>) -> PanelsOpen {
    PanelsOpen {
        browser: is_open(dock, &Tab::Browser),
        properties: is_open(dock, &Tab::Properties),
        machines: is_open(dock, &Tab::Machines),
    }
}

/// Reopen a closed panel at its default side of the whole main dock.
/// A main surface with no usable root (egui_dock's `split` asserts on
/// one) takes the panel as its first leaf instead - placed, not lost.
fn reopen(dock: &mut DockState<Tab>, p: Panel) {
    let tab = tab_of(p);
    if is_open(dock, &tab) {
        return;
    }
    let rooted = dock
        .main_surface()
        .root_node()
        .is_some_and(|n| n.is_leaf() || n.is_parent());
    if !rooted {
        dock.push_to_first_leaf(tab);
        return;
    }
    // #287: the Machines pane opens as a tab beside the properties pane
    // when that is open (same side, same width - one more context pane),
    // else alone at the right like the properties pane would.
    if p == Panel::Machines
        && let Some(props) = dock.find_tab(&Tab::Properties)
        && let Ok(leaf) = dock.leaf_mut(props.node_path())
    {
        leaf.append_tab(tab);
        return;
    }
    let (side, frac) = match p {
        Panel::Browser => (Split::Left, BROWSER_FRAC),
        Panel::Properties | Panel::Machines => (Split::Right, REOPEN_PROPS_FRAC),
    };
    dock.split(NodePath::MAIN_ROOT, side, frac, Node::leaf(tab));
}

fn close(dock: &mut DockState<Tab>, p: Panel) {
    if let Some(path) = dock.find_tab(&tab_of(p)) {
        dock.remove_tab(path);
    }
}

/// Carry out one View-door ask on the dock.
pub(crate) fn apply(dock: &mut DockState<Tab>, req: LayoutRequest) {
    match req {
        LayoutRequest::Toggle(p) if is_open(dock, &tab_of(p)) => close(dock, p),
        LayoutRequest::Toggle(p) => reopen(dock, p),
        LayoutRequest::Reset => *dock = build_dock(),
    }
}

/// The shell's per-frame seam: land any pending ask, then mirror the
/// dock into `state.panels_open`. Called before the ribbon draws (so its
/// checkmarks are current) and after (so a click lands before the dock
/// lays out this same frame).
pub(crate) fn settle(state: &mut State, dock: &mut DockState<Tab>) {
    if let Some(req) = state.layout_request.take() {
        apply(dock, req);
    }
    state.panels_open = panels_open(dock);
}

// ------------------------------------------------------------ View door

/// One row of the View door's dropdown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ViewRow {
    /// Listed, always checked, inert: the viewport cannot close.
    Viewport,
    Panel(Panel),
    Reset,
}

const ROWS: [ViewRow; 5] = [
    ViewRow::Viewport,
    ViewRow::Panel(Panel::Browser),
    ViewRow::Panel(Panel::Properties),
    ViewRow::Panel(Panel::Machines),
    ViewRow::Reset,
];

fn row_label(r: ViewRow) -> &'static str {
    match r {
        ViewRow::Viewport => "Viewport",
        ViewRow::Panel(Panel::Browser) => "Browser",
        ViewRow::Panel(Panel::Properties) => "Properties",
        ViewRow::Panel(Panel::Machines) => "Machines",
        ViewRow::Reset => "Reset layout",
    }
}

/// The door's rows, pure geometry in the strip's global frame (the
/// machine door's law): hung under the button, pulled left so the last
/// cell's list never runs off the strip's right edge.
pub(super) fn view_rows(
    button: Rect,
    strip: Rect,
    row_h: f32,
    w: f32,
    gap: f32,
) -> Vec<(ViewRow, Rect)> {
    let left = button.left().min(strip.right() - w).max(strip.left());
    ROWS.iter()
        .enumerate()
        .map(|(k, r)| {
            (
                *r,
                Rect::from_min_size(
                    egui::Pos2::new(left, button.bottom() + gap + k as f32 * row_h),
                    Vec2::new(w, row_h),
                ),
            )
        })
        .collect()
}

/// The open View door's dropdown: a foreground Area of toggle rows
/// (box filled = panel open) and the Reset row. A press on a live row
/// files a `LayoutRequest` and closes the door; an outside press closes
/// it. Nothing here touches the command queue.
pub(super) fn view_drop(
    ui: &mut Ui,
    state: &mut State,
    pal: &Tokens,
    id: egui::Id,
    button: Rect,
    rows: &[(ViewRow, Rect)],
) {
    let Some(top) = rows.first().map(|(_, r)| r.min) else {
        state.ribbon_view_open = false;
        return;
    };
    let w = rows.first().map_or(0.0, |(_, r)| r.width());
    let panel = Rect::from_min_size(top, Vec2::new(w, rows.len() as f32 * pal.row_height));
    let open = state.panels_open;
    let mut chose: Option<ViewRow> = None;
    egui::Area::new(id)
        .fixed_pos(panel.min)
        .order(egui::Order::Foreground)
        .movable(false)
        .show(ui.ctx(), |ui| {
            let p = ui.painter();
            p.rect_filled(panel, pal.radius_menu, pal.ink);
            p.rect_stroke(
                panel,
                pal.radius_menu,
                egui::Stroke::new(1.0, pal.border),
                egui::StrokeKind::Outside,
            );
            for (k, (row, r)) in rows.iter().enumerate() {
                let resp = ui.interact(*r, id.with(("view-row", k)), Sense::click());
                let live = *row != ViewRow::Viewport;
                if resp.hovered() && live {
                    p.rect_filled(*r, 0.0, pal.field_hover);
                }
                let checked = match row {
                    ViewRow::Viewport => Some(true),
                    ViewRow::Panel(pn) => Some(open.get(*pn)),
                    ViewRow::Reset => None,
                };
                if let Some(on) = checked {
                    let b = Rect::from_center_size(
                        r.left_center() + Vec2::new(14.0, 0.0),
                        Vec2::splat(10.0),
                    );
                    p.rect_stroke(
                        b,
                        2.0,
                        egui::Stroke::new(1.0, pal.text_dim),
                        egui::StrokeKind::Inside,
                    );
                    if on {
                        p.rect_filled(b.shrink(2.0), 1.0, pal.accent);
                    }
                }
                p.text(
                    r.left_center() + Vec2::new(28.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    row_label(*row),
                    egui::FontId::proportional(pal.font_row),
                    if live { pal.text } else { pal.text_faint },
                );
                let clicked = resp.clicked();
                if *row == ViewRow::Viewport {
                    resp.on_hover_text("The 3D viewport is always open");
                }
                if clicked {
                    chose = Some(*row);
                }
            }
        });
    match chose {
        // The inert row swallows its press: the list stays open, a
        // click on "always open" is not a reason to dismiss the door.
        Some(ViewRow::Viewport) => {}
        Some(ViewRow::Panel(p)) => {
            state.layout_request = Some(LayoutRequest::Toggle(p));
            state.ribbon_view_open = false;
        }
        Some(ViewRow::Reset) => {
            state.layout_request = Some(LayoutRequest::Reset);
            state.ribbon_view_open = false;
        }
        None => {
            let outside = ui.input(|i| {
                i.pointer.button_pressed(egui::PointerButton::Primary)
                    && i.pointer.interact_pos().is_none_or(|p| {
                        !button.expand(2.0).contains(p) && !rows.iter().any(|(_, r)| r.contains(p))
                    })
            });
            if outside {
                state.ribbon_view_open = false;
            }
        }
    }
}
