//! The viewport interaction law (M1.4.2 M-e), moved verbatim out of the
//! then-live `app.rs` so BOTH shells of the migration drove one copy -
//! imgui's raw-winit pipeline and the egui shell's pointer events both
//! ended in these free functions. The imgui half retired at M-g; the law
//! it carried is what lives here - not a mirror.
//!
//! The law, in one breath: a press starts a gesture only while the
//! viewport is the topmost hovered window and the cursor is inside the
//! image rect; motion with Middle orbits, with Right pans, and past
//! [`CLICK_TRAVEL_PX`] travel from a held Left promotes to an orbit; a
//! Left release under that threshold picks - anchor markers first in pick
//! mode, else the routing of [`GeoPick`]; idle motion refreshes the hover
//! highlight; the wheel dollies toward the cursor. Everything persistent
//! rides `State::pending` as a command - picking queues, it never writes
//! (AGENTS.md).
//!
//! The 3D overlay (`collect_overlay` and its producers) and the screen
//! space chips belong here for the same reason: they read the camera and
//! the drafts every frame, and the two shells must show one truth.
//! Provenance: our own code, from our own imgui app (M1-era, specced from
//! public CAM conventions; see app.rs's history and ROADMAP).

use camcraft_app::geometry::feature;
use camcraft_app::geometry::{Aabb, Point3, transform_point};
use camcraft_app::geo2d::Region;
use camcraft_app::operation::{FeatureSel, GeoSel, Height};
use camcraft_app::schema::WCS_SLOTS;
use camcraft_app::units;
use camcraft_app::{model, operation, pick};
use winit::keyboard::ModifiersState;

use crate::state::{CommandKind, GeoPick, HoverHit, State};
use crate::egui_app::ground_ink::{Overlay, light_ground, overlay};

/// Keyboard modifiers as the pick law reads them - the winit state or the
/// egui `Modifiers`, converted at the shell's edge so the law itself names
/// no windowing library. Ctrl/Shift add to a feature pick, Alt in Edge
/// mode takes the one segment under the cursor.
#[derive(Clone, Copy, Default, Debug)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl From<ModifiersState> for Mods {
    fn from(m: ModifiersState) -> Self {
        Mods { ctrl: m.control_key(), shift: m.shift_key(), alt: m.alt_key() }
    }
}

/// A press-and-release under this (logical px) is a click; past it the
/// gesture was an orbit drag that the release must end, not a pick. The
/// number imgui's winit pipeline used since the drag law was written - the
/// egui arm restates the same threshold (egui's own `max_click_dist` is
/// not the law; the shells' must agree).
pub(crate) const CLICK_TRAVEL_PX: f32 = 4.0;

// ------------------------------------------------------------ gestures

/// Is the point inside the published viewport image rect? Clicks start
/// only inside; a drag begun inside continues past the edges (the caller
/// keeps the gesture, not the hit test).
pub(crate) fn inside_viewport(state: &State, p: [f32; 2]) -> bool {
    let ([x0, y0], [x1, y1]) = state.viewport_rect;
    x1 > x0 && y1 > y0 && p[0] >= x0 && p[0] <= x1 && p[1] >= y0 && p[1] <= y1
}

/// Camera ray through a logical-px viewport point; None outside a
/// laid-out image or behind the camera. Shared by picking and hover.
pub(crate) fn ray_at(state: &State, p: [f32; 2]) -> Option<([f64; 3], [f64; 3])> {
    let ([x0, y0], [x1, y1]) = state.viewport_rect;
    let (w, h) = (x1 - x0, y1 - y0);
    if w <= 1.0 || h <= 1.0 {
        return None;
    }
    let ndc = [
        (p[0] - x0) / w * 2.0 - 1.0,
        1.0 - (p[1] - y0) / h * 2.0,
    ];
    state.cam.ray_through(ndc, f64::from(w / h))
}

/// A click inside the viewport, routed by the pick mode: models,
/// feature faces, or edge chains.
pub(crate) fn pick_at(state: &mut State, p: [f32; 2], mods: Mods) {
    // Anchor picking wins over entity picking: in pick mode a click ON a
    // marker places the origin there (same command the pane grid
    // queues); a click beside one still picks normally, so the viewport
    // never stops behaving like the viewport.
    if state.wcs_ui.pick_anchor
        && let Some((k, pos)) = anchor_hit(state, p, 12.0)
    {
        state.wcs_ui.anchor_xy = k;
        state.pending.push_back(CommandKind::SetWcs {
            pos,
            roll_deg: state.wcs_ui.roll_deg,
            pitch_deg: state.wcs_ui.pitch_deg,
            yaw_deg: state.wcs_ui.yaw_deg,
        });
        return;
    }
    match state.geo_pick {
        GeoPick::Model => pick_model(state, p),
        // #692: every feature click resolves through the one pick rule.
        GeoPick::Face | GeoPick::Edge | GeoPick::Vertex => crate::feature_click::click(state, p, mods),
    }
}

/// Raycast at a logical-px point inside the viewport; queues Select
/// (None on a miss, which clears selection - standard CAM behavior).
fn pick_model(state: &mut State, p: [f32; 2]) {
    let Some((origin, dir)) = ray_at(state, p) else {
        return;
    };
    // A browser-eye hidden model is display-off, not gone: it still
    // feeds stock and paths, but it is transparent to the ray here -
    // a click selects what the user can SEE, and geometry behind a
    // hidden model is honestly pickable (filtered `first_hit_f`). A
    // stock hit is read against the machined mesh when one is drawn
    // (M13.1 owed): the click lands on what the viewer shows.
    let hit = pick::first_hit_f(&state.world, origin, dir, |name| {
        state.hidden_in_view(name)
    });
    let (hit, facet) = crate::machined::filter_hit_facet(state, origin, dir, hit);
    state.hover_mesh = facet;
    // The click also tells us the exact facet under the pointer;
    // keeping it alive so the highlight matches the selection.
    state.hover = hit
        .as_ref()
        .map(|(name, facet, _)| HoverHit {
            entity: name.clone(),
            facet: *facet,
            feature: None,
        });
    let hit = hit.map(|(name, _, _)| name);
    // Status is the feedback channel: "nothing happened" should be
    // impossible to believe when a pick always says what it did.
    state.status = match &hit {
        Some(name) => format!("selected {name}"),
        None => String::from("no model under the cursor"),
    };
    state.pending.push_back(CommandKind::Select(hit));
}

/// A Face/Edge pick while the Boundary group collects (M11.5 UX): Ctrl
/// toggles the pick in or out of the target entry, Shift adds it and
/// closes the entry around its face loop, a plain click replaces the
/// entry's picks with this one (`boundary_draft::take_pick`). A draft
/// write, never a command until Apply.
pub fn queue_boundary_pick(state: &mut State, pick: GeoSel, mods: Mods) {
    let Some(target) = state.boundary_pick else {
        queue_feature_pick(state, pick, mods.ctrl || mods.shift);
        return;
    };
    if state.op_ui.name.is_empty() {
        state.status = String::from("open an operation first - geometry picks fill the open one");
        return;
    }
    let m = crate::boundary_draft::PickMods { toggle: mods.ctrl, close: mods.shift && !mods.ctrl };
    crate::boundary_draft::take_pick(state, target, pick, m);
}

/// Refresh the hover highlight from the cursor: a filtered raycast (the
/// one pick rule in the feature modes, #692), only while the pointer is
/// over the live viewport image with no button held and no menu or popup
/// on top of it. Anywhere else the highlight clears - it must never
/// linger where the cursor isn't. In a feature mode the hover carries
/// the feature it resolves to, so the highlight draws the same face,
/// chain or point the click would pick; the rule is shared, so they
/// cannot disagree. Ephemeral view state; never a command
/// (AGENTS.md). `held` stands in for the shell's own left-press book-
/// keeping (the app keeps `left_press` OUTSIDE State, across frames).
pub(crate) fn update_hover(state: &mut State, p: [f32; 2], held: bool) {
    state.hover_mesh = None; // re-earned below by a mesh hit, or gone
    let idle_over_viewport =
        state.drag.is_none() && !held && state.viewport_hot && inside_viewport(state, p);
    if !idle_over_viewport {
        state.hover = None;
        return;
    }
    // #692: a feature mode hovers exactly what its click would take - the
    // one pick rule, the armed tool's kinds.
    if state.geo_pick != GeoPick::Model {
        crate::feature_click::hover(state, p);
        return;
    }
    let Some((origin, dir)) = ray_at(state, p) else {
        state.hover = None;
        return;
    };
    let hit = pick::first_hit_f(&state.world, origin, dir, |name| {
        state.hidden_in_view(name)
    });
    // The machined mesh, when drawn, is what a stock hover reports
    // (M13.1 owed) - never the uncut box the ray crossed. Model mode keeps
    // hover free of features: the triangle highlight is its answer.
    let (hit, facet) = crate::machined::filter_hit_facet(state, origin, dir, hit);
    state.hover_mesh = facet;
    state.hover = hit.map(|(entity, facet, _)| HoverHit { entity, facet, feature: None });
}

