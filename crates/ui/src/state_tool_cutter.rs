//! The tool editor's drawn-cutter draft (#93 slice 2): a milling tool's
//! own cutter, neck and shank as a spun half-outline - Parametric (the
//! outline its numbers draw, regenerated as they change) or Custom
//! (hand-edited points that must keep agreeing with the numbers). The
//! draft is ephemeral pane state like `ToolProfileUi`; Apply cutter
//! queues the one command (`SetToolCutter`: a drawn outline, or a clear
//! back to the numbers' outline) and its refusal lands on the Tool pane
//! by name. Turning tools never get this draft (owner, #93).
//! #93 slice 3: the draft is edited by the tool's parameters too - a
//! number edit regenerates a Parametric outline and carries a Custom
//! one onto the new numbers (`cutter_fit::refit`), a neck is a
//! parameter of the outline, and an Apply whose numbers moved lands
//! numbers and outline together (`ReshapeTool`), one undo step.
//! Provenance: our own pane state over `app::tool_cutter`; no vendor
//! material consulted.

use camcraft_app::commands::{ReshapeTool, SetToolCutter};
use camcraft_app::cutter_fit::{self, Neck};
use camcraft_app::cutter_profile;
use camcraft_app::tools::Tool;
use camcraft_app::model;
use camcraft_app::spun::{SpunPoint, SpunProfile};
use camcraft_app::tool_cutter;

use super::{CommandKind, ErrorScope, State};

/// The head of an Apply cutter refusal.
pub(crate) const CUTTER_REFUSED: &str = "cutter profile not set: ";
/// The head of a parameter-edit (`ReshapeTool`) refusal.
pub(crate) const RESHAPE_REFUSED: &str = "tool not updated: ";

/// The editor's drawn-cutter draft for the tool it is open on.
#[derive(Clone, Default)]
pub struct ToolCutterUi {
    /// The tool the draft belongs to (the editor's `tool_ui.name`).
    pub tool: String,
    /// The Profile tab shows the cutter (true) or the holder (view state).
    pub shown: bool,
    /// Custom: `points` are hand edits. Parametric: they are regenerated
    /// from the draft tool's numbers every frame.
    pub custom: bool,
    /// The drawn cutter outline, tip-up (z, r, arc).
    pub points: Vec<SpunPoint>,
    /// The world cutter the draft last mirrored: Apply, Undo, Redo or
    /// Open re-mirror when it changes.
    seen: Option<SpunProfile>,
    /// The numbers the Custom points were last fitted to: a draft
    /// number edit carries them onto the new numbers.
    fitted: Option<Tool>,
    /// Why the last neck edit was refused, shown under the neck row.
    pub neck_refusal: Option<String>,
}

impl State {
    /// The drawn cutter landed on the editor's tool, if any.
    pub(crate) fn editor_tool_cutter(&self) -> Option<SpunProfile> {
        let e = model::find_by_name(&self.world, &self.tool_ui.name)?;
        tool_cutter::cutter_of(&self.world, &e)
    }

    /// The editor's tool's numbers as the document holds them.
    fn editor_tool_numbers(&self) -> Option<Tool> {
        let e = model::find_by_name(&self.world, &self.tool_ui.name)?;
        model::tool_of(&self.world, &e)
    }

    /// Do the draft numbers differ from the document's tool?
    pub(crate) fn editor_tool_changed(&self) -> bool {
        self.editor_tool_numbers().is_some_and(|t| t != self.tool_ui.to_tool())
    }

    /// Keep the draft on the editor's tool: a new tool, or a world
    /// cutter that changed under it, re-mirrors - a drawn cutter
    /// re-edits as Custom, none starts Parametric. In Parametric the
    /// points follow the draft numbers; in Custom they are carried onto
    /// them (#93 slice 3), so outline and numbers keep agreeing.
    pub(crate) fn sync_tool_cutter_ui(&mut self) {
        let landed = self.editor_tool_cutter();
        if self.tool_cutter_ui.tool != self.tool_ui.name || self.tool_cutter_ui.seen != landed {
            let shown = self.tool_cutter_ui.shown;
            let custom = landed.is_some();
            let points = landed.as_ref().map(|s| s.points().to_vec()).unwrap_or_default();
            let fitted = self.editor_tool_numbers();
            self.tool_cutter_ui =
                ToolCutterUi { tool: self.tool_ui.name.clone(), shown, custom, points, seen: landed, fitted, neck_refusal: None };
        }
        let draft = self.tool_ui.to_tool();
        let d = &mut self.tool_cutter_ui;
        if d.custom {
            if let Some(from) = d.fitted.as_ref().filter(|f| !cutter_fit::same_shape(f, &draft)) {
                d.points = cutter_fit::refit(from, &draft, &d.points);
            }
        } else {
            d.points = cutter_profile::parametric(&draft);
        }
        d.fitted = Some(draft);
    }

    /// Flip Parametric/Custom. Custom starts from the outline drawn now
    /// (the numbers' own), Parametric drops the edits.
    pub(crate) fn set_cutter_custom(&mut self, custom: bool) {
        self.tool_cutter_ui.custom = custom;
    }

