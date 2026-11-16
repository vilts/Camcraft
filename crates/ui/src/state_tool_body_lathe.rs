//! The tool editor's lathe body draft (#361): for a lathe-held tool the
//! imported holder is placed in the turning tool frame - the model
//! directions that are radial and along the spindle, the model point
//! that is the tip, and the optional insert zone - instead of along a
//! spindle axis. Ephemeral pane state like the spindle draft beside it
//! (`state_tool_body`): picks and previews only fill it, and the one
//! `SetToolBody::lathe` Apply body queues is the only thing that reaches
//! the world; its refusal lands on the Tool pane by name. Provenance:
//! our own pane state over `app::tool_body_lathe_view`; no vendor
//! material consulted.

use std::sync::Arc;

use camcraft_app::commands::{LatheBodySpec, SetToolBody};
use camcraft_app::geometry::Triangle;
use camcraft_app::lathe_holder::LathePlacement;
use camcraft_app::lathe_insert_zone::InsertZone;
use camcraft_app::schema::ToolBody;
use camcraft_app::tool_body::BodyFrame;
use camcraft_app::tool_body_lathe_view::{self, AS_PLACED, FrameChoice};
use camcraft_app::tool_body_view::{self, BodyPick, LoadedBody};
use camcraft_app::model;

use super::tool_body_ui::BodyPickMode;
use super::{CommandKind, ErrorScope, State};

/// The lathe half of the editor's body draft.
#[derive(Clone, Debug)]
pub struct LatheDraft {
    /// The model directions that are radial and along the spindle, and
    /// the model point that is the tip.
    pub place: LathePlacement,
    /// Does the draft state an insert zone?
    pub zone_on: bool,
    /// How far the zone reaches from the tip on the negative side of
    /// each turning-frame axis (x radial, y normal, z spindle), mm.
    pub zone_toward: [f64; 3],
    /// And on the positive side.
    pub zone_away: [f64; 3],
    /// Placement preview, cached against the numbers it read.
    preview: Option<(LatheKey, Result<ToolBody, String>)>,
}

type LatheKey = (usize, [u64; 16]);

impl Default for LatheDraft {
    fn default() -> Self {
        Self { place: AS_PLACED, zone_on: false, zone_toward: [0.0; 3], zone_away: [0.0; 3], preview: None }
    }
}

impl LatheDraft {
    /// The insert zone the draft states, if it states one.
    #[must_use]
    pub fn zone(&self) -> Option<InsertZone> {
        self.zone_on.then(|| tool_body_lathe_view::zone_of_reaches(self.zone_toward, self.zone_away))
    }

    /// The draft a landed lathe body re-edits with: its facets already
    /// stand in the turning tool frame, its zone as it landed.
    fn of_landed(zone: Option<&InsertZone>) -> Self {
        let (zone_toward, zone_away) = zone.map_or(([0.0; 3], [0.0; 3]), tool_body_lathe_view::reaches_of_zone);
        Self { zone_on: zone.is_some(), zone_toward, zone_away, ..Self::default() }
    }

    fn key(&self, facets: &Arc<[Triangle]>) -> LatheKey {
        let p = &self.place;
        let mut words = [0u64; 16];
        let nums = p.radial.iter().chain(&p.along).chain(&p.tip).chain(&self.zone_toward).chain(&self.zone_away);
        for (w, v) in words.iter_mut().zip(nums) {
            *w = v.to_bits();
        }
        words[15] = u64::from(self.zone_on);
        (facets.as_ptr() as usize, words)
    }
}

/// A lathe body as Apply body states it, for the command queue. Debug
/// prints the facet count, never the mesh.
#[derive(Clone)]
pub struct LatheBodyDraft {
    pub source: String,
    pub facets: Arc<[Triangle]>,
    pub place: LathePlacement,
    pub zone: Option<InsertZone>,
    pub csys: Vec<camcraft_app::tool_body_assembly::BodyCsys>,
    pub pieces: Vec<camcraft_app::tool_body_assembly::BodyPiece>,
}

impl std::fmt::Debug for LatheBodyDraft {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LatheBodyDraft")
            .field("source", &self.source)
            .field("facets", &self.facets.len())
            .field("place", &self.place)
            .field("zone", &self.zone)
            .finish()
    }
}

impl State {
    /// Which frame(s) a body on the editor's tool is placed in, read
    /// off the tool as the document holds it (the command's own view).
    pub(crate) fn editor_body_frames(&self) -> FrameChoice {
        model::find_by_name(&self.world, &self.tool_ui.name)
            .and_then(|e| model::tool_of(&self.world, &e))
            .map_or(FrameChoice::Spindle, |t| tool_body_lathe_view::frame_choice(t.kind))
    }

    /// The lathe draft a re-mirror starts from: a lathe steel always
    /// has one; a boring bar when its landed body is a lathe body; a
    /// spun tool never.
    pub(crate) fn lathe_draft_for(&self, landed: Option<&ToolBody>) -> Option<LatheDraft> {
        let zone = landed.and_then(|b| match &b.frame {
            BodyFrame::Lathe { zone, .. } => Some(*zone),
            BodyFrame::Spindle { .. } => None,
        });
        match (self.editor_body_frames(), zone) {
            (FrameChoice::Spindle, _) | (FrameChoice::Either, None) => None,
            (_, Some(zone)) => Some(LatheDraft::of_landed(zone.as_ref())),
            (FrameChoice::Lathe, None) => Some(LatheDraft::default()),
        }
    }

