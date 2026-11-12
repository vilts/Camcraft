//! The operation page's "Cutting data" group on the Feeds tab (M13.2):
//! the document's workpiece material, the op tool's edge material and
//! flute count, and the "Suggest feeds & speeds" readout with its one
//! explicit Apply. Every write is a command - `SetWorkMaterial`,
//! `ChangeTool`, `ApplyCuttingData` - so undo puts each back; nothing
//! here overwrites a number on its own, and the Apply waits until the
//! page's own draft is applied (a suggestion never clobbers typing).
//!
//! Provenance: our own UI over the handbook formulas in
//! `camcraft_tools::cutting` (public sources named there); no vendor
//! material or text.

use egui::Ui;

use camcraft_app::cutting;
use camcraft_app::model;
use camcraft_app::tools::{ToolMaterial, WorkMaterial};
use camcraft_app::units;

use super::fields::combo;
use super::layout::{group, hint, prop_row, prop_row_hint};
use crate::state::{CommandKind, State};

/// The flute counts the combo offers (a stated odd count outside it is
/// shown as its own row, never rewritten).
const FLUTES: [u32; 7] = [1, 2, 3, 4, 5, 6, 8];

/// Draw the group for the operation `name` (the live one; a page whose
/// op is gone draws nothing).
pub(super) fn cutting_group(ui: &mut Ui, state: &mut State, name: &str) {
    let Some(op) = model::find_by_name(&state.world, name).and_then(|e| model::operation_of(&state.world, &e))
    else {
        return;
    };
    group(ui, "Cutting data", None, |ui| body(ui, state, name, &op));
}

