//! #265: a refused post keeps its reason on screen.
//!
//! The status line is one shared line: the next viewport click
//! (`interact::pick_model`) or any other status writes over it, so a
//! refusal that lived only there was gone before the user could act on
//! it. The refusal is kept in [`State::post_refusal`] and shown in its own
//! window until the user closes it, a later post succeeds, or the
//! document is switched (New, Open). An edit does not close it: the
//! window says the document has changed since, so the reason is never
//! presented as current when it may not be.
//! Provenance: our own UI law (AGENTS.md: refusals are visible).

use std::path::PathBuf;

use egui::{Id, RichText, ScrollArea, Window};

use crate::state::State;

/// How tall the reason grows before it scrolls, so a refusal that names
/// many blocked moves stays a window that fits beside the viewport.
const REASON_MAX_HEIGHT: f32 = 240.0;
/// The window's opening width: a typical one-sentence refusal on two or
/// three lines.
const WINDOW_WIDTH: f32 = 460.0;

/// One refused post: the reason the gate gave, and what it judged.
#[derive(Clone, Debug)]
pub struct PostRefusal {
    /// The file the post would have written. The gate writes nothing on
    /// refusal, so a program already at this path is unchanged.
    pub path: PathBuf,
    /// The gate's reason, the same text the status line shows after
    /// "post refused: ".
    pub reason: String,
    /// [`State::doc_version`] when the post was refused. Once the
    /// document moves on, the reason may no longer hold.
    pub doc_version: u64,
}

/// The "Post refused" window: an in-app `egui::Window` over the dock,
/// closed by its ✖ like the G-code viewer. Draws nothing while no
/// refusal stands, so the shell snapshots never see it.
pub(crate) fn window(ctx: &egui::Context, state: &mut State) {
    let Some(refusal) = state.post_refusal.as_ref() else {
        return;
    };
    let error_ink = super::colorvision::palette_of(&state.world)
        .text_on(super::theme::current().ink)
        .error;
    let file = refusal.path.file_name().map_or_else(
        || refusal.path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let changed_since = refusal.doc_version != state.doc_version;
    let mut open = true;
    Window::new("Post refused")
        .id(Id::new("post-refusal"))
        .open(&mut open)
        .collapsible(false)
        .default_width(WINDOW_WIDTH)
        .show(ctx, |ui| {
            ScrollArea::vertical()
                .max_height(REASON_MAX_HEIGHT)
                .show(ui, |ui| {
                    super::error_text::error_text(ui, error_ink, &refusal.reason);
                });
            ui.label(format!("Nothing was written to {file}; a program already there is unchanged."));
            if changed_since {
                ui.label(
                    RichText::new("The document has changed since this post was refused - post again to check it.")
                        .weak(),
                );
            }
        });
    if !open {
        state.post_refusal = None;
    }
}
