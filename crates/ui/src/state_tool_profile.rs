//! The tool editor's spun-profile draft (#93): the holder of a milling
//! tool drawn as a revolved half-outline - Parametric (the holder
//! cylinder the tool's numbers state, regenerated as they change) or
//! Custom (hand-edited points that survive number edits). The draft is
//! ephemeral pane state like `ToolBodyUi`; Apply profile queues the one
//! command (`SetToolBody::spun`: a drawn body, or a clear back to the
//! cylinder), and its refusal lands on the Tool pane by name; #793's
//! library holder pick queues `SetToolBody::library` the same way. Turning
//! tools are not spun profiles and never get this draft (owner, #93).
//! Provenance: our own pane state over `app::tool_body_view`; no vendor
//! material consulted.

use std::sync::Arc;

use camcraft_app::commands::SetToolBody;
use camcraft_app::holder_library;
use camcraft_app::model;
use camcraft_app::geometry::Triangle;
use camcraft_app::spun::{SpunPoint, SpunProfile};
use camcraft_app::tools::{Tool, ToolKind};

use super::{CommandKind, ErrorScope, State};

/// The head of an Apply profile refusal, so a later valid outline can
/// retire exactly that message.
const PROFILE_REFUSED: &str = "holder profile not set: ";

/// Which section the editor's third column shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolColumn {
    #[default]
    Body,
    Profile,
}

/// The editor's profile draft for the tool it is open on.
#[derive(Clone, Default)]
pub struct ToolProfileUi {
    /// The tool the draft belongs to (the editor's `tool_ui.name`).
    pub tool: String,
    /// The third column's tab (view state only).
    pub column: ToolColumn,
    /// Custom: `points` are hand edits. Parametric: they are regenerated
    /// from the draft tool's holder numbers every frame.
    pub custom: bool,
    /// The drawn holder outline, tip-up (z, r, arc).
    pub points: Vec<SpunPoint>,
    /// The world body the draft last mirrored (identity): Apply, Undo,
    /// Redo or Open re-mirror.
    seen: Option<Arc<[Triangle]>>,
    /// The outline an Apply refused (`None` after any other outcome):
    /// once the draft moves off it and validates, the refusal retires.
    refused: Option<Vec<SpunPoint>>,
    /// #793: the library holder picked for Apply holder, and the gauge
    /// length (gauge line to nose face, mm) it would land at; #792: the
    /// projection (nose face above the tool tip, mm).
    pub library: Option<String>,
    pub gauge: f64,
    pub projection: f64,
}

/// Is `kind` a milling tool - one a spun profile describes?
#[must_use]
pub fn is_spun_kind(kind: ToolKind) -> bool {
    !matches!(kind, ToolKind::Turning | ToolKind::Groove | ToolKind::Thread | ToolKind::Form)
}

/// The parametric holder of `tool` as table points (empty when it has
/// no holder).
#[must_use]
pub fn parametric_points(tool: &Tool) -> Vec<SpunPoint> {
    SpunProfile::holder_of(tool).map(|s| s.points().to_vec()).unwrap_or_default()
}

impl State {
    /// Keep the draft on the editor's tool: a new tool, or a world body
    /// that changed under it, re-mirrors - a drawn body re-edits as
    /// Custom, anything else starts Parametric. In Parametric the points
    /// follow the draft numbers.
    pub(crate) fn sync_tool_profile_ui(&mut self) {
        let landed = self.editor_tool_body();
        let facets = landed.as_ref().map(|b| Arc::clone(&b.facets));
        let same = match (&facets, &self.tool_profile_ui.seen) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        if self.tool_profile_ui.tool != self.tool_ui.name || !same {
            let column = self.tool_profile_ui.column;
            let mut d = ToolProfileUi { tool: self.tool_ui.name.clone(), column, ..ToolProfileUi::default() };
            // The holder pick is the user's choice for this tool: it
            // survives a re-mirror (its own Apply lands a body).
            if self.tool_profile_ui.tool == self.tool_ui.name {
                d.library = self.tool_profile_ui.library.take();
                d.gauge = self.tool_profile_ui.gauge;
                d.projection = self.tool_profile_ui.projection;
            }
            if let Some(spun) = landed.and_then(|b| b.spun) {
                d.custom = true;
                d.points = spun.points().to_vec();
            }
            d.seen = facets;
            self.tool_profile_ui = d;
        }
        if !self.tool_profile_ui.custom {
            self.tool_profile_ui.points = parametric_points(&self.tool_ui.to_tool());
        }
    }

    /// Flip Parametric/Custom. Custom starts from what is drawn now (the
    /// parametric cylinder), so the hand edits begin on the tool's own
    /// holder; Parametric drops the edits (they live on in the history
    /// only once applied).
    pub(crate) fn set_profile_custom(&mut self, custom: bool) {
        let d = &mut self.tool_profile_ui;
        if d.custom == custom {
            return;
        }
        d.custom = custom;
        if custom && d.points.is_empty() {
            // No holder numbers to start from: a 20 mm collar on the
            // shank top, a shape to edit rather than an empty table.
            let t = self.tool_ui.to_tool();
            let zs = (t.flute_len + t.shank_len).max(0.0);
            let r = (t.shank_d * 0.5).max(1.0) + 5.0;
            d.points = vec![SpunPoint::line(zs, 0.0), SpunPoint::line(zs, r), SpunPoint::line(zs + 20.0, r), SpunPoint::line(zs + 20.0, 0.0)];
        }
    }