/// Fold one feature pick into the open operation's geometry list and
/// queue it: plain click replaces, Ctrl/Shift adds, a repeat of what is
/// already selected is refused as a no-op (the command would dedupe it
/// anyway - saying so is the honest feedback). The entity name rides along
/// in the pick, so selecting the stock's top face needs no tree selection.
/// A pure State write: it queues, the command validates, undo is the
/// history's job. The Geometry section's chip buttons queue the same
/// commands from the other end - viewport and chips are two entrances to
/// one list, pinned together in `tests/operations.rs`.
pub fn queue_feature_pick(state: &mut State, pick: GeoSel, append: bool) {
    let name = state.op_ui.name.clone();
    if name.is_empty() {
        state.status = String::from("open an operation first - geometry picks fill the open one");
        return;
    }
    // The Boundary group is collecting (M11.5): the pick fills its draft.
    if let Some(target) = state.boundary_pick {
        let mods = crate::boundary_draft::PickMods { toggle: append, close: false };
        crate::boundary_draft::take_pick(state, target, pick, mods);
        return;
    }
    let base: Vec<GeoSel> = if append {
        model::find_by_name(&state.world, &name)
            .and_then(|e| model::operation_of(&state.world, &e))
            .map_or(Vec::new(), |op| match op.src {
                operation::SrcRef::Selection(items) => items,
                _ => Vec::new(),
            })
    } else {
        Vec::new()
    };
    if base.iter().any(|g| g.entity == pick.entity && g.feature == pick.feature) {
        state.status = format!("{} of '{}' is already selected", pick.feature, pick.entity);
        return;
    }
    let mut items = base;
    items.push(pick.clone());
    state.status = format!("{} of '{}' added to {name}", pick.feature, pick.entity);
    state.pending.push_back(CommandKind::SetOperationGeometry { name, items });
}

// ------------------------------------------------- gizmos and anchors

/// Compass names for the 3x3 XY anchor grid, row-major north-to-south,
/// west-to-east (viewed from +Z: X east, Y north). Point `k` has XY
/// indices `[k % 3, 2 - k / 3]` in the box's lo/center/hi axes.
const COMPASS: [&str; 9] = ["NW", "N", "NE", "W", "C", "E", "SW", "S", "SE"];

/// Which box the anchor points live on, per the pane's explicit choice
/// (feedback: "a dropdown under WCS that states if the origin is on stock
/// or on model"), falling back honestly to the other when the chosen one
/// doesn't exist. Returns the box and which one it is, for the pane note.
///
/// #326: the model bounds are read in the ACTIVE setup's frame - the
/// setup whose offsets the pane places (`SetWcs` lands in the active
/// setup) and the frame the viewport draws the part in. An aligned
/// (flipped or turned) part's corners are its corners on the table, so
/// G54 lands on the corner the operator touches off. The stock is that
/// setup's own (setup furniture, already placed in its frame).
pub(crate) fn anchor_reference(state: &State) -> Option<(Aabb, &'static str)> {
    let active = camcraft_app::setup::active(&state.world);
    let stock = || model::stock_world_aabb_for(&state.world, active.as_deref()).map(|b| (b, "stock box"));
    let solid = || {
        camcraft_app::setup_align::solid_aabb_in(&state.world, active.as_deref()).map(|b| (b, "model bounds"))
    };
    if state.wcs_ui.anchor_source == 1 {
        solid().or_else(stock)
    } else {
        stock().or_else(solid)
    }
}

/// The nine anchor points of the current Z level, in grid order, with their
/// compass names. The pane buttons and the viewport markers both come from
/// here, so what you click and what you see are the same points by
/// construction.
pub(crate) fn anchor_points(state: &State) -> Vec<([f64; 3], &'static str)> {
    let Some((b, _)) = anchor_reference(state) else {
        return Vec::new();
    };
    let z = state.wcs_ui.anchor_z.min(2);
    (0..9)
        .map(|k| (model::anchor_point(&b, [k % 3, 2 - k / 3, z]), COMPASS[k]))
        .collect()
}

/// Nearest on-screen anchor marker to a logical-px click, within `radius`
/// px. Markers are overlay lines with no pickable geometry and no depth
/// pass, so selection is screen-space nearest-within-threshold; points
/// behind the camera never win.
pub(crate) fn anchor_hit(state: &State, p: [f32; 2], radius: f32) -> Option<(usize, [f64; 3])> {
    let ([x0, y0], [x1, y1]) = state.viewport_rect;
    let (w, h) = (x1 - x0, y1 - y0);
    if w <= 1.0 || h <= 1.0 {
        return None;
    }
    let mvp = state.cam.mvp(f64::from(w) / f64::from(h));
    let pts = anchor_points(state);
    let mut best: Option<(f32, usize)> = None;
    for (k, (pt, _)) in pts.iter().enumerate() {
        let Some(uv) = project_uv(&mvp, *pt) else {
            continue;
        };
        let dx = x0 + uv[0] * w - p[0];
        let dy = y0 + uv[1] * h - p[1];
        let d2 = dx * dx + dy * dy;
        if d2 <= radius * radius && best.is_none_or(|(bd, _)| d2 < bd) {
            best = Some((d2, k));
        }
    }
    best.map(|(_, k)| (k, pts[k].0))
}

/// Clip-projection of a world point through the column-major `mvp` to
/// viewport uv (x right, y down from the top edge); None behind the camera
/// or at degenerate `w`. Shared by the gizmo labels and anchor picking.
pub(crate) fn project_uv(mvp: &[[f32; 4]; 4], p: [f64; 3]) -> Option<[f32; 2]> {
    // Column-major mvp: clip_i = sum_c mvp[c][i] * v_c.
    let v = [p[0] as f32, p[1] as f32, p[2] as f32, 1.0f32];
    let mut clip = [0.0f32; 4];
    for (i, out_i) in clip.iter_mut().enumerate() {
        *out_i = mvp[0][i] * v[0] + mvp[1][i] * v[1] + mvp[2][i] * v[2] + mvp[3][i] * v[3];
    }
    let w = clip[3];
    if !w.is_finite() || w <= 1e-6 {
        return None; // behind the camera or degenerate
    }
    let (ndc_x, ndc_y) = (clip[0] / w, clip[1] / w);
    if !ndc_x.is_finite() || !ndc_y.is_finite() {
        return None;
    }
    Some([(ndc_x + 1.0) * 0.5, (1.0 - ndc_y) * 0.5])
}

/// One origin gizmo to draw: a triad at `pos`/`rot`; the dimmed variant is
/// the neutral world origin shown while no work offset claims that corner.
pub(crate) struct Gizmo {
    pub pos: [f64; 3],
    pub rot: [f64; 4],
    /// Triad axes sit on X/Y/Z at this exact position with no rotation.
    pub world_axes: bool,
    /// #727: whose origin this is - the work offset's entity name as the
    /// Setup pane shows it (`G54`, `Setup2/WCS`), or `World` for the
    /// dimmed neutral axes. Drawn beside the origin so whose-zero is
    /// whose reads at a glance, with no hover.
    pub name: String,
}

/// The axis inks on a light viewport (#698): the dark set washes out on
/// paper (the blue tip sat near 2:1 on the light ground).
pub(crate) const AXES_LIGHT: [([f32; 3], [f64; 3]); 3] = [
    ([0.75, 0.12, 0.10], [1.0, 0.0, 0.0]),
    ([0.10, 0.55, 0.20], [0.0, 1.0, 0.0]),
    ([0.10, 0.35, 0.85], [0.0, 0.0, 1.0]),
];

/// The axis set the live viewport ground reads (see [`AXES`], [`AXES_LIGHT`]).
pub(crate) fn axes() -> [([f32; 3], [f64; 3]); 3] {
    if light_ground(crate::egui_app::theme::current().viewport) { AXES_LIGHT } else { AXES }
}

/// Axis colors and tip offsets shared by the line overlay and the labels.
const AXES: [([f32; 3], [f64; 3]); 3] = [
    ([0.90, 0.25, 0.20], [1.0, 0.0, 0.0]),
    ([0.25, 0.82, 0.35], [0.0, 1.0, 0.0]),
    // #195: the old 0.28/0.48/0.95 blue all but vanished on the dark
    // viewport ground once dimmed for the world axes.
    ([0.38, 0.64, 1.0], [0.0, 0.0, 1.0]),
];

/// How much the neutral world axes are dimmed against a work offset's
/// triad (#195: was 0.55 for the lines - too dark to read).
const WORLD_DIM: f32 = 0.8;

/// #727: the name the dimmed world axes carry, and the gray they wear it
/// in - neutral, because no axis color belongs to the world as a whole.
const WORLD_LABEL: &str = "World";
const WORLD_GRAY: f32 = 0.75;

/// #727: how far a gizmo's name sits below its projected origin, in uv
/// (same currency as `axis_note::NOTE_DROP_UV`) - enough to clear the
/// axis lines that meet at the origin, close enough to read as the
/// origin's own tag.
const NAME_DROP_UV: f32 = 0.02;

/// Screen-space stroke widths of a gizmo axis, px (#195): the coloured
/// line over a dark outline that keeps it readable on any ground.
pub(crate) const GIZMO_STROKE_PX: f32 = 3.0;
pub(crate) const GIZMO_OUTLINE_PX: f32 = 5.0;

const AXIS_NAMES: [&str; 3] = ["X", "Y", "Z"];

/// Triad extent for the current document (never smaller than 10 mm).
fn gizmo_len(state: &State) -> f64 {
    let extent = model::solid_world_aabb(&state.world)
        .map(|b| {
            (b.max.x - b.min.x)
                .max(b.max.y - b.min.y)
                .max(b.max.z - b.min.z)
        })
        .unwrap_or(0.0);
    // #195: shorter - the triad marks the origin, it does not span the part.
    (extent * 0.15).max(8.0)
}

