//! The viewport's feature click and hover (#692 step 2): every Face, Edge
//! and Vertex pick - operation geometry, a boundary, the Setups page's
//! plane and axis rows, the picks the WCS origin chips read - resolves
//! through the app's one pick rule (`camcraft_app::pick::pick_selection`):
//! the front-most visible surface, then a feature point, a feature edge or
//! the welded face under the cursor, only among the kinds the asking tool
//! accepts. Nothing here measures geometry; it builds the camera's ray,
//! says which kinds the armed tool takes, and routes the answer to the
//! draft or command that already owned that click.
//!
//! Provenance: our own routing over our own pick API; the pixel-to-mm
//! scale is the pinhole camera's (2 tan(fov/2) depth per image height) -
//! general computer-graphics practice. No vendor material consulted.

use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::{self, EdgeUnit, FeaturePick, PickKinds, PickQuery, PickRay, PickTarget, Projection};

use crate::axis_pick::AxisBy;
use crate::interact::{Mods, queue_boundary_pick, queue_feature_pick, ray_at};
use crate::state::{GeoPick, HoverHit, State};
use crate::viewport::ProjectionMode;

/// How close a feature edge must draw to the cursor to be pickable, in
/// logical px - the same tolerance the anchor markers use.
pub(crate) const PICK_EDGE_PX: f32 = 12.0;

/// How close a feature point must draw to the cursor to be pickable, px.
pub(crate) const PICK_POINT_PX: f32 = 8.0;

/// What the asking tool takes right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Ask {
    pub kinds: PickKinds,
    /// What an edge pick selects.
    pub unit: EdgeUnit,
}

impl Ask {
    /// The snap radius, px: a point's when only points are taken.
    fn px(self) -> f32 {
        if self.kinds.points && !self.kinds.edges { PICK_POINT_PX } else { PICK_EDGE_PX }
    }

    /// What a miss says is missing.
    fn noun(self) -> &'static str {
        match (self.kinds.faces, self.kinds.edges, self.kinds.points) {
            (true, _, _) => "face",
            (false, true, _) => "edge",
            _ => "vertex",
        }
    }
}

/// The kinds the armed tool accepts: a fixture locate draft first (faces
/// and points, #561), then the three-points plane (by pick mode, #555),
/// then the Setups page rows (the plane
/// takes faces only, #554; an axis one straight edge or two points, #557),
/// else the pick mode (Edge falls back to the face between the edges). A
/// boundary and an axis take one run per edge click; Alt one segment.
pub(crate) fn ask(state: &State, mods: Mods) -> Ask {
    let chain = if mods.alt {
        EdgeUnit::Segment
    } else if state.boundary_pick.is_some()
        || crate::axis_pick::collecting(state)
        || crate::wcs_pick::collecting(state)
    {
        // #701: a role pick takes one run per edge click too, like a boundary.
        EdgeUnit::Run
    } else {
        EdgeUnit::Chain
    };
    let mode = match state.geo_pick {
        GeoPick::Edge => PickKinds { faces: true, ..PickKinds::EDGES },
        GeoPick::Vertex => PickKinds::POINTS,
        GeoPick::Face | GeoPick::Model => PickKinds::FACES,
    };
    let kinds = if state.locate_pick.is_some() {
        // #561: a mate is a face or a point, whichever is under the
        // cursor. Edge mode keeps its edges so `locate_pick::take` can
        // refuse one by name; Vertex mode stays points only.
        match state.geo_pick {
            GeoPick::Vertex => PickKinds::POINTS,
            _ => PickKinds { points: true, ..mode },
        }
    } else if state.points_pick.is_some() {
        // #555: three points (it arms Vertex mode). A face or an edge
        // picked in another mode still reaches `points_pick::take`,
        // which refuses it by name.
        mode
    } else if crate::plane_pick::collecting(state) {
        PickKinds::FACES
    } else if let Some(by) = state.axis_pick.as_ref().and_then(|d| d.picking.map(|a| d.by(a))) {
        match by {
            AxisBy::Edge => PickKinds::EDGES,
            AxisBy::Points => PickKinds::POINTS,
        }
    } else if crate::wcs_pick::collecting(state) {
        // #701: the role draft takes faces, edges, corners and walls alike -
        // the toolbar's pick mode stays the user's own.
        PickKinds { faces: true, edges: true, points: true, centres: false }
    } else {
        mode
    };
    Ask { kinds, unit: chain }
}

