//! The browser's fixture menu (#339 slice 4): a right-click on a model
//! row offers "Use as fixture" (the active setup's) or "Stop using as
//! fixture"; on a setup's Fixtures node, "Import fixture…" and the
//! fixture library's "Save fixture to library…" / "Insert from
//! library…" (#339 slice 6, `fixture_library_dialog`). Every row
//! of slice 4 queues the very command the setup's Fixtures page queues
//! (`TagFixture`, `UntagFixture`, `ImportFixtureDialog`) - one undo unit
//! each, never a world poke. Hand-drawn like the operation menu
//! (`tree_menu`): an Area at Foreground order; any other primary press
//! closes it.
//! Provenance: our own menu over our own commands; no HSMWorks material
//! consulted.

use egui::{Rect, Sense, Ui, Vec2};

use camcraft_app::{model, setup};

use crate::state::{CommandKind, SetupPage, State, TreeMenuTarget};

use super::browser_tree::Leaf;
use super::theme::Tokens;

/// The menu's rows: a header (no command), then the verbs. Empty when
/// the target no longer stands (the menu then closes).
pub(crate) fn menu_rows(state: &State, target: &TreeMenuTarget) -> Vec<(String, Option<CommandKind>)> {
    let w = &state.world;
    match target {
        TreeMenuTarget::Model { name, .. } => {
            let Some(e) = model::find_by_name(w, name).filter(|e| model::is_model(w, e)) else {
                return Vec::new();
            };
            let verb = if model::is_fixture(w, &e) {
                ("Stop using as fixture".to_owned(), CommandKind::UntagFixture(name.clone()))
            } else {
                // Workholding of the ACTIVE setup: the context every
                // setup editor works in (a click on a setup's node
                // activates it first).
                let setup = setup::active(w);
                let label = match &setup {
                    Some(s) => format!("Use as fixture ({s})"),
                    None => "Use as fixture".to_owned(),
                };
                (label, CommandKind::TagFixture { name: name.clone(), setup })
            };
            let mut rows = vec![
                ("Fixture".to_owned(), None),
                (verb.0, Some(verb.1)),
                ("Delete".to_owned(), Some(CommandKind::DeleteByName(name.clone()))),
            ];
            // #2463: a linked model offers Unlink.
            if let Some(src) = camcraft_app::model_source::of(w, &e) {
                let file = std::path::Path::new(&src.path).file_name().map_or(src.path.clone(), |f| f.to_string_lossy().into_owned());
                rows.push(("Source".to_owned(), None));
                rows.push((format!("Unlink from {file}"), Some(CommandKind::UnlinkModel(name.clone()))));
            }
            rows
        }
        TreeMenuTarget::Tool { name, .. } => {
            if model::find_by_name(w, name).and_then(|e| model::tool_of(w, &e)).is_none() {
                return Vec::new();
            }
            vec![
                ("Tool".to_owned(), None),
                ("Delete".to_owned(), Some(CommandKind::DeleteTool(name.clone()))),
            ]
        }
        TreeMenuTarget::Fixtures(s) => vec![
            ("Fixtures".to_owned(), None),
            ("Import fixture…".to_owned(), Some(CommandKind::ImportFixtureDialog)),
            // #339 slice 6: the user fixture library.
            ("Save fixture to library…".to_owned(), Some(CommandKind::FixtureLibrarySaveDialog(s.clone()))),
            ("Insert from library…".to_owned(), Some(CommandKind::FixtureLibraryInsertDialog(s.clone()))),
            // #788: a generated, parametric vise.
            ("Add built-in vise…".to_owned(), Some(CommandKind::BuiltinViseDialog(s.clone()))),
            // #789: parallels, blocks, strap clamps, soft jaws.
            ("Add accessory…".to_owned(), Some(CommandKind::AccessoryDialog(s.clone()))),
            // #819: a parametric grid plate.
            ("Add fixture plate…".to_owned(), Some(CommandKind::FixturePlateDialog(s.clone()))),
        ],
        TreeMenuTarget::Operation(_) | TreeMenuTarget::Operations | TreeMenuTarget::Entry(_) => Vec::new(),
    }
}

