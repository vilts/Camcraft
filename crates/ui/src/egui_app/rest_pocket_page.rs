//! The "Rest machining" group on the 2D Pocket page (`Passes` tab, #1815).
//!
//! Provenance: our own UI over the `RestParams` model of
//! `camcraft_app::machining2d`; no vendor material or text consulted.

use egui::Ui;

use camcraft_app::links::Approach;
use camcraft_app::machining2d::{GenError, PocketStrategy, RestParams, RestSource, rest_strategy};
use camcraft_app::operation::{POCKET_ADAPTIVE_RETIRED, REST_PREDRILLED};
use camcraft_app::units::Unit;

use super::fields::{combo, num_row};
use super::layout::{consequence, group, prop_row_hint};

fn seed_rest(tool_diameter: Option<f64>) -> RestParams {
    let previous_tool_diameter = match tool_diameter {
        Some(d) if d.is_finite() && d > 0.0 => (d * 2.0).max(1.0),
        _ => 12.0,
    };
    RestParams {
        source: RestSource::EnteredTool,
        previous_tool_diameter,
        previous_corner_radius: 0.0,
        previous_stock_to_leave: 0.0,
        allowance: 0.0,
    }
}

/// Draws the optional "Rest machining" group inside 2D Pocket's `Passes` tab.
/// Edits ride the page draft and commit via `EditOperationParams` on Apply.
pub(super) fn rest_group(
    ui: &mut Ui,
    u: Unit,
    tool_diameter: Option<f64>,
    strategy: PocketStrategy,
    approach: &Approach,
    rest: &mut Option<RestParams>,
) {
    let mut armed = rest.is_some();
    let g = group(ui, "Rest machining", Some(&mut armed), |ui| {
        if let Some(r) = rest.as_mut() {
            // #2033: which reading plans the rest area. The previous tool
            // rows are not read from the previous operations, so they hide.
            let mut from = usize::from(r.source == RestSource::PreviousOperations);
            prop_row_hint(
                ui,
                "Rest stock from",
                "entered tool: trusts the previous tool below; previous operations: reads the stock that the earlier operations of this setup leave at every cut depth",
                |ui| combo(ui, "rest_source", &mut from, &["Entered tool", "Previous operations"]),
            );
            r.source = if from == 1 { RestSource::PreviousOperations } else { RestSource::EnteredTool };
            if r.source == RestSource::EnteredTool {
                prop_row_hint(
                    ui,
                    "Prev. tool Ø",
                    "diameter of the larger tool that previously roughed this pocket; must exceed this tool's diameter",
                    |ui| num_row(ui, u, &mut r.previous_tool_diameter),
                );
                prop_row_hint(
                    ui,
                    "Prev. corner rad.",
                    "corner (bull-nose) radius of that previous tool (0 for a flat end mill)",
                    |ui| num_row(ui, u, &mut r.previous_corner_radius),
                );
                prop_row_hint(
                    ui,
                    "Prev. stock left",
                    "radial stock left on the pocket walls by the previous operation",
                    |ui| num_row(ui, u, &mut r.previous_stock_to_leave),
                );
            }
            prop_row_hint(
                ui,
                "Rest allowance",
                "grows the computed rest area outward so passes overlap the previous tool's cut",
                |ui| num_row(ui, u, &mut r.allowance),
            );
            // A legacy draft carries the global retirement refusal as well as
            // the rest-specific refusal below; Apply reports the former first.
            if strategy == PocketStrategy::Adaptive {
                consequence(ui, POCKET_ADAPTIVE_RETIRED);
            }
            if let Err(err) = rest_strategy(strategy) {
                let text = match &err {
                    GenError::Invalid(s) | GenError::Empty(s) => s.as_str(),
                    GenError::At { msg, .. } | GenError::Ghosted { msg, .. } => msg.as_str(),
                    other => &other.to_string(),
                };
                consequence(ui, text);
            }
            if approach.predrilled {
                consequence(ui, REST_PREDRILLED);
            }
        }
    });
    if g.toggled {
        *rest = if armed { Some(seed_rest(tool_diameter)) } else { None };
    }
}
