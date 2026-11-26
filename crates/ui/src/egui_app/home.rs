//! The start page (#2384): a Home button in the top band, beside Donate,
//! and a page over the dock that lists the recent projects. A click on a
//! project opens it through Open's own command (`OpenRecent`, so the
//! unsaved-changes prompt is Open's); a gone file is greyed and names
//! itself missing. The page shows at a start with no file named, and it
//! closes by itself when the document changes (an open, New, an import,
//! any command), so a refused open or a cancelled prompt keeps it up.
//! The page and its door are view state: no command, no undo. The list
//! is the user's recent list (#662, user settings), never the project.
//! Provenance: our own page over our own recent list; the glyph is
//! Tabler's home outline (MIT, assets/ui/LICENSES.md). No HSMWorks or
//! SolidCraft material.

use camcraft_app::recent::{RecentEntry, RecentKind};
use egui::{Button, Image, Rect, RichText, Ui, Vec2};

use super::contrast;
use super::icons::{Icon, IconSet};
use super::layout::{Action, ActionKind, BarInks, action_bar};
use super::theme::Tokens;
use crate::actions;
use crate::state::{CommandKind, State};

/// The top-band button (and its AccessKit label).
pub(crate) const HOME: &str = "Home";
/// The page's heading.
pub(crate) const TITLE: &str = "Recent projects";
/// The line that stands for an empty list.
pub(crate) const NO_RECENT: &str = "No recent projects";
/// The page's actions.
pub(crate) const NEW: &str = "New project";
pub(crate) const OPEN: &str = "Open project…";
pub(crate) const BACK: &str = "Back to the project";

/// Show the page over the document as it stands now.
pub(crate) fn open(state: &mut State) {
    state.home_page = Some(state.doc_version);
}

/// Hide the page.
pub(crate) fn close(state: &mut State) {
    state.home_page = None;
}

/// The page is up: it was opened, and the document is still the one it
/// opened over. A changed document closes it here.
pub(crate) fn is_open(state: &mut State) -> bool {
    match state.home_page {
        Some(v) if v == state.doc_version => true,
        Some(_) => {
            close(state);
            false
        }
        None => false,
    }
}

/// At a start: the page shows when the command line names no file.
pub(crate) fn at_start(state: &mut State, files: &[String]) {
    if files.is_empty() {
        open(state);
    }
}

/// The text of a project's row: its name, and "(missing)" for a gone file.
pub(crate) fn row_label(e: &RecentEntry) -> String {
    if e.missing { format!("{} (missing)", e.name) } else { e.name.clone() }
}

/// A stored path as the page shows it. The list keeps canonical paths,
/// which carry the `\\?\` verbatim prefix on Windows; the page shows the
/// plain form (`\\?\UNC\server\share` as `\\server\share`). Display
/// only: the open uses the stored path.
pub(crate) fn shown_path(path: &std::path::Path) -> String {
    let s = path.display().to_string();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        s
    }
}

/// The Home button, right-aligned so that its right edge is `right`, in
/// the top band. Returns its left edge. A click shows or hides the page.
pub(super) fn button(ui: &mut Ui, state: &mut State, pal: &Tokens, icons: Option<&IconSet>, band: Rect, right: f32) -> f32 {
    let label = RichText::new(HOME).size(pal.font_label).color(pal.text);
    let button = match icons.and_then(|set| set.texture(Icon::Home)) {
        Some(texture) => Button::image_and_text(Image::new(texture).fit_to_exact_size(Vec2::splat(16.0)).tint(pal.text), label),
        None => Button::new(label),
    };
    let up = is_open(state);
    let text_w = ui
        .painter()
        .layout_no_wrap(HOME.into(), egui::FontId::proportional(pal.font_label), pal.text)
        .size()
        .x;
    let width = text_w + 24.0 + 2.0 * pal.button_padding_x;
    let rect = Rect::from_min_max(egui::pos2(right - width, band.top() + 2.0), egui::pos2(right, band.bottom() - 2.0));
    let tip = if up { "Back to the project" } else { "Recent projects and the start page" };
    if ui.put(rect, button.selected(up)).on_hover_text(tip).clicked() {
        if up { close(state) } else { open(state) }
    }
    rect.left()
}

/// What a click on the page asks for.
enum Pick {
    Recent(std::path::PathBuf),
    New,
    Open,
    Back,
}

/// The page, in place of the dock.
pub(super) fn page(ui: &mut Ui, state: &mut State, pal: &Tokens) {
    let entries = state.recent_entries(RecentKind::Project);
    let mut pick = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(pal.item_spacing_y * 4.0);
        ui.label(RichText::new(TITLE).size(pal.font_label * 1.4).strong().color(pal.text));
        ui.add_space(pal.item_spacing_y * 2.0);
        if entries.is_empty() {
            ui.label(RichText::new(NO_RECENT).color(pal.text_faint));
        }
        for e in &entries {
            ui.horizontal(|ui| {
                let row = Button::new(RichText::new(row_label(e)).size(pal.font_row)).frame(false);
                let resp = ui.add_enabled(!e.missing, row);
                let path = shown_path(&e.path);
                let clicked = resp.clicked();
                resp.on_hover_text(&path).on_disabled_hover_text(e.refusal(RecentKind::Project));
                ui.label(RichText::new(path).size(pal.font_row).color(pal.text_faint));
                if clicked {
                    pick = Some(Pick::Recent(e.path.clone()));
                }
            });
        }
        ui.add_space(pal.item_spacing_y * 2.0);
        let inks = BarInks { accent: pal.accent, on_accent: contrast::on_fill(pal.accent), danger: pal.text };
        let bar = [
            Action { label: OPEN, kind: ActionKind::Primary, enabled: true, hint: Some("Choose a project file"), why_not: None },
            Action { label: NEW, kind: ActionKind::Secondary, enabled: true, hint: Some("Start an empty project"), why_not: None },
            Action { label: BACK, kind: ActionKind::Secondary, enabled: true, hint: Some("Close this page"), why_not: None },
        ];
        pick = pick.take().or(match action_bar(ui, &bar, &inks) {
            Some(0) => Some(Pick::Open),
            Some(1) => Some(Pick::New),
            Some(2) => Some(Pick::Back),
            _ => None,
        });
    });
    match pick {
        // The page stays up until the document changes: a refused open
        // or a cancelled unsaved-changes prompt leaves it on screen.
        Some(Pick::Recent(path)) => state.pending.push_back(CommandKind::OpenRecent(RecentKind::Project, path)),
        Some(Pick::New) => actions::new_document(state),
        Some(Pick::Open) => actions::open_project(state),
        Some(Pick::Back) => close(state),
        None => {}
    }
}