    /// The neck the draft outline draws (`cutter_fit::neck_of`), if any.
    pub(crate) fn cutter_neck(&self) -> Option<Neck> {
        cutter_fit::neck_of(&self.tool_ui.to_tool(), &self.tool_cutter_ui.points)
    }

    /// Draw a neck (`Some`: the outline becomes the kind's tip, this
    /// neck and the stated shank - Custom) or drop it (`None`: the
    /// numbers' own outline - Parametric). A neck that cannot fit is
    /// refused by name and the outline stays as it was.
    pub(crate) fn set_cutter_neck(&mut self, neck: Option<Neck>) {
        let draft = self.tool_ui.to_tool();
        let d = &mut self.tool_cutter_ui;
        match neck.map(|n| cutter_fit::necked(&draft, n)) {
            None => {
                d.custom = false;
                d.points = cutter_profile::parametric(&draft);
                d.neck_refusal = None;
            }
            Some(Ok(points)) => {
                d.custom = true;
                d.points = points;
                d.neck_refusal = None;
            }
            Some(Err(e)) => d.neck_refusal = Some(format!("neck not drawn: {e}")),
        }
        d.fitted = Some(draft);
    }

    /// Insert a point after row `i`, midway to the next (a Custom edit).
    pub(crate) fn insert_cutter_point(&mut self, i: usize) {
        let d = &mut self.tool_cutter_ui;
        let (Some(a), Some(b)) = (d.points.get(i).copied(), d.points.get(i + 1).copied()) else {
            return;
        };
        d.points.insert(i + 1, SpunPoint::line((a.z + b.z) * 0.5, (a.r + b.r) * 0.5));
        d.custom = true;
    }

    /// Remove row `i` (a Custom edit); the validation names what is left.
    pub(crate) fn remove_cutter_point(&mut self, i: usize) {
        let d = &mut self.tool_cutter_ui;
        if i < d.points.len() {
            d.points.remove(i);
            d.custom = true;
        }
    }

    /// "Apply cutter": Custom lands the drawn outline; Parametric clears
    /// it back to the numbers' outline. One command either way: with
    /// the draft numbers when they moved (`ReshapeTool`, #93 slice 3),
    /// else the outline alone (`SetToolCutter`).
    pub(crate) fn queue_apply_tool_cutter(&mut self) {
        let d = &self.tool_cutter_ui;
        let points = d.custom.then(|| d.points.clone());
        let name = self.tool_ui.name.clone();
        let kind = if self.editor_tool_changed() {
            CommandKind::ReshapeTool { name, tool: self.tool_ui.to_tool(), points }
        } else {
            CommandKind::SetToolCutter { name, points }
        };
        self.pending.push_back(kind);
    }

    /// The editor's Apply for the tool's numbers: `ChangeTool`, or - when
    /// a drawn cutter is landed and the edit moves a number it is drawn
    /// to - `ReshapeTool`, the LANDED outline carried onto the new
    /// numbers, so a number edit never strands it (unapplied outline
    /// edits stay the Apply cutter button's).
    pub(crate) fn apply_tool_numbers(&self, name: &str) -> CommandKind {
        let tool = self.tool_ui.to_tool();
        let landed = model::find_by_name(&self.world, name).and_then(|e| {
            let t = model::tool_of(&self.world, &e)?;
            Some((t, tool_cutter::cutter_of(&self.world, &e)?))
        });
        match landed {
            Some((was, drawn)) if !cutter_fit::same_shape(&was, &tool) => {
                let points = cutter_fit::refit(&was, &tool, drawn.points());
                CommandKind::ReshapeTool { name: name.to_owned(), tool, points: Some(points) }
            }
            _ => CommandKind::ChangeTool { name: name.to_owned(), tool },
        }
    }

    /// The command handler: validate now; a refusal lands on the Tool
    /// pane by name and the draft stays for correction.
    pub(crate) fn run_set_tool_cutter(&mut self, name: &str, points: Option<Vec<SpunPoint>>) {
        let clearing = points.is_none();
        match SetToolCutter::new(&self.world, name, points) {
            Ok(command) => {
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.status = if clearing {
                    format!("tool {name}: cutter drawn from its numbers")
                } else {
                    format!("tool {name}: cutter profile set")
                };
            }
            Err(err) => self.refuse(ErrorScope::Tool, format!("{CUTTER_REFUSED}{err}")),
        }
    }

    /// The `ReshapeTool` handler: numbers and outline land together or
    /// not at all; a refusal lands on the Tool pane by name.
    pub(crate) fn run_reshape_tool(&mut self, name: &str, tool: Tool, points: Option<Vec<SpunPoint>>) {
        match ReshapeTool::new(&self.world, name, tool, points) {
            Ok(command) => {
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.status = format!("tool {name} updated with its cutter profile");
            }
            Err(err) => self.refuse(ErrorScope::Tool, format!("{RESHAPE_REFUSED}{err}")),
        }
    }
}
