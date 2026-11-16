//! The operation page's section tab bar: the specced named sections
//! (ROADMAP M1.4: "tabs and named sections (geometry, heights, passes,
//! leads and links, feeds) instead of long stacks"), which kinds have
//! which, and the strip that switches between them.
//!
//! M9.1 B folded the tabs into one stacked pane; user feedback reversed
//! that ("everything is just a lot to scroll thru - no organization"), so
//! the bar is back in its M1.4 shape: one tab per section the kind has
//! fields for (no dead tabs), each field on its own tab only, in its
//! within-section order. The open tab is `State::op_tab` - transient view
//! state, never a command, never saved: a click queues nothing.
//!
//! The mapping is by section, not by field: `pages::params_fields` guards
//! every field with the section it belongs to, so a field added under an
//! existing guard lands on that tab with no edit here. `has_section` only
//! answers which tabs a kind shows, and the registry-wide drift guard in
//! `tests.rs` renders every kind's every section to prove the answer
//! matches what the guards actually draw (both directions).
//!
//! Provenance: our own code (the M1.4 tab bar, restored from our git
//! history); no HSMWorks material involved.

use egui::{CornerRadius, Stroke, Ui, Vec2};

use camcraft_app::operation::OpType;

use super::contrast;
use super::theme::Tokens;

/// The section names in tab-bar order. The guard indices in
/// `pages::params_fields` are these same numbers - one placement law,
/// table and guards read each other.
pub(crate) const SECTIONS: [&str; 6] = ["Geometry", "Heights", "Passes", "Lead in / Lead out", "Feeds", "Machine"];
pub(super) const SEC_GEO: usize = 0;
pub(super) const SEC_HEIGHTS: usize = 1;
pub(super) const SEC_PASSES: usize = 2;
pub(super) const SEC_LEADS: usize = 3;
pub(super) const SEC_FEEDS: usize = 4;
pub(super) const SEC_MACHINE: usize = 5;

/// Which sections an operation kind actually has fields (or the pick law)
/// for: Geometry/Heights/Feeds always; Passes except the single-pass rides
/// (Trace and its engrave twin, the chamfer whose floor is derived from
/// width and angle, the deburr's one edge - none has a stepover or a leave
/// to show); Lead in / Lead out (M11.2, the user's "as is common in most
/// CAM packages") on every level-ladder kind - the entry, the descent
/// between levels and the links live there, with the contour's lead moves
/// and tabs; Machine only Part - the bar feeder's signals are the one
/// lifecycle group no cutting tab names (M5.2 G.2). The tab bar shows
/// exactly these: no dead tabs.
pub(crate) fn has_section(kind: OpType, s: usize) -> bool {
    match s {
        SEC_GEO | SEC_HEIGHTS => true,
        // A wire cut states no feed (#532): the control's generator
        // sets the rate, so the kind has no Feeds tab.
        SEC_FEEDS => kind != OpType::WireContour,
        SEC_PASSES => {
            !matches!(
                kind,
                OpType::Trace | OpType::Engrave | OpType::Chamfer2D | OpType::Deburr
            )
        }
        SEC_LEADS => matches!(
            kind,
            OpType::Contour
                | OpType::Pocket
                | OpType::Adaptive2D
                | OpType::Offset2D
                | OpType::Circular
                | OpType::Slot
                | OpType::Bore
                | OpType::Pocket3D
                | OpType::RotaryOffset
                | OpType::MultiAxisRough
                // The torch's arc leads (#670).
                | OpType::PlasmaProfile
        ),
        // The probe's result action - set a work offset or stop for
        // the operator - is a machine-side act like the feeder's (#108).
        SEC_MACHINE => matches!(kind, OpType::Part | OpType::Probe),
        _ => false,
    }
}

/// The tab that shows for `kind` when `want` was the last one opened: the
/// same section if this kind has it (switching Contour to Pocket keeps
/// Heights open), otherwise the first tab. Reselecting the same operation
/// never reaches the fallback - its kind has the tab it showed.
pub(crate) fn resolve_tab(kind: OpType, want: usize) -> usize {
    if want < SECTIONS.len() && has_section(kind, want) {
        want
    } else {
        SEC_GEO
    }
}

/// Draw the strip for `kind` and return the section to show this frame.
/// `open` is the remembered tab (`State::op_tab`); it is rewritten to the
/// resolved tab, and to the clicked one on a click - navigation only, the
/// caller queues nothing for it.
///
/// Look: wrapped, not one row - the properties pane is ~22% wide and the
/// full names overflow it (an off-pane tab would take clicks meant for the
/// viewport, M1.4's pane-width law). The open tab wears the accent fill
/// with its on-fill text and sits on an accent rule; the rest are the
/// theme's plain buttons, hover included.
/// The glyph of one section tab, in `SECTIONS` index order. Exhaustive
/// on purpose: every index the tab bar can show names its own `sec-*`
/// master (the `op_icon_table` law, applied to the tab bar).
pub(super) fn sec_icon(i: usize) -> crate::egui_app::icons::Icon {
    use crate::egui_app::icons::Icon;
    match i {
        0 => Icon::SecGeometry,
        1 => Icon::SecHeights,
        2 => Icon::SecPasses,
        3 => Icon::SecLeads,
        4 => Icon::SecFeeds,
        _ => Icon::SecMachine,
    }
}

pub(super) fn tab_bar(ui: &mut Ui, kind: OpType, open: &mut usize, pal: &Tokens) -> usize {
    const ICON_PX: f32 = 12.0;
    const PAD_X: f32 = 6.0;
    let radius = CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 };
    let mut tab = resolve_tab(kind, *open);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        for (i, name) in SECTIONS.iter().enumerate() {
            if !has_section(kind, i) {
                continue;
            }
            let sel = tab == i;
            let ink = if sel { contrast::on_fill(pal.accent) } else { ui.visuals().text_color() };
            let font = egui::FontId::proportional(12.0);
            let galley = ui.ctx().fonts_mut(|f| f.layout_no_wrap((*name).to_owned(), font.clone(), ink));
            let w = PAD_X + ICON_PX + 3.0 + galley.size().x + PAD_X;
            let size = Vec2::new(w, 22.0);
            let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
            let painter = ui.painter();
            if sel {
                painter.rect_filled(rect, radius, pal.accent);
                painter.rect_stroke(rect, radius, egui::Stroke::new(1.0, pal.accent_hover), egui::StrokeKind::Outside);
            } else if resp.hovered() {
                painter.rect_filled(rect, radius, pal.field_hover);
            }
            if let Some(set) = crate::egui_app::icons::IconSet::from_ctx(ui.ctx())
                && let Some(tex) = set.texture(sec_icon(i))
            {
                let icon_rect = egui::Rect::from_center_size(
                    egui::Pos2::new(rect.left() + PAD_X + ICON_PX * 0.5, rect.center().y),
                    Vec2::splat(ICON_PX),
                );
                painter.image(
                    tex.id(),
                    icon_rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                    ink,
                );
            }
            painter.galley(
                egui::Pos2::new(rect.left() + PAD_X + ICON_PX + 3.0, rect.center().y - galley.size().y * 0.5),
                galley,
                ink,
            );
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, *name));
            if resp.clicked() {
                tab = i;
            }
        }
    });
    let rule = ui.available_rect_before_wrap();
    let y = rule.top();
    ui.painter().hline(rule.x_range(), y, Stroke::new(2.0, pal.accent));
    ui.add_space(4.0);
    *open = tab;
    tab
}
