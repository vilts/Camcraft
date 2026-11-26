//! #3897 (U6 of #3663): the tool categories on the left of the tool list,
//! as in the #3663 mock-up: one category for each family of tool kinds,
//! grouped under Milling, Hole making, Turning and Other, each with the
//! count of the shown tools. A pick is view state; it filters the rows
//! as the Tool type choice does. Categories without a tool are not shown.
//! Provenance: our own grouping of our own tool kinds; no HSMWorks
//! material involved.

use egui::{RichText, Ui};

use camcraft_app::tools::{Tool, ToolKind};

use crate::state::ToolCategory;

/// The groups and their categories, in the order the column shows them.
pub(crate) const GROUPS: [(&str, &[ToolCategory]); 4] = [
    (
        "Milling",
        &[
            ToolCategory::FlatEndMills,
            ToolCategory::BallNose,
            ToolCategory::BullNose,
            ToolCategory::ChamferMills,
            ToolCategory::ThreadSlotForm,
        ],
    ),
    ("Hole making", &[ToolCategory::Drills, ToolCategory::Taps, ToolCategory::BoringBars]),
    ("Turning", &[ToolCategory::Turning]),
    ("Other", &[ToolCategory::Probes, ToolCategory::BeamAndWire]),
];

/// The label of the row that shows every category.
pub(crate) const ALL: &str = "All tools in the list";

/// The category of each tool kind. A new kind must name its category here.
#[must_use]
pub(crate) fn category_of(kind: ToolKind) -> ToolCategory {
    match kind {
        ToolKind::EndMill => ToolCategory::FlatEndMills,
        ToolKind::BallNose => ToolCategory::BallNose,
        ToolKind::BullNose => ToolCategory::BullNose,
        ToolKind::Taper => ToolCategory::ChamferMills,
        ToolKind::ThreadMill
        | ToolKind::ThreadMillSingle
        | ToolKind::TSlot
        | ToolKind::Keyseat
        | ToolKind::Dovetail
        | ToolKind::Lollipop
        // #3369: the form mill joins the form cutters of this family.
        | ToolKind::FormMill => ToolCategory::ThreadSlotForm,
        ToolKind::Drill => ToolCategory::Drills,
        ToolKind::Tap => ToolCategory::Taps,
        ToolKind::Bore => ToolCategory::BoringBars,
        ToolKind::Turning | ToolKind::Groove | ToolKind::Thread | ToolKind::Form => ToolCategory::Turning,
        ToolKind::Probe => ToolCategory::Probes,
        ToolKind::Torch | ToolKind::Waterjet | ToolKind::Electrode | ToolKind::Wire => ToolCategory::BeamAndWire,
    }
}

/// The name the column shows for a category.
#[must_use]
pub(crate) fn label(c: ToolCategory) -> &'static str {
    match c {
        ToolCategory::FlatEndMills => "Flat end mills",
        ToolCategory::BallNose => "Ball nose",
        ToolCategory::BullNose => "Bull nose",
        ToolCategory::ChamferMills => "Chamfer mills",
        ToolCategory::ThreadSlotForm => "Thread, T-slot, undercut",
        ToolCategory::Drills => "Drills",
        ToolCategory::Taps => "Taps",
        ToolCategory::BoringBars => "Boring bars",
        ToolCategory::Turning => "Inserts and blades",
        ToolCategory::Probes => "Touch probes",
        ToolCategory::BeamAndWire => "Wire, electrode, torch, jet",
    }
}

/// The column: "All tools in the list", then each group with its
/// categories that have a tool in `rows`. Gives the new pick when the
/// user clicks a row. A pick whose category has no tool goes back to all.
pub(super) fn draw(ui: &mut Ui, rows: &[(String, Tool)], picked: Option<ToolCategory>) -> Option<Option<ToolCategory>> {
    let count = |c: ToolCategory| rows.iter().filter(|(_, t)| category_of(t.kind) == c).count();
    let mut out = None;
    if picked.is_some_and(|c| count(c) == 0) {
        out = Some(None);
    }
    if row(ui, picked.is_none(), ALL, rows.len()) {
        out = Some(None);
    }
    for (group, cats) in GROUPS {
        let shown: Vec<(ToolCategory, usize)> = cats.iter().map(|c| (*c, count(*c))).filter(|(_, n)| *n > 0).collect();
        if shown.is_empty() {
            continue;
        }
        ui.add_space(4.0);
        ui.label(RichText::new(group.to_uppercase()).small().weak());
        for (c, n) in shown {
            if row(ui, picked == Some(c), label(c), n) {
                out = Some(Some(c));
            }
        }
    }
    out
}

/// One category row: the name as a selectable label, the count beside it.
fn row(ui: &mut Ui, selected: bool, text: &str, n: usize) -> bool {
    ui.horizontal(|ui| {
        let clicked = ui.selectable_label(selected, text).clicked();
        ui.weak(n.to_string());
        clicked
    })
    .inner
}

/// Does `t` pass the category pick?
#[must_use]
pub(super) fn passes(picked: Option<ToolCategory>, t: &Tool) -> bool {
    picked.is_none_or(|c| category_of(t.kind) == c)
}