/// Reference-geometry overlay: world origin axes (dimmed) plus one colored
/// triad per defined work offset. Length scales with the model extent, with
/// a floor so an empty or tiny document still shows orientation.
///
/// Which origin gizmos to draw (feedback 6): a triad per defined work
/// offset, plus the dimmed world axes - unless some triad already sits
/// exactly on the world origin, in which case its own lines are the world
/// axes and drawing both reads as a duplicated gizmo, or the Models panel
/// has the axes hidden: two gizmos around a moved G5x read as clutter, and
/// the world axes are context furniture, not setup. The triads stay: they
/// are the work offsets themselves.
pub(crate) fn gizmos(state: &State) -> Vec<Gizmo> {
    let mut out: Vec<Gizmo> = Vec::new();
    for (name, entity) in model::named_entities(&state.world) {
        if state.hidden_offsets.contains(&name) || !model::is_wcs(&state.world, &entity) {
            continue;
        }
        let Some(stored) = model::transform_of(&state.world, &entity) else {
            continue;
        };
        // #558: another setup's WCS is a point on the part in ITS setup's
        // frame; the viewport shows the active setup's, so it rides the
        // part's transform between the two. A refused transform hides
        // the triad - never drawn at a place that is not on the part.
        let loc = match camcraft_app::setup_relative::wcs_view_pose(&state.world, &name, stored) {
            Ok(mapped) => mapped.unwrap_or(stored),
            Err(_) => continue, // app logs why (setup_relative::wcs_view_pose)
        };
        out.push(Gizmo {
            pos: [loc.pos.x, loc.pos.y, loc.pos.z],
            rot: loc.rot,
            world_axes: false,
            name: name.clone(),
        });
    }
    let claims_origin = out.iter().any(|g| {
        let on_spot = g.pos[0].abs() < 1e-3 && g.pos[1].abs() < 1e-3 && g.pos[2].abs() < 1e-3;
        // Unit quaternion up to sign: [0,0,0,±1] is identity rotation.
        let unrotated = g.rot[0].abs() < 1e-6
            && g.rot[1].abs() < 1e-6
            && g.rot[2].abs() < 1e-6
            && (1.0 - g.rot[3].abs()).abs() < 1e-6;
        on_spot && unrotated
    });
    if !claims_origin && state.show_world_axes {
        out.push(Gizmo {
            pos: [0.0, 0.0, 0.0],
            rot: [0.0, 0.0, 0.0, 1.0],
            world_axes: true,
            name: WORLD_LABEL.to_string(),
        });
    }
    out
}

// ------------------------------------------------- screen-space layers

/// Screen-space segments of the FEATURE under the cursor - in the viewport,
/// or on a Geometry-list row (`state.geo_hover`) - as viewport-uv
/// pairs (0..1 within the image, like `overlay_labels` - each shell maps
/// them through the image rect and draws with its own draw list). A
/// draw-list producer on purpose: hovering must never rebuild the 3D
/// overlay buffer, and the projection tracks camera moves without a new
/// raycast. A face lights its boundary loops, a chain its polyline; the
/// per-segment projection drops any part behind the camera rather than
/// the whole feature, so a ring half off-screen still draws honestly.
/// Gone entities, hidden models, deleted features and stale ids draw
/// nothing - a highlight must never promise a pick the click cannot make.
pub(crate) fn hover_features(state: &State, aspect: f64) -> Vec<([f32; 2], [f32; 2])> {
    let mut out = Vec::new();
    let mut viewport: Option<(String, FeatureSel)> = None;
    if let Some(hit) = &state.hover
        && let Some(sel) = hit.feature
    {
        feature_segments(state, &hit.entity, sel, aspect, &mut out);
        viewport = Some((hit.entity.clone(), sel));
    }
    // The Geometry list's row hover lights the same feature the same way:
    // the list answering "which one is that?" without a click. Viewport
    // hover keeps precedence, and one feature never draws twice.
    if let Some(g) = &state.geo_hover
        && !viewport.as_ref().is_some_and(|(e, f)| *e == g.entity && *f == g.feature)
    {
        feature_segments(state, &g.entity, g.feature, aspect, &mut out);
    }
    out
}

/// One (entity, feature) resolved to viewport-uv segments: gone entities,
/// hidden models, deleted features and stale ids draw nothing.
pub(crate) fn feature_segments(
    state: &State,
    entity: &str,
    sel: FeatureSel,
    aspect: f64,
    out: &mut Vec<([f32; 2], [f32; 2])>,
) {
    if state.hidden_in_view(entity) {
        return;
    }
    let Some(found) = model::find_by_name(&state.world, entity) else {
        return;
    };
    let Some(features) = model::features_of(&state.world, &found) else {
        return;
    };
    let loc = camcraft_app::setup_align::view_pose(&state.world, &found).unwrap_or_default(); // #326
    let mvp = state.cam.mvp(aspect);
    match sel {
        FeatureSel::Face(fid) => {
            let Some(face) = features.faces.iter().find(|f| f.id == fid) else {
                return; // stale id: the model re-meshed under the hover
            };
            for l in &face.loops {
                let pts: Vec<[f64; 3]> = l
                    .verts
                    .iter()
                    .filter_map(|&vi| features.verts.get(vi as usize).copied())
                    .collect();
                push_poly(&mvp, &loc, &pts, !matches!(l.kind, feature::LoopKind::Open), out);
            }
        }
        FeatureSel::Chain(cid) => {
            let Some(chain) = feature::resolve_chain(&features, cid) else {
                return;
            };
            let pts = feature::chain_points(&features, &chain);
            push_poly(&mvp, &loc, &pts, chain.closed, out);
        }
        FeatureSel::Patch(pid) => {
            let Some(rings) = feature::patch_rings(&features, pid) else {
                return; // stale id: the model re-meshed under the hover
            };
            for r in &rings {
                let pts: Vec<[f64; 3]> = r
                    .verts
                    .iter()
                    .filter_map(|&vi| features.verts.get(vi as usize).copied())
                    .collect();
                push_poly(&mvp, &loc, &pts, r.closed, out);
            }
        }
        FeatureSel::Vertex(vid) => {
            let Some(v) = feature::resolve_vertex(&features, vid).and_then(|vi| features.verts.get(vi as usize))
            else {
                return; // stale id: the model re-meshed under the hover
            };
            let w = transform_point(Point3::new(v[0], v[1], v[2]), loc.pos, loc.rot);
            crate::vertex_pick::vertex_marker(&mvp, [w.x, w.y, w.z], aspect, out);
        }
    }
}

/// Project a local-space polyline (feature vertices, entity transform
/// applied) to viewport-uv segments, closing the ring when it is a loop.
/// A vertex behind the camera drops only the segments that touch it.
fn push_poly(
    mvp: &[[f32; 4]; 4],
    loc: &camcraft_app::schema::Location,
    pts: &[[f64; 3]],
    closed: bool,
    out: &mut Vec<([f32; 2], [f32; 2])>,
) {
    let uv = |p: &[f64; 3]| {
        let w = transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
        project_uv(mvp, [w.x, w.y, w.z])
    };
    let n = pts.len();
    if n < 2 {
        return;
    }
    let last = if closed { n } else { n - 1 };
    for i in 0..last {
        let (a, b) = match (uv(&pts[i]), uv(&pts[(i + 1) % n])) {
            (Some(a), Some(b)) => (a, b),
            _ => continue,
        };
        if a != b {
            out.push((a, b));
        }
    }
}

/// Screen-space labels for the gizmos just drawn (feedback 7): each axis
/// tip gets its letter, projected through the same camera math. #727:
/// each gizmo also wears its name just under its origin - the work
/// offset's entity name, or `World` for the dimmed neutral axes. Entries
/// are (uv within the viewport image, linear rgb, text); tips behind the
/// camera or at degenerate `w` are skipped.
pub(crate) fn overlay_labels(state: &State, aspect: f64) -> Vec<([f32; 2], [f32; 3], String)> {
    let len = gizmo_len(state);
    let mvp = state.cam.mvp(aspect);
    let mut out = Vec::new();
    // Corner tags for the height planes - "Operation1 · Clearance 25.00"
    // - one per plane, including the region plane when the orange outline
    // carries it. The operation's name rides along: with two ops open in
    // one session the plane set must say whose heights these are. Screen
    // space like the axis letters, so they track the camera for free and
    // never rebuild the 3D buffer. A corner that projects outside the
    // frame (a tight zoom leaves the padded rectangle's far corners)
    // clamps to a margin instead of dragging its label off-screen: the
    // plane's level and value stay readable, and `project_uv` itself is
    // untouched - picking shares it and must keep true positions.
    let planes = height_planes(state);
    if !planes.is_empty()
        && let Some(rect) = plane_rect(state)
    {
        for p in &planes {
            let (cx, cy) = plane_corner(rect, p.corner);
            if let Some(uv) = project_uv(&mvp, [cx, cy, p.z]) {
                const M: f32 = 0.02; // uv margin: pixels, whichever way the viewport measures
                let uv = [uv[0].clamp(M, 1.0 - M), uv[1].clamp(M, 1.0 - M)];
                // A reference plane names its source so the number on
                // screen says what it rides on: "Operation1 · Bottom
                // 22.00 (G54 +0.00)". Fixed heights are their own
                // number and carry no parenthetical.
                let u = state.units();
                let chip = if p.src.is_fixed() {
                    format!("{} · {} {}", state.op_ui.name, p.name, units::len(p.z, u, 2))
                } else {
                    // A work-offset plane names the setup's WCS and its
                    // word (#329, #342) - the setup context
                    // `height_planes` resolved it in.
                    let source = match p.src.reference {
                        camcraft_app::operation::HeightReference::WorkOffset => format!(
                            "WCS {}",
                            WCS_SLOTS
                                .get(camcraft_app::setup::wcs_slot(&state.world, shown_setup(state).as_deref()))
                                .copied()
                                .unwrap_or("G54")
                        ),
                        r => r.to_string(),
                    };
                    format!(
                        "{} · {} {} ({} {})",
                        state.op_ui.name,
                        p.name,
                        units::len(p.z, u, 2),
                        source,
                        units::len_signed(p.src.offset, u, 2),
                    )
                };
                out.push((uv, p.rgb, chip));
            }
        }
    }
    // #375: an inverted machine axis is noted once, on the first triad
    // (the setup's own offset before the world axes) - the arrow is the
    // program frame and is never flipped.
    let mut notes = crate::axis_note::triad_notes(state);
    for gizmo in gizmos(state) {
        let base = Point3::new(gizmo.pos[0], gizmo.pos[1], gizmo.pos[2]);
        // #727: the gizmo's own name, at its origin - whose zero this is,
        // readable without a hover. Emitted before the letters so each
        // name leads its gizmo's labels. The theme's Z-axis blue for
        // offsets (via `axes()`, so it lifts for the light ground like
        // the triad does); neutral gray dimmed like the world axes it
        // stands for.
        // #645: names only while the user's "Show origin names" is on -
        // the letters below never depend on it, and no line does.
        if state.show_gizmo_names
            && let Some(uv) = project_uv(&mvp, gizmo.pos)
        {
            let c = if gizmo.world_axes {
                [WORLD_GRAY * WORLD_DIM; 3]
            } else {
                axes()[2].0
            };
            out.push(([uv[0], uv[1] + NAME_DROP_UV], c, gizmo.name.clone()));
        }
        for (axis, (rgb, dir)) in axes().iter().enumerate() {
            let p = transform_point(
                Point3::new(
                    dir[0] * len * 1.08,
                    dir[1] * len * 1.08,
                    dir[2] * len * 1.08,
                ),
                base,
                gizmo.rot,
            );
            let Some(uv) = project_uv(&mvp, [p.x, p.y, p.z]) else {
                continue;
            };
            let c = if gizmo.world_axes {
                [rgb[0] * WORLD_DIM, rgb[1] * WORLD_DIM, rgb[2] * WORLD_DIM]
            } else {
                *rgb
            };
            out.push((uv, c, AXIS_NAMES[axis].to_string()));
            if let Some(note) = notes.get_mut(axis).and_then(Option::take) {
                out.push(([uv[0], uv[1] + crate::axis_note::NOTE_DROP_UV], c, note));
            }
        }
    }
    out
}

