//! The "Entry positions (pre-drilled)" group on the 2D Pocket page
//! (`Lead in / Lead out` tab, #1449): one header switch over
//! `links.approach.predrilled`. Draws into the draft params; the page's
//! Apply commits them through `EditOperationParams`, so undo puts the
//! old value back. The generator owns the law: it plunges only down a
//! hole that it proved, and names every level that keeps the asked
//! entry.
//!
//! Provenance: our own UI over the general shop practice of a drilled
//! start hole for an end mill; no vendor material or text.

use egui::Ui;

use camcraft_app::links::Approach;
use camcraft_app::operation::REST_PREDRILLED;

use super::layout::{consequence, group};

/// The group title, as the tab's structural tests find it.
pub(super) const PREDRILLED_GROUP: &str = "Entry positions (pre-drilled)";

/// Draws the group. `rest` says that rest machining is on in the draft:
/// the two together are refused on Apply, and the group names it.
pub(super) fn predrilled_group(ui: &mut Ui, a: &mut Approach, rest: bool) {
    group(ui, PREDRILLED_GROUP, Some(&mut a.predrilled), |ui| {
        consequence(
            ui,
            "the tool goes straight down a hole that a drilling operation ahead of this pocket in its setup has made - only where the hole is wider than the tool with clearance and the stock shows it open to the level; every other level keeps the Entry above, named in the status",
        );
        if rest {
            consequence(ui, REST_PREDRILLED);
        }
    });
}
