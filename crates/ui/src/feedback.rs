//! The refusal banner's lifecycle and the Apply button's spoken reason.
//!
//! A refusal describes one state: the operation it was about, the
//! document version it was raised against, and the page draft on screen
//! when it fired. The moment any of those moves - a command lands, undo or
//! redo runs, another operation takes the page, the user edits the draft -
//! the sentence no longer describes what is shown, and it retires (banner
//! and status strip both). The Apply hint names why the button is dark, or
//! which edited field makes the draft unusable - never a silent dead button.
//!
//! Provenance: our own UI law (AGENTS.md Prime Directive: refusals are
//! visible and current), specced from the panes' contract.

use camcraft_app::operation::{AreaControls, OpParams};

use crate::state::{ErrorScope, State, UiTarget};

/// What a refusal was about when it fired (see the module doc).
#[derive(Clone, Debug)]
pub struct FeedbackCtx {
    /// The operation page the refusal was raised on; `None` when no
    /// operation was on the page.
    pub op: Option<String>,
    /// [`State::doc_version`] at the refusal.
    pub doc_version: u64,
    /// The page draft at the refusal: its name, parameters and area.
    pub draft_name: String,
    pub draft: OpParams,
    pub draft_area: AreaControls,
}

impl State {
    /// Snapshot what an operation-scope refusal describes: the page it
    /// was raised on (a refusal about another row - the tree menu's -
    /// still shows where the user is looking until that page moves).
    pub(crate) fn feedback_ctx_for(&self, scope: ErrorScope) -> Option<FeedbackCtx> {
        if scope != ErrorScope::Operation {
            return None;
        }
        let op = match self.pane_target() {
            UiTarget::Operation(n) => Some(n),
            _ => None,
        };
        Some(FeedbackCtx {
            op,
            doc_version: self.doc_version,
            draft_name: self.op_ui.name.clone(),
            draft: self.op_ui.params.clone(),
            draft_area: self.op_ui.area.clone(),
        })
    }

    /// Whether the standing refusal still describes what is on screen.
    /// Refusals outside the operation scope keep the older law (retired
    /// by the next command that runs).
    #[must_use]
    pub fn feedback_live(&self) -> bool {
        let Some(ctx) = &self.feedback_ctx else {
            return true;
        };
        if ctx.doc_version != self.doc_version {
            return false; // a command, undo or redo moved the document
        }
        if let Some(op) = &ctx.op
            && let UiTarget::Operation(now) = self.pane_target()
            && now != *op
        {
            return false; // another operation owns the page
        }
        // The draft the user is correcting: any edit retires the old
        // sentence - it was about the values before the edit.
        self.op_ui.name == ctx.draft_name
            && self.op_ui.params == ctx.draft
            && self.op_ui.area == ctx.draft_area
    }

    /// Drop a refusal that no longer describes the page, and its mirror
    /// in the status strip (only when the strip still shows that same
    /// sentence - a newer status is never wiped).
    pub(crate) fn retire_stale_feedback(&mut self) {
        if self.feedback_live() {
            return;
        }
        if let Some((_, msg)) = self.feedback.take()
            && self.status == msg
        {
            self.status = String::from("ready");
        }
        self.feedback_ctx = None;
        self.feedback_at = None;
        self.feedback_ghost = None;
    }

    /// #261 part 2: the world spot the standing generate refusal names -
    /// what the viewport marks and the refusal line frames. `None` once
    /// that refusal is gone or no longer describes the page, so the mark
    /// can never outlive its sentence.
    #[must_use]
    pub fn refusal_focus(&self) -> Option<[f64; 3]> {
        match &self.feedback {
            Some((ErrorScope::Operation, _)) if self.feedback_live() => self.feedback_at,
            _ => None,
        }
    }

    /// #261 part 3: the moves the standing generate refusal handed back,
    /// for the viewport's diagnostic ghost. Same law as
    /// [`State::refusal_focus`]: `None` once the refusal is gone or no
    /// longer describes the page.
    #[must_use]
    pub fn refusal_ghost(&self) -> Option<&camcraft_app::machining2d::Ghost> {
        match &self.feedback {
            Some((ErrorScope::Operation, _)) if self.feedback_live() => self.feedback_ghost.as_ref(),
            _ => None,
        }
    }
}