/// The origin gizmos as screen-space segments (#195): (uv start, uv end,
/// linear rgb) per axis, for the shell to stroke at [`GIZMO_STROKE_PX`]
/// over the 1 px GPU lines of [`collect_overlay`] (a wgpu line has no
/// width). An axis with an end behind the camera is skipped.
pub(crate) fn gizmo_segments(state: &State, aspect: f64) -> Vec<([f32; 2], [f32; 2], [f32; 3])> {
    let len = gizmo_len(state);
    let mvp = state.cam.mvp(aspect);
    let mut out = Vec::new();
    for gizmo in gizmos(state) {
        let base = Point3::new(gizmo.pos[0], gizmo.pos[1], gizmo.pos[2]);
        let Some(o) = project_uv(&mvp, gizmo.pos) else {
            continue;
        };
        for (rgb, dir) in &axes() {
            let p = transform_point(Point3::new(dir[0] * len, dir[1] * len, dir[2] * len), base, gizmo.rot);
            let Some(tip) = project_uv(&mvp, [p.x, p.y, p.z]) else {
                continue;
            };
            let c = if gizmo.world_axes {
                [rgb[0] * WORLD_DIM, rgb[1] * WORLD_DIM, rgb[2] * WORLD_DIM]
            } else {
                *rgb
            };
            out.push((o, tip, c));
        }
    }
    out
}

// ------------------------------------------------------ the ground grid

/// The viewport's clear colour (viewport.rs's `LoadOp::Clear`): the grid
/// fades toward it at its rim, so the floor never ends on a hard edge.
const VIEWPORT_BG: [f32; 3] = [0.09, 0.10, 0.12];

/// Minor / major ground-grid ink, a step above [`VIEWPORT_BG`] so the floor
/// reads as a surface without competing with the part or the toolpath.
const GRID_MINOR: [f32; 3] = [0.16, 0.17, 0.20];
const GRID_MAJOR: [f32; 3] = [0.23, 0.25, 0.29];

/// Grid lines from the camera target to the rim, per axis. The rim sits
/// beyond what the default framing shows, so no hard edge is ever on
/// screen; the outermost rings fade into the ground (issue: an empty
/// viewport read as a void).
const GRID_HALF_STEPS: i64 = 24;
/// Beyond this (mm) the grid is not drawn: far past any machine table, and
/// where `step * GRID_HALF_STEPS` can overflow to infinity and stretch the
/// line-index range to the whole of i64.
const GRID_MAX_COORD: f64 = 1.0e9;
/// Every fifth line is a major one (0.5, 1, 5, 10, 50 ... on a 1-2-5 step).
const GRID_MAJOR_EVERY: i64 = 5;

/// The Z=0 ground grid: the machine table plane under the part, one line
/// per 1-2-5 step of `dist / 10`, centred on the camera target and snapped
/// to round machine coordinates so the spacing reads in whole millimetres
/// at any zoom. World-space geometry in the overlay buffer, not a
/// screen-space overlay, so the part occludes it like a real table.
///
/// The same switch as the triads (`show_world_axes`): the grid is
/// reference furniture, and hiding the axes hides it too. It fades toward
/// the viewport ground at the rim rather than stopping on a rectangle.
pub(crate) fn ground_grid_lines(state: &State) -> Vec<[f32; 6]> {
    if !state.show_world_axes {
        return Vec::new();
    }
    let step = nice_step(state.cam.dist / 10.0);
    let (cx, cy) = (state.cam.target[0], state.cam.target[1]);
    // A non-finite or absurdly far target (or a non-finite step) would
    // turn the index range below into billions of lines; the grid is
    // furniture, so it simply stays out of that frame (Prime Directive:
    // no hang).
    if !(cx.is_finite() && cy.is_finite() && step.is_finite())
        || cx.abs() > GRID_MAX_COORD
        || cy.abs() > GRID_MAX_COORD
        || step > GRID_MAX_COORD
    {
        return Vec::new();
    }
    let reach = step * GRID_HALF_STEPS as f64;
    let (k0x, k1x) = (
        ((cx - reach) / step).floor() as i64,
        ((cx + reach) / step).ceil() as i64,
    );
    let (k0y, k1y) = (
        ((cy - reach) / step).floor() as i64,
        ((cy + reach) / step).ceil() as i64,
    );
    // The line nearest the target is the fade's centre.
    let (cix, ciy) = ((cx / step).round() as i64, (cy / step).round() as i64);
    let mut out: Vec<[f32; 6]> = Vec::new();
    // Lines of constant y run along X, and the other way round.
    for k in k0y..=k1y {
        let y = k as f64 * step;
        let c = grid_ink(k, ciy);
        push_line(&mut out, Point3::new(cx - reach, y, 0.0), Point3::new(cx + reach, y, 0.0), c);
    }
    for k in k0x..=k1x {
        let x = k as f64 * step;
        let c = grid_ink(k, cix);
        push_line(&mut out, Point3::new(x, cy - reach, 0.0), Point3::new(x, cy + reach, 0.0), c);
    }
    out
}

/// The ink of the grid line at index `k` (round machine coordinates, so a
/// zoomed camera keeps landing on 0, 5, 10 ...), faded toward the viewport
/// ground for the lines near the rim, `centre` being the target's index.
fn grid_ink(k: i64, centre: i64) -> [f32; 3] {
    let base = if k.rem_euclid(GRID_MAJOR_EVERY) == 0 {
        GRID_MAJOR
    } else {
        GRID_MINOR
    };
    // Cube the falloff: only the last few rings dim visibly.
    let d = (k - centre).unsigned_abs() as f32 / GRID_HALF_STEPS as f32;
    let fade = (1.0 - d * d * d).clamp(0.0, 1.0);
    [
        VIEWPORT_BG[0] + (base[0] - VIEWPORT_BG[0]) * fade,
        VIEWPORT_BG[1] + (base[1] - VIEWPORT_BG[1]) * fade,
        VIEWPORT_BG[2] + (base[2] - VIEWPORT_BG[2]) * fade,
    ]
}

/// The next 1-2-5 decade at or above `v` (0.1, 0.2, 0.5, 1, 2, 5, 10 ...),
/// so grid spacing is a round machine number at every zoom.
fn nice_step(v: f64) -> f64 {
    if !v.is_finite() || v <= 0.0 {
        return 1.0;
    }
    let decade = 10f64.powf(v.log10().floor());
    let m = v / decade;
    let m = if m <= 1.0 {
        1.0
    } else if m <= 2.0 {
        2.0
    } else if m <= 5.0 {
        5.0
    } else {
        10.0
    };
    m * decade
}

/// Is this overlay label an axis letter (drawn larger, #195)?
pub(crate) fn is_axis_label(text: &str) -> bool {
    AXIS_NAMES.contains(&text)
}

// ---------------------------------------------------- overlay builders

