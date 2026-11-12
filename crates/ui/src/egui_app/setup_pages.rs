//! A setup's own pages, opened from its browser nodes (#343): the setup's
//! settings (active radio, stock source, part alignment, WCS label), its
//! Machine and Post (own, or following the document's), its Fixtures and
//! its Tool list. Every change is a queued command - `ActivateSetup`,
//! `SetupOwnMachine` / `SetupOwnPost`, the machine and post verbs (routed
//! to the setup's own mount, `state::setup_mount`), the fixture verbs -
//! so undo walks each one back.
//!
//! The mounted machine and post are the ACTIVE setup's (the one-mount
//! law, `camcraft_app::setup_mount`): a page of an inactive setup edits
//! nothing until it is activated, so no click can change a setup the
//! page does not name. (The browser activates a setup before it opens
//! one of its nodes, so this guard only shows after a later radio click.)
//!
//! Provenance: our own page law (issue #343, the maintainer's per-node
//! behaviour); no HSMWorks material consulted.

use egui::{RichText, Ui};

use camcraft_app::{model, setup, setup_mount, tool_numbers};

use crate::state::{CommandKind, SetupPage, State, UiTarget};

use super::pages;
use super::panes::kind_name;

/// Draw the page of `name`'s `page` node (`""` - the implicit context).
pub(super) fn page(ui: &mut Ui, state: &mut State, name: &str, page: SetupPage) {
    let ctx = (!name.is_empty()).then_some(name);
    let title = ctx.unwrap_or("Setup");
    match page {
        SetupPage::Settings => {
            ui.heading(title);
            ui.separator();
            settings(ui, state, ctx);
        }
        SetupPage::Machine => {
            ui.heading(format!("Machine - {title}"));
            ui.separator();
            if editable(ui, state, ctx) {
                ownership(ui, state, ctx, Mount::Machine);
                pages::machine_section(ui, state);
            }
        }
        SetupPage::Post => {
            ui.heading(format!("Post - {title}"));
            ui.separator();
            if editable(ui, state, ctx) {
                ownership(ui, state, ctx, Mount::Post);
                pages::post_section(ui, state);
                ui.separator();
                super::setup_post_page::options_ownership(ui, state, ctx);
            }
        }
        SetupPage::Fixtures => {
            ui.heading(format!("Fixtures - {title}"));
            ui.separator();
            if editable(ui, state, ctx) {
                pages::fixtures_section(ui, state, ctx);
            }
        }
        SetupPage::Tools => {
            ui.heading(format!("Tool list - {title}"));
            ui.separator();
            tool_table(ui, state, ctx);
            super::setup_post_page::tool_list_export(ui, state, ctx);
            ui.separator();
            pages::tool_list_section(ui, state, ctx);
            pages::sheet_footer(ui, state);
        }
    }
}

fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).weak());
}

/// Whether `ctx` is the active setup (or the implicit one): the pages
/// that edit the mounted steel draw their editors only then.
fn editable(ui: &mut Ui, state: &mut State, ctx: Option<&str>) -> bool {
    let Some(name) = ctx else {
        return true;
    };
    if setup::active(&state.world).as_deref() == Some(name) {
        return true;
    }
    ui.label(format!("{name} is not the active setup - activate it to edit its machine, post and fixtures."));
    if ui.button(format!("Activate {name}")).clicked() {
        state.pending.push_back(CommandKind::ActivateSetup(Some(name.to_owned())));
    }
    false
}

#[derive(Clone, Copy)]
enum Mount {
    Machine,
    Post,
}

/// The own / follow-the-document choice of a setup's machine or post.
fn ownership(ui: &mut Ui, state: &mut State, ctx: Option<&str>, which: Mount) {
    let Some(name) = ctx else {
        hint(ui, "A document without setups uses the document's machine and post.");
        return;
    };
    let w = &state.world;
    let (owns, doc) = match which {
        Mount::Machine => (
            setup_mount::own_machine(w, Some(name)).is_some(),
            setup_mount::machine_label(&setup_mount::document_machine(w)),
        ),
        Mount::Post => (
            setup_mount::own_post(w, Some(name)).is_some(),
            camcraft_app::posting::post_ref_label(&setup_mount::document_post(w)),
        ),
    };
    let word = match which {
        Mount::Machine => "machine",
        Mount::Post => "post",
    };
    let mut pick = owns;
    ui.radio_value(&mut pick, false, format!("Use the document's {word} ({doc})"));
    ui.radio_value(&mut pick, true, format!("This setup has its own {word}"));
    if pick != owns {
        let setup = name.to_owned();
        state.pending.push_back(match which {
            Mount::Machine => CommandKind::SetupOwnMachine { setup, own: pick },
            Mount::Post => CommandKind::SetupOwnPost { setup, own: pick },
        });
    }
    hint(ui, &format!(
        "Setups with different {word}s post one at a time: a program is written by one post for one machine."
    ));
    ui.separator();
}