/// The Apply Parameters button's reason, for its hover text and the line
/// under the footer: `None` when Apply will do something useful.
/// `live` is the operation as stored, `draft` the page's values.
#[must_use]
pub fn apply_hint(live: Option<&OpParams>, draft: &OpParams, unchanged: bool) -> Option<String> {
    if unchanged {
        return Some(
            "Apply is disabled: no parameter differs from the operation - edit a field first."
                .to_owned(),
        );
    }
    if let Some(why) = draft.named_refusal() {
        let end = if why.ends_with('.') { "" } else { "." };
        return Some(format!("Apply will be refused: {why}{end}"));
    }
    if draft.is_sane() {
        return None;
    }
    let kind = draft.kind();
    let fields = live.map(|l| changed_fields(l, draft)).unwrap_or_default();
    Some(if fields.is_empty() {
        format!("Apply will be refused: a value is not usable for a {kind} operation (non-finite, or a feed/step that must be positive).")
    } else {
        format!(
            "Apply will be refused: check {} - the value is not usable for a {kind} operation.",
            fields.join(", ")
        )
    })
}

/// The draft fields that differ from the stored operation, by their
/// parameter names made readable (`plunge_feed` -> "plunge feed"). Read
/// off the `Debug` tree, leaf by leaf - the parameter set has no other
/// reflection, and the edited leaves are exactly where an unusable value
/// came from when the stored operation was usable.
fn changed_fields(live: &OpParams, draft: &OpParams) -> Vec<String> {
    let a = leaves(&format!("{live:?}"));
    let b = leaves(&format!("{draft:?}"));
    let mut out: Vec<String> = Vec::new();
    for (path, value) in &b {
        if a.iter().any(|(p, v)| p == path && v == value) {
            continue;
        }
        let word = path.rsplit('.').next().unwrap_or(path).replace('_', " ");
        if !word.is_empty() && !out.contains(&word) {
            out.push(word);
        }
    }
    out
}

/// `(field.path, value)` for every scalar leaf of a `Debug` rendering.
fn leaves(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut path: Vec<String> = Vec::new();
    let mut key = String::new();
    let mut token = String::new();
    let flush = |path: &[String], key: &str, token: &mut String, out: &mut Vec<(String, String)>| {
        let t = token.trim();
        // A keyless scalar (`Some(3.0)`, a list item) belongs to the
        // field that opened the bracket around it.
        let mut segs: Vec<&str> = path.iter().map(String::as_str).filter(|s| !s.is_empty()).collect();
        if !key.is_empty() {
            segs.push(key);
        }
        if !t.is_empty() && !segs.is_empty() {
            out.push((segs.join("."), t.to_owned()));
        }
        token.clear();
    };
    for c in s.chars() {
        match c {
            ':' => {
                key = token.trim().to_owned();
                token.clear();
            }
            '{' | '(' | '[' => {
                path.push(std::mem::take(&mut key));
                token.clear();
            }
            '}' | ')' | ']' => {
                flush(&path, &key, &mut token, &mut out);
                key.clear();
                path.pop();
            }
            ',' => {
                flush(&path, &key, &mut token, &mut out);
                key.clear();
            }
            _ => token.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::leaves;

    #[test]
    fn debug_leaves_carry_their_field_paths() {
        let l = leaves("Pocket { feed: 1200.0, lead: Lead { radius: 2.0 }, top_z: Some(Height { offset: NaN }) }");
        assert!(l.contains(&("feed".into(), "1200.0".into())), "{l:?}");
        assert!(l.contains(&("lead.radius".into(), "2.0".into())), "{l:?}");
        assert!(l.contains(&("top_z.offset".into(), "NaN".into())), "{l:?}");
        let s = leaves("Pocket { stepdown: Some(3.0), climb: true }");
        assert!(s.contains(&("stepdown".into(), "3.0".into())), "{s:?}");
        assert!(s.contains(&("climb".into(), "true".into())), "{s:?}");
    }
}