pub(crate) fn collect_overlay(state: &State) -> Vec<[f32; 6]> {
    let len = gizmo_len(state);
    let mut out: Vec<[f32; 6]> = Vec::new();
    for gizmo in gizmos(state) {
        let (px, py, pz) = (ftoz(gizmo.pos[0]), ftoz(gizmo.pos[1]), ftoz(gizmo.pos[2]));
        for (rgb, dir) in &axes() {
            let p = transform_point(
                Point3::new(dir[0] * len, dir[1] * len, dir[2] * len),
                Point3::new(gizmo.pos[0], gizmo.pos[1], gizmo.pos[2]),
                gizmo.rot,
            );
            let c = if gizmo.world_axes {
                [rgb[0] * WORLD_DIM, rgb[1] * WORLD_DIM, rgb[2] * WORLD_DIM]
            } else {
                *rgb
            };
            out.push([px, py, pz, c[0], c[1], c[2]]);
            out.push([ftoz(p.x), ftoz(p.y), ftoz(p.z), c[0], c[1], c[2]]);
        }
    }
    if state.wcs_ui.pick_anchor {
        // Virtual vertices (feedback): a small 3-axis cross per anchor of
        // the chosen level, so the box points can be picked where they
        // actually are; the pane-highlighted one is drawn brighter.
        let arm = (gizmo_len(state) * 0.09).max(0.5);
        for (k, (pt, _)) in anchor_points(state).iter().enumerate() {
            let c = if k == state.wcs_ui.anchor_xy.min(8) {
                [0.98, 0.98, 0.60]
            } else {
                [0.82, 0.68, 0.22]
            };
            for axis in 0..3 {
                let (mut a, mut b) = (*pt, *pt);
                a[axis] -= arm;
                b[axis] += arm;
                out.push([ftoz(a[0]), ftoz(a[1]), ftoz(a[2]), c[0], c[1], c[2]]);
                out.push([ftoz(b[0]), ftoz(b[1]), ftoz(b[2]), c[0], c[1], c[2]]);
            }
        }
    }
    out.extend(toolpath_lines(state));
    out.extend(region_lines(state));
    out.extend(height_plane_lines(state));
    out.extend(selection_feature_lines(state));
    capped(out)
}

/// Overlay palette for cached paths: feed green, rapid blue, plunge red.
/// Provenance: our own display choice; the three-way move vocabulary
/// follows the RS-274 motion words, not any vendor's screen.
pub(crate) const FEED_RGB: [f32; 3] = [0.20, 0.85, 0.35];
pub(crate) const RAPID_RGB: [f32; 3] = [0.30, 0.50, 0.95];
pub(crate) const PLUNGE_RGB: [f32; 3] = [0.92, 0.30, 0.25];
/// Unselected operations under "Show all toolpaths" draw behind the
/// selected one.
const OTHERS_DIM: f32 = 0.45;
/// One direction chevron per this much feed travel.
const CHEVRON_SPACING: f64 = 10.0;
/// Vertex budget for the overlay buffer (24 bytes each, ~25 MB). Past it
/// the line list is truncated with a logged diagnostic - never silently,
/// and never by refusing to draw at all.
pub(crate) const MAX_OVERLAY_VERTICES: usize = 1 << 20;
/// Sagitta bound for drawing arc moves: arcs flatten through the same
/// law the verifier and the golden dump use, at a display tolerance
/// fine enough that zoomed-in curvature still reads as a curve.
const DISPLAY_CHORD_TOL: f64 = 0.02;

/// One overlay line: two `[x, y, z, r, g, b]` vertices.
pub(crate) fn push_line(out: &mut Vec<[f32; 6]>, a: Point3, b: Point3, c: [f32; 3]) {
    out.push([ftoz(a.x), ftoz(a.y), ftoz(a.z), c[0], c[1], c[2]]);
    out.push([ftoz(b.x), ftoz(b.y), ftoz(b.z), c[0], c[1], c[2]]);
}

pub(crate) fn ftoz(v: f64) -> f32 {
    if v.is_finite() { v as f32 } else { 0.0 }
}

/// The cached CL paths as overlay segments: straight moves as one line in
/// their kind color with direction chevrons along feed travel, arc moves
/// as their chord runs. The selected
/// operation always draws bright; other operations show dimmed only under
/// "Show all toolpaths", and never at all when hidden by its browser eye.
/// Staleness is not a display state here - what is drawn is the cache
/// itself, and the tree's STALE badge is where a stale path is called
/// out.
///
/// Public for the docs-image harness (`crates/ui/tests/docs_images.rs`):
/// tutorial pictures must carry the app's own path display - same colors,
/// same chords, same chevrons - never a test-local reconstitution that
/// can drift from what the viewport actually paints.
#[must_use]
pub fn toolpath_lines(state: &State) -> Vec<[f32; 6]> {
    let mut out = Vec::new();
    let arm = chevron_arm(state);
    // M13.4: the move kinds wear the viewer's colour-vision palette
    // (Off = the three consts above, unchanged).
    let pal = crate::egui_app::colorvision::palette_of(&state.world);
    for (name, entity) in camcraft_app::setup::operation_order(&state.world) {
        // Browser-eye hidden: the cache still exists and still posts, it
        // just does not draw (display state, never a geometry change).
        if state.hidden_paths.contains(&name) {
            continue;
        }
        let Some(cache) = model::toolpath_of(&state.world, &entity) else {
            continue;
        };
        let selected = model::is_selected(&state.world, &entity);
        if !selected && !state.show_all_paths {
            continue;
        }
        let dim = if selected { 1.0 } else { OTHERS_DIM };
        // #261: warned moves wear the gouge colour (crate::path_lines).
        if !crate::path_lines::cache_lines(&mut out, &cache, pal, dim, arm) {
            return out;
        }
    }
    // #261 part 3: and draws the moves it had built as a dashed ghost
    // (crate::refusal_ghost) - off the refusal, never off the operation.
    if let Some(g) = state.refusal_ghost()
        && !crate::refusal_ghost::ghost_lines(&mut out, g, pal, arm)
    {
        return out;
    }
    // #261 part 2: a standing generate refusal marks the spot it names.
    if let Some(at) = state.refusal_focus() {
        crate::path_lines::refusal_mark(&mut out, at, pal, arm);
    }
    out
}

/// An arc move as displayed 3-D chords - `Move::segments3`, the one
/// plane-aware law (M11.2 A). A helical XY arc (the helix entry's half
/// turns) descends linearly in sweep, Z riding the equal-sweep chord
/// index: drawing every chord at `from.z` showed a continuous helix as
/// flat arcs stepping between planes (user hand-test, 2026-11-05). An
/// XZ / YZ arc draws its own vertical chords.
pub(crate) fn arc_segments(m: &camcraft_app::cl::Move) -> Vec<(Point3, Point3)> {
    m.segments3(DISPLAY_CHORD_TOL)
}

/// Chevron size follows the document: 1% of the solid extent, floored so
/// a small part still reads direction.
fn chevron_arm(state: &State) -> f64 {
    let extent = model::solid_world_aabb(&state.world)
        .map(|b| {
            (b.max.x - b.min.x)
                .max(b.max.y - b.min.y)
                .max(b.max.z - b.min.z)
        })
        .unwrap_or(0.0);
    (extent * 0.01).max(0.6)
}

/// Two trailing wings at each mark: the apex sits on the path and the
/// wings sweep backwards, so the pair reads as motion, not a diamond.
/// A feed too short to hold legible wings gets no chevrons at all.
pub(crate) fn chevrons(out: &mut Vec<[f32; 6]>, from: Point3, to: Point3, c: [f32; 3], arm: f64) {
    let d = to - from;
    let len2 = d.dot(d);
    let min2 = (2.0 * arm) * (2.0 * arm);
    if !len2.is_finite() || len2 <= min2 {
        return;
    }
    let len = len2.sqrt();
    let dir = Point3::new(d.x / len, d.y / len, d.z / len);
    // Any perpendicular works for the wing spread; a steep feed can't use
    // Z as the helper, so it takes X.
    let refv = if dir.z.abs() > 0.99 {
        Point3::new(1.0, 0.0, 0.0)
    } else {
        Point3::new(0.0, 0.0, 1.0)
    };
    let n0 = refv.cross(dir);
    let n2 = n0.dot(n0);
    if !n2.is_finite() || n2 <= f64::EPSILON {
        return;
    }
    let nl = n2.sqrt();
    let n = Point3::new(n0.x / nl, n0.y / nl, n0.z / nl);
    let count = ((len / CHEVRON_SPACING).floor().max(1.0) as usize).min(10_000);
    let (back, spread) = (arm * 0.8, arm * 0.45);
    for i in 0..count {
        let t = len * (i as f64 + 0.5) / count as f64;
        let q = Point3::new(
            from.x + dir.x * t,
            from.y + dir.y * t,
            from.z + dir.z * t,
        );
        for s in [-1.0, 1.0] {
            let tip = Point3::new(
                q.x - dir.x * back + n.x * spread * s,
                q.y - dir.y * back + n.y * spread * s,
                q.z - dir.z * back + n.z * spread * s,
            );
            push_line(out, q, tip, c);
        }
    }
}

/// The whole overlay's hard ceiling, enforced at the one place every line
/// producer passes through: truncate on even pairs (never half a line)
/// and say so in the log.
pub(crate) fn capped(mut lines: Vec<[f32; 6]>) -> Vec<[f32; 6]> {
    if lines.len() > MAX_OVERLAY_VERTICES {
        diag_warn!(
            "overlay: {} vertices exceed the display budget; the rest of this frame's \
             lines are dropped",
            lines.len()
        );
        lines.truncate(MAX_OVERLAY_VERTICES & !1);
    }
    lines
}

/// The selected operation's cutting region, in warning orange (our own
/// display choice): "what gets cut", outlined even before the operation
/// has ever been generated. The loops come straight from
/// [`operation::region_of`] and the open runs a Contour pick list adds
/// from [`operation::chains_of`] - both the generators' own construction,
/// one source each - so the outline cannot disagree with the path, and an
/// operation whose source vanished shows nothing rather than a stale
/// outline.
pub(crate) fn region_lines(state: &State) -> Vec<[f32; 6]> {
    let mut out = Vec::new();
    for (name, entity) in model::named_entities(&state.world) {
        if state.hidden_paths.contains(&name)
            || !model::is_operation(&state.world, &entity)
            || !model::is_selected(&state.world, &entity)
        {
            continue;
        }
        let Some(op) = model::operation_of(&state.world, &entity) else {
            continue;
        };
        let Some(z) = operation::region_z(&state.world, op.setup.as_deref(), Some(&op.src), &op.params) else {
            continue; // the region plane's source is gone - draw nothing
                      // rather than an outline at a guessed level
        };
        if let Some(region) = operation::region_of(&state.world, &op) {
            for l in region.loops() {
                let pts = l.points();
                if pts.len() < 3 {
                    continue; // a closed ring needs three corners; never mod 0
                }
                for i in 0..pts.len() {
                    let a = Point3::new(pts[i].x, pts[i].y, z);
                    let q = pts[(i + 1) % pts.len()];
                    push_line(&mut out, a, Point3::new(q.x, q.y, z), overlay(Overlay::Region));
                }
            }
        }
        // Centerline runs: open polylines, no closing chord - the run's
        // ends are where the path stops, and the outline says so.
        for run in operation::chains_of(&state.world, &op) {
            for w in run.windows(2) {
                let a = Point3::new(w[0].x, w[0].y, z);
                let b = Point3::new(w[1].x, w[1].y, z);
                push_line(&mut out, a, b, overlay(Overlay::Region));
            }
        }
    }
    out
}