    /// Insert a point after row `i`, midway to the next (a Custom edit).
    pub(crate) fn insert_profile_point(&mut self, i: usize) {
        let d = &mut self.tool_profile_ui;
        let (Some(a), Some(b)) = (d.points.get(i).copied(), d.points.get(i + 1).copied()) else {
            return;
        };
        let mid = SpunPoint::line((a.z + b.z) * 0.5, (a.r + b.r) * 0.5);
        d.points.insert(i + 1, mid);
        d.custom = true;
    }

    /// Remove row `i` (a Custom edit); the validation names what is left.
    pub(crate) fn remove_profile_point(&mut self, i: usize) {
        let d = &mut self.tool_profile_ui;
        if i < d.points.len() {
            d.points.remove(i);
            d.custom = true;
        }
    }

    /// "Apply profile": Custom lands the drawn holder; Parametric clears
    /// any body back to the cylinder. One command either way.
    pub(crate) fn queue_apply_tool_profile(&mut self) {
        let d = &self.tool_profile_ui;
        let points = d.custom.then(|| d.points.clone());
        self.pending.push_back(CommandKind::SetToolProfile { name: self.tool_ui.name.clone(), points });
    }

    /// The command handler: validate and judge now; a refusal lands on
    /// the Tool pane by name and the draft stays for correction.
    pub(crate) fn run_set_tool_profile(&mut self, name: &str, points: Option<Vec<SpunPoint>>) {
        let clearing = points.is_none();
        self.tool_profile_ui.refused = None;
        let tried = points.clone();
        match SetToolBody::spun(&self.world, name, points) {
            Ok(command) => {
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.status = if clearing {
                    format!("tool {name}: holder back to its parametric cylinder")
                } else {
                    format!("tool {name}: holder profile set")
                };
            }
            Err(err) => {
                self.tool_profile_ui.refused = tried;
                self.refuse(ErrorScope::Tool, format!("{PROFILE_REFUSED}{err}"));
            }
        }
    }

    /// #793: pick library holder `entry` for Apply holder; the gauge
    /// length starts at the entry's own and (#792) the projection at
    /// the stored tool's stickout (flute plus shank length), the
    /// command's own default. An unknown name clears the pick. #2018: a
    /// taper's Weldon row ("BT40 Weldon") stays the pick as it is named,
    /// its bore the one for the tool's shank (resolved on Apply), and
    /// takes the taper's Weldon gauge length.
    pub(crate) fn pick_library_holder(&mut self, entry: &str) {
        let stickout = self.editor_stored_tool().map(|t| t.flute_len + t.shank_len);
        let key = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase();
        let group = holder_library::picker_rows().into_iter().find_map(|r| match &r {
            holder_library::PickerRow::Weldon { taper, bores } if key(&r.name()) == key(entry) => {
                let e = bores.first().and_then(|b| holder_library::weldon_for(taper.name, *b).ok())?;
                Some((r.name(), e))
            }
            _ => None,
        });
        let pick = group.or_else(|| holder_library::find(entry).ok().map(|e| (e.name(), e)));
        let d = &mut self.tool_profile_ui;
        match pick {
            Some((name, e)) => {
                d.library = Some(name);
                d.gauge = e.gauge_len;
                d.projection = stickout.unwrap_or(d.projection);
            }
            None => d.library = None,
        }
    }

    /// The editor's tool as the document stores it (what Apply holder
    /// judges), not the unapplied draft.
    pub(crate) fn editor_stored_tool(&self) -> Option<Tool> {
        let e = model::find_by_name(&self.world, &self.tool_ui.name)?;
        model::tool_of(&self.world, &e)
    }

    /// "Apply holder": queue the one `SetToolBody::library` for the
    /// pick (nothing when none is picked).
    pub(crate) fn queue_apply_tool_holder(&mut self) {
        let d = &self.tool_profile_ui;
        if let Some(entry) = d.library.clone() {
            let (gauge, projection) = (Some(d.gauge), Some(d.projection));
            self.pending.push_back(CommandKind::SetToolHolder { name: self.tool_ui.name.clone(), entry, gauge, projection });
        }
    }

    /// The command handler: a refusal lands on the Tool pane by name;
    /// a landed holder re-mirrors as a Custom (editable) profile.
    pub(crate) fn run_set_tool_holder(&mut self, name: &str, entry: &str, gauge: Option<f64>, projection: Option<f64>) {
        match SetToolBody::library(&self.world, name, entry, gauge, projection) {
            Ok(command) => {
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.status = format!("tool {name}: holder {entry} set");
            }
            Err(err) => self.refuse(ErrorScope::Tool, format!("holder not set: {err}")),
        }
    }

    /// The draft outline validates: an Apply refusal of a DIFFERENT
    /// earlier outline no longer describes it, so it leaves the banner
    /// (and the status line, when that still shows it). A refusal of
    /// this very outline (e.g. the tool is gone) and any other refusal
    /// stand.
    pub(crate) fn retire_profile_refusal(&mut self) {
        let d = &self.tool_profile_ui;
        if d.refused.as_ref().is_none_or(|r| *r == d.points) {
            return;
        }
        self.tool_profile_ui.refused = None;
        if !self.error_for(ErrorScope::Tool).is_some_and(|m| m.starts_with(PROFILE_REFUSED)) {
            return;
        }
        if let Some((_, msg)) = self.feedback.take()
            && self.status == msg
        {
            self.status = String::from("ready");
        }
        self.feedback_ctx = None;
    }
}