    /// Does the draft's frame still suit the editor's tool? (A kind
    /// change under an open editor moves it.)
    pub(crate) fn body_draft_frame_stale(&self) -> bool {
        match self.editor_body_frames() {
            FrameChoice::Spindle => self.tool_body_ui.lathe.is_some(),
            FrameChoice::Lathe => self.tool_body_ui.lathe.is_none(),
            FrameChoice::Either => false,
        }
    }

    /// A boring bar's frame switch: place the draft in the turning tool
    /// frame (`true`) or along the spindle axis. Draft state only.
    pub(crate) fn set_body_draft_frame(&mut self, lathe: bool) {
        if self.editor_body_frames() != FrameChoice::Either || self.tool_body_ui.lathe.is_some() == lathe {
            return;
        }
        self.tool_body_ui.lathe = lathe.then(LatheDraft::default);
        self.tool_body_ui.pick = BodyPickMode::Off;
    }

    /// An imported file into a lathe draft: the mesh in its own frame,
    /// the placement back at its start (nothing in a mesh says where
    /// the spindle is).
    pub(crate) fn import_lathe_body(&mut self, loaded: LoadedBody) {
        let d = &mut self.tool_body_ui;
        d.pieces.clear();
        d.source = loaded.source;
        d.facets = Arc::from(loaded.facets);
        d.lathe = Some(LatheDraft::default());
        d.align_note = None;
        d.pick = BodyPickMode::Off;
        self.status = format!(
            "body {}: {} facets - set the radial and spindle directions and the tip, then Apply body",
            d.source,
            d.facets.len()
        );
    }

    /// The lathe draft's placement as `SetToolBody::lathe` would read
    /// it, cached; `None` with no mesh or no lathe draft.
    pub(crate) fn lathe_body_preview(&mut self) -> Option<&Result<ToolBody, String>> {
        let d = &mut self.tool_body_ui;
        if d.facets.is_empty() {
            return None;
        }
        let facets = Arc::clone(&d.facets);
        let l = d.lathe.as_mut()?;
        let key = l.key(&facets);
        if l.preview.as_ref().is_none_or(|(k, _)| *k != key) {
            l.preview = Some((key, tool_body_lathe_view::preview(&facets, &l.place, l.zone())));
        }
        l.preview.as_ref().map(|(_, r)| r)
    }

    /// A click on the preview in a pick mode fills the lathe placement
    /// (ephemeral).
    pub(crate) fn apply_lathe_body_pick(&mut self, hit: BodyPick) {
        let mode = self.tool_body_ui.pick;
        self.tool_body_ui.pick = BodyPickMode::Off;
        let u = self.units();
        let Some(l) = self.tool_body_ui.lathe.as_mut() else { return };
        let named = |n: [f64; 3]| {
            tool_body_view::axis_label(n).map_or_else(|| format!("({:.3}, {:.3}, {:.3})", n[0], n[1], n[2]), str::to_owned)
        };
        self.status = match (mode, hit.normal) {
            (BodyPickMode::Off | BodyPickMode::Axis, _) => return,
            (BodyPickMode::Tip, _) => {
                l.place.tip = hit.vertex;
                let t = hit.vertex.map(|v| camcraft_app::units::len(v, u, 3));
                format!("tip picked: ({}, {}, {}) {}", t[0], t[1], t[2], u.suffix())
            }
            (BodyPickMode::Radial, Some(n)) => {
                l.place.radial = n;
                format!("radial direction picked: {}", named(n))
            }
            (BodyPickMode::Along, Some(n)) => {
                l.place.along = n;
                format!("spindle direction picked: {}", named(n))
            }
            (BodyPickMode::Radial | BodyPickMode::Along, None) => "direction not picked: that facet has no area".into(),
        };
    }

    /// "Apply body" on a lathe draft: queue the one `SetToolBody::lathe`.
    pub(crate) fn queue_apply_lathe_tool_body(&mut self) {
        let d = &self.tool_body_ui;
        let Some(l) = d.lathe.as_ref() else { return };
        if d.facets.is_empty() {
            return;
        }
        let body = LatheBodyDraft { source: d.source.clone(), facets: Arc::clone(&d.facets), place: l.place, zone: l.zone(), csys: d.csys.clone(), pieces: d.pieces.clone() };
        self.pending.push_back(CommandKind::SetLatheToolBody { name: self.tool_ui.name.clone(), body });
    }

    /// The command handler: place, band and fit now; a refusal lands on
    /// the Tool pane by name and the draft stays for correction.
    pub(crate) fn run_set_lathe_tool_body(&mut self, name: &str, body: LatheBodyDraft) {
        let spec = LatheBodySpec { source: body.source, facets: body.facets.to_vec(), place: body.place, zone: body.zone };
        let res = if !body.pieces.is_empty() {
            SetToolBody::lathe_with_pieces(&self.world, name, spec, body.pieces)
        } else if body.csys.is_empty() {
            SetToolBody::lathe(&self.world, name, Some(spec))
        } else {
            SetToolBody::lathe_with_csys(&self.world, name, spec, body.csys)
        };
        match res {
            Ok(command) => {
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.status = format!("tool {name}: lathe body set");
            }
            Err(err) => self.refuse(ErrorScope::Tool, format!("tool body not set: {err}")),
        }
    }
}
