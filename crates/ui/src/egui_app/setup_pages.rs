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
//! behaviour; #1515, a pick on one setup never re-mounts another); no
//! HSMWorks material consulted.

use egui::{RichText, Ui};

use camcraft_app::{setup, setup_mount};

use crate::state::{CommandKind, SetupPage, State};

use super::pages;

/// Draw the page of `name`'s `page` node (`""` - the implicit context).
pub(super) fn page(ui: &mut Ui, state: &mut State, name: &str, page: SetupPage) {
    let ctx = (!name.is_empty()).then_some(name);
    let title = ctx.unwrap_or("Setup");
    match page {
        SetupPage::Settings => {
            ui.heading(title);
            ui.separator();
            super::setup_settings_page::settings_page(ui, state, ctx);
        }
        SetupPage::Machine => {
            ui.heading(format!("Machine - {title}"));
            ui.separator();
            if editable(ui, state, ctx) {
                super::machine_page::machine_page(ui, state, ctx);
                super::setup_rotary_page::section(ui, state, ctx);
            }
        }
        SetupPage::Post => {
            ui.heading(format!("Post - {title}"));
            ui.separator();
            if editable(ui, state, ctx) {
                ownership(ui, state, ctx, Mount::Post);
                // No setups: the hint runs straight into the post, as before.
                if ctx.is_some() {
                    ui.separator();
                }
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
            super::tool_list_page::tool_list_page(ui, state, ctx);
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
pub(super) enum Mount {
    Machine,
    Post,
}

/// The own / follow-the-document choice of a setup's machine or post.
pub(super) fn ownership(ui: &mut Ui, state: &mut State, ctx: Option<&str>, which: Mount) {
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
    // #1515: say by name where a pick on this page lands - this setup's
    // own mount, or the document's (which every following setup reads).
    let target = match which {
        Mount::Machine => state.machine_target(),
        Mount::Post => state.post_target(),
    };
    ui.label(target_line(word, name, target.as_deref() == Some(name)));
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
}

/// Where a machine (`word`) or post picked on setup `name`'s page lands
/// (#1515): `own` - that setup's own, nothing else moves; else the
/// document's, named as such.
pub(super) fn target_line(word: &str, name: &str, own: bool) -> String {
    if own {
        format!("A {word} picked on this page is {name}'s own - no other setup changes.")
    } else {
        format!("A {word} picked on this page sets the document's {word} - every setup without its own uses it.")
    }
}

/// The active-setup radio's words (#866: "furniture" was our in-code
/// word for a setup's stock, WCS and fixtures - never a user's).
/// The one pointer line to the browser's Fixtures and Tool list nodes, shared
/// by the Setups page (no setups) and the implicit Settings page (#1197).
pub(crate) const WORKHOLDING_POINTER: &str = "Fixtures and tools are under Setup in the browser, not on this page.";

pub(crate) const ACTIVE_SETUP_LABEL: &str = "Active setup - new operations go into it, and the Stock and Work offset pages edit it";
