//! The GUI's live MCP host (#183): each frame, the agent's waiting calls
//! are answered on the UI thread against the window's own document and
//! History through `camcraft_app::mcp::answer_live`, so the browser
//! shows an agent's step and Ctrl+Z undoes it like any user step.
//! Provenance: our own code over our own app seam; no HSMWorks material.

use crate::state::State;

/// Answer every waiting request. Safe to call every frame; never blocks
/// (each agent waits for its answer, so there is rarely more than one).
pub fn pump(state: &mut State) {
    let waiting: Vec<_> = state
        .mcp
        .as_ref()
        .map_or_else(Vec::new, |inbox| std::iter::from_fn(|| inbox.next()).collect());
    for request in waiting {
        answer_request(state, request, false);
    }
    let ai_waiting: Vec<_> = state
        .ai_mcp_inbox
        .as_ref()
        .map_or_else(Vec::new, |inbox| std::iter::from_fn(|| inbox.next()).collect());
    for request in ai_waiting {
        if state.ai_chat.accepts_mcp_requests() {
            answer_request(state, request, true);
        }
        // Dropping a stale request disconnects its reply channel without executing it.
    }
}

fn answer_request(state: &mut State, request: camcraft_app::mcp::Request, track_ai_turn: bool) {
    let before_entries = state.history.done_entry_ids();
    let answer = camcraft_app::mcp::answer_live(
        &state.world,
        &mut state.history,
        state.project_path.as_deref(),
        request.line(),
    );
    if track_ai_turn {
        state.ai_chat.note_mcp_execution(&before_entries, &state.history);
    }
    if let Some(tool) = &answer.tool {
        // A tool may have changed the document: every view keyed on
        // the document version refetches, the same as after an undo.
        state.doc_version += 1;
        state.status = format!("agent: {tool}");
    }
    request.answer(answer.reply);
}

/// The status strip's note while the server listens, so the user can
/// see the window is open to an agent.
#[must_use]
pub fn badge(state: &State) -> Option<String> {
    state.mcp.as_ref()?.addr().map(|addr| format!("MCP listening on {addr}"))
}