/// The active-setup radio's words (#866: "furniture" was our in-code
/// word for a setup's stock, WCS and fixtures - never a user's).
/// The one pointer line to the browser's Fixtures and Tool list nodes, shared
/// by the Setups page (no setups) and the implicit Settings page (#1197).
pub(crate) const WORKHOLDING_POINTER: &str = "Fixtures and tools are under Setup in the browser, not on this page.";

pub(crate) const ACTIVE_SETUP_LABEL: &str = "Active setup - new operations go into it, and the Stock and Work offset pages edit it";

/// The setup's settings: which setup is active, where its stock comes
/// from, how the part sits, and the word its WCS posts as.
fn settings(ui: &mut Ui, state: &mut State, ctx: Option<&str>) {
    let Some(name) = ctx else {
        ui.label("No setups in this document: the one stock and the one WCS act as the first setup.");
        hint(ui, WORKHOLDING_POINTER);
        if ui.button("Create first setup").clicked() {
            state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
        }
        return;
    };
    let is_active = setup::active(&state.world).as_deref() == Some(name);
    if ui.radio(is_active, ACTIVE_SETUP_LABEL).clicked() && !is_active {
        state.pending.push_back(CommandKind::ActivateSetup(Some(name.to_owned())));
    }
    ui.separator();
    super::setup_stock_pick::picker(ui, state, name);
    ui.separator();
    if let Some(def) = setup::def_of(&state.world, name) {
        let mut line = format!("Stock entity {}", def.stock);
        if !def.datum_from.is_empty() {
            line.push_str(&format!(", datum from {}", def.datum_from));
        }
        ui.label(line);
    }
    super::setup_align_pick::picker(ui, state, name);
    super::setup_relative_pick::picker(ui, state, name);
    super::setup_plane_pick::row(ui, state, name);
    super::setup_points_pick::row(ui, state, name);
    super::setup_axis_pick::rows(ui, state, name);
    super::wcs_pick_row::row(ui, state, name); // #701
    super::setup_wcs_pick::picker(ui, state, name);
    super::setup_workholding_row::row(ui, state, name);
    ui.separator();
    // Workholding is edited on the Fixtures page only (#1197): the one
    // click to it sits on the Delete line, so the page gains no row.
    ui.horizontal(|ui| {
        if ui.button(format!("Delete {name}")).clicked() {
            state.pending.push_back(CommandKind::DeleteSetup(name.to_owned()));
        }
        if ui.button(format!("Open {name} fixtures")).clicked() {
            // Activate it too, as the browser's Fixtures row does: the
            // fixture page edits only the active setup.
            super::browser_pane::focus(state, name);
            crate::actions::target(state, UiTarget::SetupNode(name.to_owned(), SetupPage::Fixtures));
        }
    });
}

/// The tools this setup's operations use, in program order: number,
/// size, kind, how many operations - and the editor one click away.
/// Tools live in the document's library (decision #343 (b)), so the note
/// says plainly that an edit reaches every setup using the tool.
fn tool_table(ui: &mut Ui, state: &mut State, ctx: Option<&str>) {
    let used = tools_of(state, ctx);
    if used.is_empty() {
        ui.label("No operation in this setup uses a tool yet.");
        return;
    }
    hint(ui, "Tools live in the document's tool library: editing one here edits it in every setup that uses it.");
    let mut open: Option<String> = None;
    egui::Grid::new("setup_tool_table").striped(true).show(ui, |ui| {
        for h in ["T", "Tool", "Size", "Ops", ""] {
            ui.label(RichText::new(h).strong());
        }
        ui.end_row();
        for (tool, ops) in &used {
            let number = tool_numbers::number_of(&state.world, tool).map_or_else(|| "?".to_owned(), |n| n.to_string());
            ui.label(number);
            ui.horizontal(|ui| {
                super::tool_icons::named_glyph(ui, &state.world, tool);
                ui.label(tool);
            });
            let size = model::find_by_name(&state.world, tool)
                .and_then(|e| model::tool_of(&state.world, &e))
                .map_or_else(|| "missing".to_owned(), |t| format!("Ø{} {}", t.diameter, kind_name(t.kind)));
            ui.label(size);
            ui.label(ops.to_string());
            if ui.small_button("Edit").clicked() {
                open = Some(tool.clone());
            }
            ui.end_row();
        }
    });
    let names: Vec<String> = used.iter().map(|(t, _)| t.clone()).collect();
    if let Some(why) = tool_numbers::clash(&state.world, &names) {
        let ink = super::colorvision::text_inks(ui, &state.world).error;
        super::error_text::error_text(ui, ink, &why);
    }
    if let Some(tool) = open {
        crate::actions::open_tool_editor(state, &tool);
    }
}

/// (tool, operation count) for the setup's operations, in program order.
fn tools_of(state: &State, ctx: Option<&str>) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for (_, e) in setup::operation_order(&state.world) {
        let Some(op) = model::operation_of(&state.world, &e) else {
            continue;
        };
        if let Some(name) = ctx
            && !setup::belongs(&state.world, op.setup.as_deref(), name)
        {
            continue;
        }
        match out.iter_mut().find(|(t, _)| *t == op.tool) {
            Some((_, n)) => *n += 1,
            None => out.push((op.tool.clone(), 1)),
        }
    }
    out
}