/// The pick query through a logical-px viewport point: the camera's ray
/// and the pixel radius as world mm (ortho: one scale everywhere;
/// perspective: per unit of depth, so it holds at the silhouette too).
/// `through` follows the viewport's pick-through toggle (#692), off by
/// default: only then can a hidden edge or point be picked.
pub(crate) fn query_at(state: &State, p: [f32; 2], ask: Ask) -> Option<PickQuery> {
    let ([_, y0], [_, y1]) = state.viewport_rect;
    let h = f64::from(y1 - y0);
    let (origin, dir) = ray_at(state, p)?;
    let px = f64::from(ask.px());
    let (projection, radius, radius_per_depth) = match state.cam.projection {
        ProjectionMode::Orthographic => (Projection::Ortho, px * 2.0 * state.cam.ortho_half_height() / h, 0.0),
        // `Camera::ray_through`'s 60-degree vertical field.
        ProjectionMode::Perspective => {
            (Projection::Perspective, 0.0, px * 2.0 * std::f64::consts::FRAC_PI_6.tan() / h)
        }
    };
    let ray = PickRay { origin, dir, projection };
    Some(PickQuery { ray, radius, radius_per_depth, kinds: ask.kinds, through: state.pick_through })
}

/// The one pick under a viewport point, and the selection it stands for.
pub(crate) fn feature_at(state: &State, p: [f32; 2], ask: Ask) -> Option<(FeaturePick, Option<FeatureSel>)> {
    let q = query_at(state, p, ask)?;
    pick::pick_selection(&state.world, &q, ask.unit, |name| state.hidden_in_view(name))
}

fn hover_of(pick: &FeaturePick, sel: Option<FeatureSel>) -> HoverHit {
    let facet = match pick.target {
        PickTarget::Face { facet, .. } => facet,
        _ => usize::MAX, // no facet drives an edge or point highlight
    };
    HoverHit { entity: pick.entity.clone(), facet, feature: sel }
}

/// Hover in a feature mode: what a click here would take, or nothing.
pub(crate) fn hover(state: &mut State, p: [f32; 2]) {
    let ask = ask(state, Mods::default());
    state.hover = feature_at(state, p, ask).map(|(pick, sel)| hover_of(&pick, sel));
}

/// A Face/Edge/Vertex click: resolve it by the one rule, then hand it to
/// whoever collects - in main's order: a fixture locate draft, the
/// three-points plane, a #701 WCS role draft, an axis or plane row, the
/// stray-click hint, a boundary, or the open operation's geometry (edges
/// through the edge pick's Ctrl/Shift law, faces and points straight into
/// the list).
pub(crate) fn click(state: &mut State, p: [f32; 2], mods: Mods) {
    let ask = ask(state, mods);
    let Some((found, sel)) = feature_at(state, p, ask) else {
        state.hover = None;
        state.status = format!("no {} under the cursor", ask.noun());
        return;
    };
    // Mirror the pick in the hover so highlight and click agree.
    state.hover = Some(hover_of(&found, sel));
    let Some(feature) = sel else {
        state.status = format!("the pick on '{}' names no feature - try beside it", found.entity);
        return;
    };
    let pick = GeoSel { entity: found.entity, feature };
    if state.locate_pick.is_some() {
        crate::locate_pick::take(state, pick); // #561: the locate mates; refuses an edge by name
        return;
    }
    if state.points_pick.is_some() {
        crate::points_pick::take(state, pick); // #555: refuses a face or an edge by name
        return;
    }
    if crate::wcs_pick::collecting(state) {
        crate::wcs_pick::take(state, pick); // #701: the Setups page's role draft
        return;
    }
    if crate::axis_pick::collecting(state) {
        crate::axis_pick::take(state, pick); // #557
        return;
    }
    if crate::plane_pick::collecting(state) {
        crate::plane_pick::take(state, pick); // #330 / #554
        return;
    }
    if crate::axis_pick::hint_stray(state) {
        return; // #557: the axes rows are open, nothing armed
    }
    let append = mods.ctrl || mods.shift;
    match (found.target, state.boundary_pick.is_some()) {
        (PickTarget::Point(_), true) => {
            // A point bounds nothing: refused by name, never drafted.
            state.status =
                format!("{} of '{}' is a point - a boundary takes faces or edges", pick.feature, pick.entity);
        }
        (PickTarget::Edge { .. }, false) => crate::edge_pick::queue_edge_pick(state, pick, mods),
        (_, true) => queue_boundary_pick(state, pick, mods),
        _ => queue_feature_pick(state, pick, append),
    }
}
