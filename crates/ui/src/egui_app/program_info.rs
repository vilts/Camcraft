//! The post page's program number and comment rows (#258): what every
//! post writes as the program's identity (the O-word, or the program
//! name on Heidenhain and SINUMERIK). Each field edits a draft held in
//! egui's memory (immediate-mode, not undoable) and commits on Enter or
//! focus loss by queueing `SetProgramNumber` / `SetProgramComment` -
//! only when the typed value differs from the document's.
//! Provenance: our own panel (issue #258).

use camcraft_app::posting;
use egui::{TextEdit, Ui};

use crate::state::{CommandKind, State};

/// The shown text of the document's program number: blank for none.
#[must_use]
pub(crate) fn number_text(number: Option<u32>) -> String {
    number.map(|n| n.to_string()).unwrap_or_default()
}

/// The two rows, under the post's Output choice.
pub(super) fn program_rows(ui: &mut Ui, state: &mut State) {
    let info = posting::program_info(&state.world);
    let shown_number = number_text(info.number);
    let shown_comment = info.comment.clone().unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label("Program number");
        if let Some(text) = field(ui, "program_number", &shown_number, 60.0, "1", "O0001 when blank (1 to 9999)") {
            state.pending.push_back(CommandKind::SetProgramNumber(text));
        }
    });
    ui.horizontal(|ui| {
        ui.label("Program comment");
        let hover = "the program's comment - its name on Heidenhain and SINUMERIK; the machine's name when blank. \
                     At most 40 characters, no ( ) % ; *";
        if let Some(text) = field(ui, "program_comment", &shown_comment, 180.0, "machine name", hover) {
            state.pending.push_back(CommandKind::SetProgramComment(text));
        }
    });
}

/// One draft field: `Some(typed)` when a commit (Enter or focus loss)
/// changed the value, else `None`. Unfocused, the field shows the
/// document's value, so an undo is seen at once.
fn field(ui: &mut Ui, salt: &str, shown: &str, width: f32, hint: &str, hover: &str) -> Option<String> {
    let id = ui.id().with(salt);
    let mut draft: String = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| shown.to_owned());
    let resp = ui
        .add(TextEdit::singleline(&mut draft).id(id).desired_width(width).hint_text(hint))
        .on_hover_text(hover);
    if resp.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, draft));
        return None;
    }
    ui.data_mut(|d| d.remove::<String>(id));
    (resp.lost_focus() && draft.trim() != shown).then_some(draft)
}
