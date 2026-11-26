//! #2007: what the Properties tool preview (`tool_preview`) draws - the
//! holder bands and the steel outline - moved out of `tool_profile_page`
//! (#3914 review). #3894: a landed drawn cutter shows on every tab.
//! Frame cost (`crates/ui/AGENTS.md`): the cutter's check and its outline
//! are computed when their input changes - the draft tool value and the
//! cutter draft's points - and read from a memo on each other frame,
//! never keyed on `doc_version`. `checks` counts the computes.
//! Provenance: our own pane logic over `app::cutter_profile`,
//! `app::tool_body` and `app::tool_body_view`. No vendor material consulted.

use std::cell::{Cell, RefCell};

use camcraft_app::cutter_profile;
use camcraft_app::spun::SpunPoint;
use camcraft_app::tool_body;
use camcraft_app::tool_body_view;
use camcraft_app::tools::Tool;

use crate::state::{State, ToolColumn, is_spun_kind};

/// Holder bands `(above, len, r)` and a steel outline override.
pub(super) type PreviewInputs = (Option<Vec<(f64, f64, f64)>>, Option<Vec<(f64, f64)>>);

type Outline = Option<Vec<(f64, f64)>>;

thread_local! {
    static MEMO: RefCell<Option<(Tool, Vec<SpunPoint>, Outline)>> = const { RefCell::new(None) };
    static CHECKS: Cell<u64> = const { Cell::new(0) };
}

/// The cutter checks and outlines computed on this thread - the counter
/// of the frame-cost rule: a still frame adds none.
#[cfg(test)]
pub(crate) fn checks() -> u64 {
    CHECKS.with(Cell::get)
}

/// The drawn cutter's outline when it validates on `tool`, from the memo
/// while the tool value and the points are the ones it was computed from.
fn cutter_outline(tool: &Tool, points: &[SpunPoint]) -> Outline {
    MEMO.with(|m| {
        let mut m = m.borrow_mut();
        if let Some((t, p, out)) = m.as_ref()
            && t == tool
            && p.as_slice() == points
        {
            return out.clone();
        }
        let _span = crate::frame_cost::span("ui/properties.tool.cutter_outline");
        CHECKS.with(|c| c.set(c.get().saturating_add(1)));
        let out = cutter_profile::check(tool, points.to_vec()).ok().map(|s| s.outline());
        *m = Some((tool.clone(), points.to_vec(), out.clone()));
        out
    })
}

/// #2007: what the one to-scale preview draws. On the Profile tab a draft
/// that validates shows at once - the drawn holder as the bands the
/// collision check sweeps (#2017), the drawn cutter as the steel
/// outline. Otherwise the landed body and the landed drawn cutter. A draft
/// that does not validate shows the landed state; its refusal stands below.
pub(super) fn preview_inputs(state: &mut State, exists: bool) -> PreviewInputs {
    if !exists {
        return (None, None);
    }
    // The Body tab's draft follows the landed body each frame, as before
    // #2007, also while a drawn holder draft shows.
    state.sync_tool_body_ui();
    let tool = state.tool_ui.to_tool();
    let profile_tab = is_spun_kind(state.tool_ui.tool_kind()) && state.tool_profile_ui.column == ToolColumn::Profile;
    let geom = tool.geom();
    let bands = |p: Option<&_>| tool_body::stack(&geom, p).bands.iter().map(|q| (q.above, q.len, q.r)).collect::<Vec<_>>();
    // #3894 (#93 leftover): a landed drawn cutter shows on every tab.
    let draw_cutter = profile_tab && state.tool_cutter_ui.shown;
    let steel = if draw_cutter || state.editor_tool_cutter().is_some() {
        state.sync_tool_cutter_ui();
        cutter_outline(&tool, &state.tool_cutter_ui.points)
    } else {
        None
    };
    if !draw_cutter && profile_tab && state.tool_profile_ui.custom {
        state.sync_tool_profile_ui();
        if let Ok(s) = tool_body_view::spun_preview(&tool, &state.tool_profile_ui.points) {
            return (Some(bands(Some(&s.holder_profile()))), steel);
        }
    }
    let body = super::tool_body_page::envelope(state);
    // The drawn cutter stops at the shank top: the holder cylinder goes
    // on top of it as bands when no body is landed.
    let body = match (&steel, body) {
        (Some(_), None) => Some(bands(None)),
        (_, b) => b,
    };
    (body, steel)
}