/// The region outline's orange, shared by `region_lines` and the height
/// planes so the "what gets cut" level reads as one thing.
pub(crate) const REGION_RGB: [f32; 3] = [0.95, 0.58, 0.15];

/// What `region_lines` actually paints, per SELECTED operation whose
/// source resolves geometry: the operation's committed `region_z` and its
/// region - `None` for an open-runs-only pick (a Contour chain has outline
/// but no area to fill). `height_planes` defers a region rectangle to
/// these levels and the scene collectors fill these areas: the same gate
/// the outline runs under, no parallel truth, so a rectangle neither
/// doubles a standing outline nor vanishes because the panel disagrees
/// with the selection about who is shown - and a translucent fill never
/// covers where no outline stands.
pub(crate) fn shown_regions(state: &State) -> Vec<(f64, Option<Region>)> {
    let mut out = Vec::new();
    for (name, entity) in model::named_entities(&state.world) {
        if state.hidden_paths.contains(&name)
            || !model::is_operation(&state.world, &entity)
            || !model::is_selected(&state.world, &entity)
        {
            continue;
        }
        let Some(op) = model::operation_of(&state.world, &entity) else {
            continue;
        };
        let region = operation::region_of(&state.world, &op);
        if let Some(z) = operation::region_z(&state.world, op.setup.as_deref(), Some(&op.src), &op.params)
            && (region.is_some() || !operation::chains_of(&state.world, &op).is_empty())
        {
            out.push((z, region));
        }
    }
    out
}

/// The levels of [`shown_regions`] - one truth, read at the height-plane
/// gate.
pub(crate) fn shown_region_levels(state: &State) -> Vec<f64> {
    shown_regions(state).into_iter().map(|(z, _)| z).collect()
}
/// Safety level (our own palette): the clearance plane everything starts
/// above, and - Contour only - the shorter retract between passes.
pub(crate) const CLEARANCE_RGB: [f32; 3] = [0.55, 0.60, 0.68];
pub(crate) const RETRACT_RGB: [f32; 3] = [0.40, 0.70, 0.95];
/// The depth the stock is cut to: red, unmistakably the low water mark.
pub(crate) const CUT_RGB: [f32; 3] = [0.90, 0.35, 0.30];

/// One horizontal level of the open operation's DRAFT parameters: a plane
/// to mark in the viewport, with the corner label it carries.
pub(crate) struct HeightPlane {
    pub z: f64,
    /// The draft height this plane resolved from - the chip suffix names
    /// its source, but only when it is one (`Fixed` planes are their own
    /// number and get no parenthetical).
    pub src: Height,
    pub rgb: [f32; 3],
    pub name: &'static str,
    /// Which rectangle corner carries the label - 0 NE, 1 NW, 2 SE, 3 SW,
    /// distinct per kind so simultaneous labels never stack.
    pub corner: usize,
    /// The region plane's rectangle is dropped where `region_lines`
    /// already outlines the region at the same level: the outline IS the
    /// plane, and a coincident double would only thicken one line.
    pub skip_rect: bool,
}

/// The setup context the viewport's height planes and their work-offset
/// chip resolve in (#329): the shown operation's own declared setup; with
/// no such operation (a draft name), the ACTIVE setup - what
/// CreateOperation would declare - never a locally chosen default.
pub(crate) fn shown_setup(state: &State) -> Option<String> {
    match model::find_by_name(&state.world, &state.op_ui.name)
        .and_then(|e| model::operation_of(&state.world, &e))
    {
        Some(op) => op.setup,
        None => camcraft_app::setup::active(&state.world),
    }
}

