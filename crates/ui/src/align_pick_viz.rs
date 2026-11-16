//! The Setups page's alignment picks, drawn so they can be seen (#738,
//! the maintainer's hand test: "only the outline is drawn in purple",
//! "not a speck", "so thin it barely shows"). While a face, three-point
//! or axis draft is collecting, on top of the 1 px GPU outline
//! (`interact::selection_feature_lines`):
//!
//! - a picked **face** is filled with the pick violet, semi-transparent,
//!   so the whole face shows which one it is;
//! - a picked **point** is a round, filled disc with a light rim;
//! - a picked **edge** is a bold stroke ([`EDGE_PX`]).
//!
//! Screen-space egui shapes (a wgpu line has no width), projected through
//! the same camera math as the hover outline, in the active setup's frame
//! (`setup_align::view_pose`). A vanished pick - hidden model, deleted
//! entity, stale id - draws nothing, the hover rule.
//!
//! Provenance: our own display design; no vendor material consulted.

use camcraft_app::geometry::{Point3, feature, transform_point};
use camcraft_app::model;
use camcraft_app::operation::{FeatureSel, GeoSel};

use crate::interact::{feature_segments, project_uv};
use crate::state::State;

/// The pick violet (`interact::feature_lines`' own colour), 0..255.
pub const PICK_RGB8: [u8; 3] = [184, 112, 219];
/// A picked face's fill: the violet, this opaque (of 255).
pub const FACE_ALPHA: u8 = 96;
/// A picked edge's stroke width (px) - bold; the hover outline is 1 px.
pub const EDGE_PX: f32 = 4.0;
/// A picked point's disc radius (px).
pub const POINT_RADIUS_PX: f32 = 6.0;
/// A picked point's disc fill: the violet, this opaque (of 255).
pub const POINT_ALPHA: u8 = 210;

/// The draft's picks as viewport-uv (0..1) geometry.
#[derive(Debug, Default, PartialEq)]
pub struct AlignMarks {
    /// One triangle per facet of each picked face.
    pub faces: Vec<[[f32; 2]; 3]>,
    pub edges: Vec<([f32; 2], [f32; 2])>,
    pub points: Vec<[f32; 2]>,
}

/// The picks of the alignment draft that is collecting - the same gates
/// as `selection_feature_lines` draws them under.
fn draft_picks(state: &State) -> Vec<GeoSel> {
    if state.locate_pick.is_some() {
        return Vec::new(); // the locate draft draws its own mates
    }
    if let Some(d) = state.plane_pick.as_ref().filter(|d| d.picking) {
        return d.picks.clone();
    }
    if let Some(d) = &state.points_pick {
        return d.picks.clone();
    }
    if let Some(d) = state.axis_pick.as_ref().filter(|d| d.picking.is_some()) {
        return [&d.axes.y, &d.axes.x].into_iter().flatten().flat_map(|p| p.picks.clone()).collect();
    }
    Vec::new()
}

/// The marks for this frame (`aspect`: the viewport's width over height).
#[must_use]
pub fn align_marks(state: &State, aspect: f64) -> AlignMarks {
    let mut out = AlignMarks::default();
    let mvp = state.cam.mvp(aspect);
    for g in draft_picks(state) {
        if state.hidden_in_view(&g.entity) {
            continue;
        }
        let Some(e) = model::find_by_name(&state.world, &g.entity) else { continue };
        let Some(features) = model::features_of(&state.world, &e) else { continue };
        let loc = camcraft_app::setup_align::view_pose(&state.world, &e).unwrap_or_default();
        let uv = |p: Point3| {
            let w = transform_point(p, loc.pos, loc.rot);
            project_uv(&mvp, [w.x, w.y, w.z])
        };
        match g.feature {
            FeatureSel::Face(fid) => {
                let Some(face) = features.faces.iter().find(|f| f.id == fid) else { continue };
                let Some(tris) = model::facets_of(&state.world, &e) else { continue };
                for t in face.facets.iter().filter_map(|&i| tris.get(i as usize)) {
                    if let (Some(a), Some(b), Some(c)) = (uv(t.a), uv(t.b), uv(t.c)) {
                        out.faces.push([a, b, c]);
                    }
                }
            }
            FeatureSel::Chain(_) => feature_segments(state, &g.entity, g.feature, aspect, &mut out.edges),
            FeatureSel::Vertex(vid) => {
                let v = feature::resolve_vertex(&features, vid).and_then(|vi| features.verts.get(vi as usize));
                if let Some(p) = v.and_then(|v| uv(Point3::new(v[0], v[1], v[2]))) {
                    out.points.push(p);
                }
            }
            // A curved face is never an alignment pick (refused by name);
            // its outline still draws on the GPU lines.
            FeatureSel::Patch(_) => {}
        }
    }
    out
}

/// Paint the marks over the viewport image `rect`: fills, then bold
/// edges, then discs, so a point on a picked face stays on top.
pub fn paint(painter: &egui::Painter, rect: egui::Rect, state: &State, aspect: f64) {
    let m = align_marks(state, aspect);
    if m == AlignMarks::default() {
        return;
    }
    let to_px = |uv: [f32; 2]| egui::pos2(rect.min.x + uv[0] * rect.width(), rect.min.y + uv[1] * rect.height());
    let [r, g, b] = PICK_RGB8;
    if !m.faces.is_empty() {
        let fill = egui::Color32::from_rgba_unmultiplied(r, g, b, FACE_ALPHA);
        let mut mesh = egui::Mesh::default();
        for tri in &m.faces {
            let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
            for uv in tri {
                mesh.colored_vertex(to_px(*uv), fill);
            }
            mesh.add_triangle(base, base.saturating_add(1), base.saturating_add(2));
        }
        painter.add(egui::Shape::mesh(mesh));
    }
    let ink = egui::Color32::from_rgb(r, g, b);
    for (a, bb) in &m.edges {
        painter.line_segment([to_px(*a), to_px(*bb)], egui::Stroke::new(EDGE_PX, ink));
    }
    let disc = egui::Color32::from_rgba_unmultiplied(r, g, b, POINT_ALPHA);
    let rim = egui::Stroke::new(1.5, egui::Color32::from_rgb(245, 240, 250));
    for p in &m.points {
        painter.circle(to_px(*p), POINT_RADIUS_PX, disc, rim);
    }
}