fn body(ui: &mut Ui, state: &mut State, name: &str, op: &camcraft_app::schema::Operation) {
    let u = state.units();
    // The document's material: index 0 is "not stated".
    let mut names: Vec<&str> = vec!["Not stated"];
    names.extend(WorkMaterial::ALL.iter().map(|m| m.label()));
    let now = cutting::material_of(&state.world);
    let mut idx = now.map_or(0, |m| m as usize + 1);
    prop_row_hint(ui, "Workpiece material", "the document's material - every operation reads it", |ui| {
        combo(ui, "cut.material", &mut idx, &names)
    });
    let picked = idx.checked_sub(1).and_then(|i| WorkMaterial::ALL.get(i).copied());
    if picked != now {
        state.pending.push_back(CommandKind::SetWorkMaterial(picked));
    }

    // The op's tool: edge material and flutes, edited through ChangeTool
    // (the tool is shared by every op that names it - the hover says so).
    let tool_name = op.tool.clone();
    if let Some(tool) = model::find_by_name(&state.world, &tool_name).and_then(|t| model::tool_of(&state.world, &t)) {
        let tm_names = ["Not stated", ToolMaterial::Hss.label(), ToolMaterial::Carbide.label()];
        let mut tm_idx = tool.tool_material.map_or(0, |m| m as usize + 1);
        prop_row_hint(ui, "Tool material", &format!("edits tool '{tool_name}' - every operation using it"), |ui| {
            combo(ui, "cut.tool_material", &mut tm_idx, &tm_names)
        });
        let tm = tm_idx.checked_sub(1).and_then(|i| ToolMaterial::ALL.get(i).copied());
        // A combo, not a drag: one discrete pick is one command.
        let mut z_idx = tool.flutes.and_then(|z| FLUTES.iter().position(|f| *f == z)).map_or(0, |i| i + 1);
        let z_names: Vec<String> = std::iter::once("Not stated".to_owned())
            .chain(FLUTES.iter().map(u32::to_string))
            .chain(tool.flutes.filter(|z| !FLUTES.contains(z)).map(|z| z.to_string()))
            .collect();
        if tool.flutes.is_some_and(|z| !FLUTES.contains(&z)) {
            z_idx = z_names.len() - 1;
        }
        let z_refs: Vec<&str> = z_names.iter().map(String::as_str).collect();
        prop_row(ui, "Flutes", |ui| combo(ui, "cut.flutes", &mut z_idx, &z_refs));
        let flutes = match z_idx {
            0 => None,
            i => z_names.get(i).and_then(|n| n.parse::<u32>().ok()),
        };
        if tm != tool.tool_material || flutes != tool.flutes {
            state.pending.push_back(CommandKind::ChangeTool {
                name: tool_name.clone(),
                tool: camcraft_app::tools::Tool { tool_material: tm, flutes, ..tool.clone() },
            });
        } else {
            // #225: the spindle speed, editable as rpm or surface speed.
            // (Skipped on a frame that already queued a tool change, so
            // two ChangeTools never race on one tool.)
            super::spindle_row::row(ui, state, &tool_name, &tool);
        }
    }

    // The suggestion readout, then the one explicit Apply.
    match cutting::suggest(&state.world, name) {
        Err(why) => {
            ui.label(egui::RichText::new(format!("No suggestion: {why}")).weak());
        }
        Ok(s) => {
            let thin = s.thinning.map_or(String::new(), |f| {
                format!(", feed raised x{:.2} for radial chip thinning (hex = fz sin phi)", 1.0 / f)
            });
            // #891: a turning suggestion speaks surface speed and feed
            // per REV, read at the stock's swing diameter.
            let readout = match s.turning {
                Some(t) => format!(
                    "Vc {:.0} m/min · f {} {}/rev at Ø {} {}",
                    s.vc,
                    units::len(s.fz, u, 3),
                    u.suffix(),
                    units::len(t.diameter, u, 1),
                    u.suffix()
                ),
                None => format!("Vc {:.0} m/min · fz {} {}", s.vc, units::len(s.fz, u, 3), u.suffix()),
            };
            prop_row_hint(
                ui,
                "Suggested",
                &format!("{} ({}){thin}", s.material.label(), s.tool_material.label()),
                |ui| ui.label(readout),
            );
            let feed_now = cutting::feed_of(&op.params)
                .or_else(|| {
                    let mut q = op.params.clone();
                    camcraft_app::cutting_turn::turn_feeds_mut(&mut q).map(|(f, _)| *f)
                })
                .unwrap_or(0.0);
            let tool_now = model::find_by_name(&state.world, &tool_name).and_then(|t| model::tool_of(&state.world, &t));
            let rpm_now = tool_now.as_ref().map_or(0.0, |t| t.rpm);
            // A turning tool that already states this Vc and f/rev.
            let data_same = s.turning.is_none()
                || tool_now.and_then(|t| t.turning).is_some_and(|d| d.vc == s.vc && d.feed_per_rev == s.fz);
            prop_row(ui, "Spindle", |ui| ui.label(format!("{rpm_now:.0} -> {:.0} rpm", s.rpm)));
            prop_row(ui, "Feed", |ui| ui.label(format!("{} -> {}", units::feed(feed_now, u), units::feed(s.feed, u))));
            for n in &s.notes {
                hint(ui, n);
            }
            let dirty = op.params != state.op_ui.params || op.area != state.op_ui.area;
            let same = data_same && (feed_now - s.feed).abs() < 1e-9 && (rpm_now - s.rpm).abs() < 1e-9;
            let why = if dirty {
                Some("apply or revert the page's own edits first - a suggestion never overwrites typing")
            } else if same {
                Some("the operation already runs the suggestion")
            } else {
                None
            };
            let mut ok = ui
                .add_enabled(!dirty && !same, egui::Button::new("Apply suggestion"))
                .on_hover_text("write the suggested spindle and feed (one undoable command)");
            if let Some(w) = why {
                ok = ok.on_disabled_hover_text(w);
            }
            if ok.clicked() {
                state.pending.push_back(CommandKind::ApplyCuttingData(name.to_owned()));
            }
        }
    }
}