/// The open operation's height planes, read from the draft parameters -
/// what the spinboxes point at right now, not what was last applied, so
/// spinning Clearance visibly walks the plane before Apply commits it.
/// No operation open (empty or unresolved name): no planes, ever.
pub(crate) fn height_planes(state: &State) -> Vec<HeightPlane> {
    if state.op_ui.name.is_empty() || state.hidden_paths.contains(&state.op_ui.name) {
        return Vec::new();
    }
    let Some(entity) = model::find_by_name(&state.world, &state.op_ui.name) else {
        return Vec::new();
    };
    if model::operation_of(&state.world, &entity).is_none() {
        return Vec::new(); // a typed name is not an operation
    }
    let draft = &state.op_ui.params;
    // Where the orange outline actually stands, level by level: the
    // shared `shown_region_levels` gate, so `region_lines` and the
    // plane rectangles answer to one predicate, never two.
    let levels = shown_region_levels(state);
    let stands = |z: f64| levels.contains(&z);
    // Heights resolve against the world before they reach the viewport:
    // an unavailable source drops its plane (never a rectangle at a
    // guessed z) - the panel says why, and generation refuses by name.
    let world = &state.world;
    // A work-offset height measures from the shown op's OWN setup (#329).
    let ctx = shown_setup(state);
    let setup = ctx.as_deref();
    // A From: Selection height reads the open op's own source (M11.3 A).
    let src_owned = state.op_ui.src();
    let src = Some(&src_owned);
    let mut out = Vec::new();
    let clr = draft.clearance();
    if let Some(z) = clr.z_in(world, setup, src) {
        out.push(HeightPlane {
            z,
            src: clr,
            rgb: CLEARANCE_RGB,
            name: "Clearance",
            corner: 0,
            skip_rect: false,
        });
    }
    // Retract is the between-pass hop plane, every kind that stores one.
    // It draws at the effective hop - `min(retract, clearance)` of the
    // RESOLVED planes - because that is the Z the rapids actually travel
    // at, even when a legacy file parked retract above clearance. Kinds
    // whose retract is unset hop at clearance and get no extra plane: the
    // Clearance plane above already says where those rapids run.
    let hop = match draft {
        operation::OpParams::Contour { retract_z, .. } => Some(*retract_z),
        operation::OpParams::Pocket { retract_z, .. }
        // The roughing twins hop like their parent ladder (M2.4 A).
        | operation::OpParams::Adaptive2D { retract_z, .. }
        | operation::OpParams::Offset2D { retract_z, .. }
        | operation::OpParams::Pocket3D { retract_z, .. }
        | operation::OpParams::Finish3D { retract_z, .. }
        | operation::OpParams::Contour5D { retract_z, .. }
        | operation::OpParams::Parallel5D { retract_z, .. }
        | operation::OpParams::Swarf { retract_z, .. }
        | operation::OpParams::AdvancedSwarf { retract_z, .. }
        // The rotary twins store the hop the same way (M5.1 stage E).
        | operation::OpParams::RotaryContour { retract_z, .. }
        | operation::OpParams::RotaryParallel { retract_z, .. }
        // The stamped ladder keeps the pocket-family hop (M5.1 E3),
        // flow stores the finish's (M5.1 I), and the drive-curve
        // contour hops between its chain runs the same way (M5.1 J).
        | operation::OpParams::MultiAxisRough { retract_z, .. }
        | operation::OpParams::Flow5D { retract_z, .. }
        | operation::OpParams::MultiAxisContour { retract_z, .. }
        | operation::OpParams::Slot { retract_z, .. }
        | operation::OpParams::TrochoidalSlot { retract_z, .. }
        // The engrave twin hops where trace hops, the circular ladder
        // where the pocket hops, and the chamfer ride between its
        // chains the trace's way (M2.4 B) - all the store-either shape.
        | operation::OpParams::Trace { retract_z, .. }
        | operation::OpParams::Engrave { retract_z, .. }
        | operation::OpParams::Chamfer2D { retract_z, .. }
        | operation::OpParams::Circular { retract_z, .. }
        // The boring ride and the thread mill hop between holes the
        // drill's way - the same store-either shape (M2.4 C).
        | operation::OpParams::Drill { retract_z, .. }
        | operation::OpParams::Bore { retract_z, .. }
        | operation::OpParams::Thread { retract_z, .. } => *retract_z,
        // The wrap ladder stores no hop at all - its rapids ride
        // the kernel's law off the skin (M5.1 stage E2) - exactly
        // Face's answer of "hop at clearance, say nothing extra".
        // Facing is the same answer for the same reason (M5.2 A):
        // its rapids slide tangentially off the fresh face at the
        // clear radius, and the descent between rungs is a feed
        // move in air - no plane to name. The profile kinds say it
        // again (M5.2 B): their rungs slide radially in and out at
        // the window's ends, and the air between rungs is a rapid
        // at the clear radius, not a plane.
        // The blades say it a third time (M5.2 C): their rapids ride
        // the clear radius over the bar too, and Part's kerf ladder
        // lifts out and back at the same height - no hop plane. The
        // wave says it a fourth (M5.2 D): a thread's rungs pass over
        // the crest radius in the clear, never lifting to a plane.
        operation::OpParams::Face { .. }
        | operation::OpParams::RotaryOffset { .. }
        | operation::OpParams::Facing { .. }
        | operation::OpParams::ProfileRough { .. }
        | operation::OpParams::ProfileFinish { .. }
        | operation::OpParams::GrooveRough { .. }
        | operation::OpParams::GrooveFinish { .. }
        | operation::OpParams::Part { .. }
        // The contour rides name their rapids plane off the ride's
        // own reach too - no authored hop to draw (M5.2 E).
        | operation::OpParams::Threading { .. }
        // The gauge rapids plane names itself off the bar's radius
        // the same way - no authored hop to draw (M5.2 E2).
        | operation::OpParams::Chamfering { .. }
        | operation::OpParams::TaperTurning { .. }
        | operation::OpParams::MultiPassTurning { .. }
        // The boring bar hops to the axis, not to a plane - the
        // rapids in and out are the lead-off and lead-back, and the
        // travel home rides the clearance height (M5.2 F).
        | operation::OpParams::FormTurning { .. }
        | operation::OpParams::BoringTurning { .. }
        // The deburr ride says it too: every run starts and ends at
        // the clearance plane by the kernel's own word - no hop
        // height exists to author or draw (M2.5 E B).
        | operation::OpParams::Deburr { .. }
        // The probe travels at its clearance plane only (#108), and
        // so does the torch between contours (#670).
        | operation::OpParams::Probe { .. }
        | operation::OpParams::PlasmaProfile(_) => None,
    };
    if let Some(hop) = hop
        && let (Some(r), Some(c)) = (hop.z_in(world, setup, src), draft.clearance().z_in(world, setup, src))
    {
        out.push(HeightPlane {
            z: r.min(c),
            src: hop,
            rgb: overlay(Overlay::Retract),
            name: "Retract",
            corner: 1,
            skip_rect: false,
        });
    }
    match draft {
        // For the depth kinds, Top is the section the paths run in and
        // Bottom the last step down. Face skims one plane unless it
        // stages: then its Top is the first level, drawn like the depth
        // kinds' Top, with the finished Cut below it.
        operation::OpParams::Face { top_z, cut_z, .. } => {
            let cut = cut_z.z_in(world, setup, src);
            if let (Some(t), Some(c)) = ((*top_z).to_owned(), cut)
                && let Some(tz) = t.z_in(world, setup, src)
                && tz != c
            {
                out.push(HeightPlane {
                    z: tz,
                    src: t,
                    rgb: overlay(Overlay::Region),
                    name: "Top",
                    corner: 1,
                    skip_rect: stands(tz),
                });
            }
            if let Some(z) = cut {
                out.push(HeightPlane {
                    z,
                    src: *cut_z,
                    rgb: overlay(Overlay::Cut),
                    name: "Cut",
                    corner: 2,
                    // Stands only where no outline is painted AT the cut
                    // level: a staged Face's outline sits up at its Top,
                    // so the finished Cut keeps its own rectangle - the
                    // old blanket flag used to erase it.
                    skip_rect: stands(z),
                });
            }
        }
        // The 3-D pocket's Top plane is its first level - the one plane
        // of a ladder the viewport cannot draw here (the model's own
        // sections are the rest); Bottom is the snapped last level, cut
        // exact, so the shared depth-kind drawing is right for both.
        // The finish's Top is the first ring's tip plane and Bottom the
        // snapped last one - the same ladder drawing (the rings between
        // them are the model's own, undrawable here, like the pocket's).
        // The 5-axis twins draw the finish's ladder: first ring plane
        // up top, snapped last one below (M5.1 stage C).
        operation::OpParams::Contour { top_z, bottom_z, .. }
        | operation::OpParams::Pocket { top_z, bottom_z, .. }
        // The twins run the pocket's ladder: first level up top, the
        // floor below, the levels between the section's own (M2.4 A).
        | operation::OpParams::Adaptive2D { top_z, bottom_z, .. }
        | operation::OpParams::Offset2D { top_z, bottom_z, .. }
        | operation::OpParams::Pocket3D { top_z, bottom_z, .. }
        | operation::OpParams::Finish3D { top_z, bottom_z, .. }
        | operation::OpParams::Contour5D { top_z, bottom_z, .. }
        | operation::OpParams::Parallel5D { top_z, bottom_z, .. }
        // The swarf family draws the same ladder planes (M5.1 stage D),
        // and the rotary twins' are the stations' own top and bottom
        // (stage E): first and last axial rung, the rungs between are
        // the model's, like every ladder here.
        | operation::OpParams::Swarf { top_z, bottom_z, .. }
        | operation::OpParams::AdvancedSwarf { top_z, bottom_z, .. }
        | operation::OpParams::RotaryContour { top_z, bottom_z, .. }
        | operation::OpParams::RotaryParallel { top_z, bottom_z, .. }
        // The rough ladder's stations draw the same two rungs (E2),
        // and so does the stamped one - it IS the pocket ladder (E3).
        | operation::OpParams::RotaryOffset { top_z, bottom_z, .. }
        | operation::OpParams::MultiAxisRough { top_z, bottom_z, .. }
        // Flow draws the raster's ladder: first and last line planes,
        // the turning of the lines in between needs no plane (M5.1 I).
        // The drive-curve contour draws the same two planes, but its
        // pair is no ladder - it is the machining window: a picked
        // chain standing outside it is refused, so the planes say
        // exactly what may be cut (M5.1 J).
        | operation::OpParams::Flow5D { top_z, bottom_z, .. }
        | operation::OpParams::MultiAxisContour { top_z, bottom_z, .. }
        | operation::OpParams::Slot { top_z, bottom_z, .. }
        | operation::OpParams::TrochoidalSlot { top_z, bottom_z, .. }
        // The engrave twin draws the trace's ladder - first level up
        // top, floor below - and the circular ladder its rings' two
        // rungs: vault or flat, the dome law's stations live between
        // the same pair (M2.4 B).
        | operation::OpParams::Engrave { top_z, bottom_z, .. }
        | operation::OpParams::Circular { top_z, bottom_z, .. }
        // The facing ladder reads the same two rungs (M5.2 A): stock
        // top above, the facing plane below - world Z planes standing
        // as stations on the fitted axis, the rungs between them the
        // stepdown's own, like every ladder here. The profile kinds
        // bound their window the same two planes (M5.2 B).
        | operation::OpParams::Trace { top_z, bottom_z, .. }
        | operation::OpParams::Facing { top_z, bottom_z, .. }
        | operation::OpParams::ProfileRough { top_z, bottom_z, .. }
        | operation::OpParams::ProfileFinish { top_z, bottom_z, .. }
        // The groove kinds bound their window the same two planes
        // (M5.2 C): Top is where the blade may enter, Bottom the
        // floor or finished groove it walks to. The threading window
        // is the same promise (M5.2 D): Top is the first turn's
        // plane, Bottom the last rungs' - the helix lives between.
        | operation::OpParams::GrooveRough { top_z, bottom_z, .. }
        | operation::OpParams::GrooveFinish { top_z, bottom_z, .. }
        // The contour window is the same promise (M5.2 E): the two
        // planes the promised face stands between.
        | operation::OpParams::Threading { top_z, bottom_z, .. }
        // The form window is the same pair of planes: the stations
        // the formed surfaces stand between (M5.2 E2).
        | operation::OpParams::Chamfering { top_z, bottom_z, .. }
        | operation::OpParams::TaperTurning { top_z, bottom_z, .. }
        | operation::OpParams::MultiPassTurning { top_z, bottom_z, .. }
        | operation::OpParams::FormTurning { top_z, bottom_z, .. }
        // The bore window is the same pair of planes: the length of
        // wall the ride walks (M5.2 F). The deburr window is the two
        // planes the tip rides between - the ceiling and floor its
        // loci are clipped by (M2.5 E B).
        | operation::OpParams::BoringTurning { top_z, bottom_z, .. }
        // The plate the torch cuts through: its top (the cut plane) and
        // bottom (#670).
        | operation::OpParams::PlasmaProfile(operation::PlasmaOp { top_z, bottom_z, .. })
        | operation::OpParams::Deburr { top_z, bottom_z, .. } => {
            if let Some(z) = top_z.z_in(world, setup, src) {
                out.push(HeightPlane {
                    z,
                    src: *top_z,
                    rgb: overlay(Overlay::Region),
                    name: "Top",
                    corner: 2,
                    skip_rect: stands(z),
                });
            }
            if let Some(z) = bottom_z.z_in(world, setup, src) {
                out.push(HeightPlane {
                    z,
                    src: *bottom_z,
                    rgb: overlay(Overlay::Cut),
                    name: "Bottom",
                    corner: 3,
                    skip_rect: false,
                });
            }
        }
        // The probe draws only its clearance plane (#108): its touches
        // are points, not a window.
        operation::OpParams::Probe { .. } => {}
        // Parting draws one plane: the parting station itself, the
        // kerf's only promise (M5.2 C). Its Top of Part bound is a
        // sanity gate on the parameter, not geometry - the column the
        // blade severs comes from the model, so painting a Top the
        // path never walks to would advertise a window it cuts past.
        operation::OpParams::Part { bottom_z, .. } => {
            if let Some(z) = bottom_z.z_in(world, setup, src) {
                out.push(HeightPlane {
                    z,
                    src: *bottom_z,
                    rgb: overlay(Overlay::Cut),
                    name: "Parting",
                    corner: 3,
                    skip_rect: false,
                });
            }
        }
        // The chamfer ride draws its one authored plane: the top edge's
        // own Z, where the cone's apex stands. The face's floor is
        // DERIVED (width·tan(angle), variant docs) - a rectangle at a
        // z no height author could disagree with is honest, and Part's
        // law above says an advertised window the path cannot be
        // measured against is not. The corner Z rides the pane's read
        // of the same two numbers.
        operation::OpParams::Chamfer2D { top_z, .. } => {
            if let Some(z) = top_z.z_in(world, setup, src) {
                out.push(HeightPlane {
                    z,
                    src: *top_z,
                    rgb: overlay(Overlay::Region),
                    name: "Top",
                    corner: 2,
                    skip_rect: stands(z),
                });
            }
        }
        // A drill's Top is its R plane - where the feed move starts in
        // every hole - and its Bottom exists only as an override: the
        // hole floors ride the holes themselves, and a floor per hole
        // is not a plane. The boring ride and the thread mill keep
        // the same promise: they are hole-list kinds, one ring or one
        // helix per hole (M2.4 C).
        operation::OpParams::Drill { top_z, bottom_z, .. }
        | operation::OpParams::Bore { top_z, bottom_z, .. }
        | operation::OpParams::Thread { top_z, bottom_z, .. } => {
            if let Some(z) = top_z.z_in(world, setup, src) {
                out.push(HeightPlane {
                    z,
                    src: *top_z,
                    rgb: overlay(Overlay::Region),
                    name: "R plane",
                    corner: 2,
                    skip_rect: stands(z),
                });
            }
            if let Some(b) = bottom_z
                && let Some(z) = b.z_in(world, setup, src)
            {
                out.push(HeightPlane {
                    z,
                    src: *b,
                    rgb: overlay(Overlay::Cut),
                    name: "Depth",
                    corner: 3,
                    skip_rect: false,
                });
            }
        }
    }
    out.retain(|p| p.z.is_finite());
    out
}