/// The row the menu hangs under.
fn anchor_row(target: &TreeMenuTarget, leaves: &[Leaf]) -> Option<usize> {
    match target {
        TreeMenuTarget::Model { name, row } => {
            (leaves.get(*row) == Some(&Leaf::Model(name.clone()))).then_some(*row).or_else(|| {
                leaves.iter().position(|l| *l == Leaf::Model(name.clone()))
            })
        }
        TreeMenuTarget::Fixtures(s) => leaves.iter().position(|l| *l == Leaf::Page(s.clone(), SetupPage::Fixtures)),
        TreeMenuTarget::Tool { name, row } => {
            (leaves.get(*row) == Some(&Leaf::Tool(name.clone()))).then_some(*row).or_else(|| {
                leaves.iter().position(|l| *l == Leaf::Tool(name.clone()))
            })
        }
        TreeMenuTarget::Operation(_) | TreeMenuTarget::Operations | TreeMenuTarget::Entry(_) => None,
    }
}

/// Draw the menu for a model, tool or Fixtures target; a click queues its
/// command and closes the menu.
pub(super) fn fixture_menu(ui: &mut Ui, state: &mut State, pal: &Tokens, rect: Rect, leaves: &[Leaf], target: &TreeMenuTarget) {
    let lines = menu_rows(state, target);
    let Some(ix) = anchor_row(target, leaves).filter(|_| !lines.is_empty()) else {
        state.tree_menu = None;
        return;
    };
    let g = pal.dropdown();
    let (anchor, width) = g.tree_anchor(rect, ui.ctx().content_rect(), ix, pal.row_height);
    let panel = Rect::from_min_size(anchor, Vec2::new(width, lines.len() as f32 * pal.row_height));
    let mut chosen: Option<usize> = None;
    egui::Area::new(ui.id().with(("fixture-menu", ix)))
        .fixed_pos(anchor)
        .order(egui::Order::Foreground)
        .movable(false)
        .show(ui.ctx(), |ui| {
            ui.expand_to_include_rect(panel);
            let p = ui.painter().clone();
            p.rect_filled(panel, pal.radius_menu, pal.ink);
            p.rect_stroke(panel, pal.radius_menu, egui::Stroke::new(1.0, pal.border), egui::StrokeKind::Outside);
            for (k, (label, cmd)) in lines.iter().enumerate() {
                let r = Rect::from_min_size(
                    egui::Pos2::new(anchor.x, anchor.y + k as f32 * pal.row_height),
                    Vec2::new(width, pal.row_height),
                );
                let why = cmd.as_ref().and_then(|cmd| {
                    state.browser_build.then(|| crate::browser_files::refusal(cmd, false)).flatten()
                });
                let live = cmd.is_some() && why.is_none();
                let resp = cmd.as_ref().map(|_| {
                    let resp = ui.add_enabled_ui(live, |ui| {
                        ui.interact(r, ui.id().with(("fixture-menu-row", k)), Sense::click())
                    }).inner;
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, live, label));
                    if let Some(why) = why { resp.on_disabled_hover_text(why) } else { resp }
                });
                if resp.as_ref().is_some_and(egui::Response::hovered) {
                    p.rect_filled(r, 0.0, pal.field_hover);
                }
                p.text(
                    r.left_center() + Vec2::new(g.pad_x, 0.0),
                    egui::Align2::LEFT_CENTER,
                    label,
                    egui::FontId::proportional(pal.font_row),
                    if live { pal.text } else { pal.text_dim },
                );
                if resp.is_some_and(|r| r.clicked()) {
                    chosen = Some(k);
                }
            }
        });
    if let Some(k) = chosen {
        if let Some((_, Some(cmd))) = lines.into_iter().nth(k) {
            state.pending.push_back(cmd);
        }
        state.tree_menu = None;
    } else if ui.input(|i| {
        i.pointer.button_pressed(egui::PointerButton::Primary)
            && i.pointer.hover_pos().is_none_or(|pos| !panel.contains(pos))
    }) {
        state.tree_menu = None;
    }
}