/// The planes' XY reach: the stock box (the model bounds when there is no
/// stock), padded outward past the tool so a rectangle always sits clear
/// of the material. Nothing resolvable, or a non-finite pad, leaves the
/// planes unplaced - never a rectangle of nonsense extent.
pub(crate) fn plane_rect(state: &State) -> Option<[f64; 4]> {
    let b = model::stock_world_aabb(&state.world).or_else(|| model::solid_world_aabb(&state.world))?;
    let pad = model::find_by_name(&state.world, &state.op_ui.tool)
        .and_then(|e| model::tool_of(&state.world, &e))
        .map(|t| t.diameter)
        .filter(|d| d.is_finite() && *d > 0.0)
        .map_or(0.0, |d| d * 0.5 + 2.0);
    let rect = [b.min.x - pad, b.min.y - pad, b.max.x + pad, b.max.y + pad];
    rect.iter()
        .all(|v| v.is_finite())
        .then_some(rect)
}

pub(crate) fn plane_corner(rect: [f64; 4], corner: usize) -> (f64, f64) {
    let [x0, y0, x1, y1] = rect;
    match corner {
        0 => (x1, y1),
        1 => (x0, y1),
        2 => (x1, y0),
        _ => (x0, y0),
    }
}

/// The height planes as overlay geometry: one rectangle each, at the
/// plane's level. Under the same document/draft gates as the rest of
/// `collect_overlay` - `draft_key` folds the draft heights, so a spinbox
/// edit walks its plane without touching the document.
pub(crate) fn height_plane_lines(state: &State) -> Vec<[f32; 6]> {
    let planes = height_planes(state);
    if planes.iter().all(|p| p.skip_rect) {
        return Vec::new();
    }
    let Some(rect) = plane_rect(state) else {
        return Vec::new();
    };
    let [x0, y0, x1, y1] = rect;
    let ring = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    let mut out = Vec::new();
    for p in planes.iter().filter(|p| !p.skip_rect) {
        for i in 0..4 {
            let a = ring[i];
            let b = ring[(i + 1) % 4];
            push_line(
                &mut out,
                Point3::new(a[0], a[1], p.z),
                Point3::new(b[0], b[1], p.z),
                p.rgb,
            );
        }
    }
    out
}

/// The open operation's resolved pick list in 3D: each picked face's
/// boundary loops and each picked chain's polyline, drawn where the
/// feature actually is - violet, deliberately not region orange. Orange
/// answers "what the path cuts, at the cut plane" (see `region_lines`);
/// violet answers "what you picked, where it is" - the two disagree
/// honestly (a face picked low on a wall cuts at its own height, a chain
/// runs in the solid's skin, not at some level's offset). The hover
/// highlight is screen-space and evaporates under camera moves; this is
/// overlay-buffer geometry, rebuilt under the same document and
/// selection gates as the rest of `collect_overlay`, so picks hold their
/// place while orbiting. A vanished pick - hidden model, deleted entity,
/// stale id - draws nothing rather than a ghost, the hover rule again.
pub(crate) fn selection_feature_lines(state: &State) -> Vec<[f32; 6]> {
    feature_lines(state, &shown_picks(state))
}

/// The picks shown: the collecting Setups page draft, else the open operation's.
pub(crate) fn shown_picks(state: &State) -> Vec<GeoSel> {
    // #330: while the Setups page collects a lay-on-a-plane pick, its
    // draft picks are the ones drawn.
    // #561: the locate draft's mates and its waiting half.
    if let Some(d) = &state.locate_pick {
        return d.mates.iter().flat_map(|m| [m.part.clone(), m.fixture.clone()]).chain(d.half.clone()).collect();
    }
    if let Some(draft) = state.plane_pick.as_ref().filter(|d| d.picking) {
        return draft.picks.clone();
    }
    // #555: the three-points draft's picks.
    if let Some(d) = &state.points_pick {
        return d.picks.clone();
    }
    // #557: the axes-in-the-plane draft's picks, while it collects.
    if let Some(d) = state.axis_pick.as_ref().filter(|d| d.picking.is_some()) {
        return [&d.axes.y, &d.axes.x].into_iter().flatten().flat_map(|p| p.picks.clone()).collect();
    }
    if state.op_ui.name.is_empty() || state.hidden_paths.contains(&state.op_ui.name) {
        return Vec::new(); // nothing open: nothing picked to pin down
    }
    let op = model::find_by_name(&state.world, &state.op_ui.name).and_then(|e| model::operation_of(&state.world, &e));
    match op.map(|o| o.src) {
        Some(operation::SrcRef::Selection(items)) => items,
        _ => Vec::new(),
    }
}

/// The pick violet: the 3D outlines and the picked points' screen marks.
pub(crate) const PICK_RGB: [f32; 3] = [0.72, 0.44, 0.86];

/// The picks' outlines in 3D, violet (see [`selection_feature_lines`]).
fn feature_lines(state: &State, items: &[GeoSel]) -> Vec<[f32; 6]> {
    let mut out = Vec::new();
    for item in items {
        if state.hidden_in_view(&item.entity) {
            continue; // display-off models are transparent to picking and drawing
        }
        let Some(entity) = model::find_by_name(&state.world, &item.entity) else {
            continue;
        };
        let Some(features) = model::features_of(&state.world, &entity) else {
            continue;
        };
        let loc = camcraft_app::setup_align::view_pose(&state.world, &entity).unwrap_or_default(); // #326
        let draw = |out: &mut Vec<[f32; 6]>, pts: &[[f64; 3]], closed: bool| {
            let n = pts.len();
            if n < 2 || (closed && n < 3) {
                return; // never mod 0; a ring needs three corners
            }
            let last = if closed { n } else { n - 1 };
            for i in 0..last {
                let a = transform_point(Point3::new(pts[i][0], pts[i][1], pts[i][2]), loc.pos, loc.rot);
                let q = &pts[(i + 1) % n];
                let b = transform_point(Point3::new(q[0], q[1], q[2]), loc.pos, loc.rot);
                push_line(out, a, b, overlay(Overlay::Pick));
            }
        };
        match item.feature {
            FeatureSel::Face(fid) => {
                let Some(face) = features.faces.iter().find(|f| f.id == fid) else {
                    continue; // stale id: the model re-meshed since the pick
                };
                for l in &face.loops {
                    let pts: Vec<[f64; 3]> = l
                        .verts
                        .iter()
                        .filter_map(|&vi| features.verts.get(vi as usize).copied())
                        .collect();
                    draw(&mut out, &pts, !matches!(l.kind, feature::LoopKind::Open));
                }
            }
            FeatureSel::Chain(cid) => {
                let Some(chain) = feature::resolve_chain(&features, cid) else {
                    continue;
                };
                let pts = feature::chain_points(&features, &chain);
                draw(&mut out, &pts, chain.closed);
            }
            FeatureSel::Patch(pid) => {
                // The weld's outline, not the seams inside it: the seam
                // edges are exactly what the patch dissolved, and
                // drawing them would advertise strips the pick is not.
                let Some(rings) = feature::patch_rings(&features, pid) else {
                    continue; // stale id: the model re-meshed since the pick
                };
                for r in &rings {
                    let pts: Vec<[f64; 3]> = r
                        .verts
                        .iter()
                        .filter_map(|&vi| features.verts.get(vi as usize).copied())
                        .collect();
                    draw(&mut out, &pts, r.closed);
                }
            }
            FeatureSel::Vertex(vid) => {
                // A point has no outline: a small 3-axis cross pins it.
                let Some(v) = feature::resolve_vertex(&features, vid)
                    .and_then(|vi| features.verts.get(vi as usize))
                else {
                    continue; // stale id: the model re-meshed since the pick
                };
                let c = transform_point(Point3::new(v[0], v[1], v[2]), loc.pos, loc.rot);
                let arm = (gizmo_len(state) * 0.05).max(0.5);
                for d in [[arm, 0.0, 0.0], [0.0, arm, 0.0], [0.0, 0.0, arm]] {
                    let a = Point3::new(c.x - d[0], c.y - d[1], c.z - d[2]);
                    let b = Point3::new(c.x + d[0], c.y + d[1], c.z + d[2]);
                    push_line(&mut out, a, b, overlay(Overlay::Pick));
                }
            }
        }
    }
    out
}
