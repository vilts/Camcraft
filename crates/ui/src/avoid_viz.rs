//! Viewport highlight for "Do not touch" protected faces and edges (#2055).
//! Provenance: our own overlay projection mirroring `align_pick_viz`, memoized
//! per input and camera matrix per `crates/ui/AGENTS.md` ("Frame cost"); no vendor material.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::sync::{Arc, LazyLock, Weak};

use camcraft_app::geometry::feature::{self, Features};
use camcraft_app::geometry::{Point3, Triangle, transform_point};
use camcraft_app::model;
use camcraft_app::operation::{self, FeatureSel, GeoSel};
use camcraft_app::schema::Location;
use flecs_ecs::prelude::Entity;

use crate::interact::project_uv;
use crate::state::State;

/// Semi-transparent fill opacity for protected faces and patches (0..255).
pub const AVOID_FACE_ALPHA: u8 = 88;
/// Stroke width (px) for protected edges.
pub const AVOID_EDGE_PX: f32 = 3.5;

const CACHE_CAP: usize = 8;

/// Screen-space geometry of `"Do not touch"` items on the currently selected operation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AvoidMarks {
    pub faces: Vec<[[f32; 2]; 3]>,
    pub edges: Vec<([f32; 2], [f32; 2])>,
}

#[derive(Clone)]
struct Feats(Weak<Features>);

impl PartialEq for Feats {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

#[derive(Clone)]
struct Tris(Weak<[Triangle]>);

impl PartialEq for Tris {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

#[derive(Clone, PartialEq)]
struct Body {
    name: String,
    hidden: bool,
    entity: Option<Entity>,
    features: Option<Feats>,
    facets: Option<Tris>,
    pose: Option<Location>,
}

#[derive(Clone, PartialEq)]
struct GeomKey {
    items: Vec<GeoSel>,
    bodies: Vec<Body>,
}

impl GeomKey {
    fn of(state: &State, items: &[GeoSel]) -> Self {
        let mut names: Vec<&str> = Vec::new();
        for g in items {
            if !names.contains(&g.entity.as_str()) {
                names.push(&g.entity);
            }
        }
        let bodies = names
            .into_iter()
            .map(|name| {
                let entity = model::find_by_name(&state.world, name);
                Body {
                    name: name.to_owned(),
                    hidden: state.hidden_in_view(name),
                    entity,
                    features: entity
                        .and_then(|e| model::features_of(&state.world, &e))
                        .map(|f| Feats(Arc::downgrade(&f))),
                    facets: entity
                        .and_then(|e| model::facets_of(&state.world, &e))
                        .map(|t| Tris(Arc::downgrade(&t))),
                    pose: entity.and_then(|e| camcraft_app::setup_align::view_pose(&state.world, &e)),
                }
            })
            .collect();
        Self { items: items.to_vec(), bodies }
    }
}

#[derive(Default)]
struct WorldGeom {
    tris: Vec<[Point3; 3]>,
    edges: Vec<(Point3, Point3)>,
}

struct GeomEntry {
    key: GeomKey,
    geom: Arc<WorldGeom>,
}

struct MarkEntry {
    key: GeomKey,
    mvp: [[u32; 4]; 4],
    marks: Arc<AvoidMarks>,
}

static EMPTY_MARKS: LazyLock<Arc<AvoidMarks>> = LazyLock::new(|| Arc::new(AvoidMarks::default()));

thread_local! {
    static GEOM_MEMO: RefCell<Vec<GeomEntry>> = const { RefCell::new(Vec::new()) };
    static MARK_MEMO: RefCell<Vec<MarkEntry>> = const { RefCell::new(Vec::new()) };
    static GEOM_BUILDS: Cell<u64> = const { Cell::new(0) };
    static MARK_BUILDS: Cell<u64> = const { Cell::new(0) };
}

/// Number of 3D world protection-geometry builds on this thread.
#[must_use]
pub fn avoid_geom_builds() -> u64 {
    GEOM_BUILDS.with(Cell::get)
}

/// Number of screen-space projection builds on this thread.
#[must_use]
pub fn avoid_mark_builds() -> u64 {
    MARK_BUILDS.with(Cell::get)
}

fn is_active(state: &State) -> bool {
    !state.op_ui.name.is_empty()
        && operation::do_not_touch_applies(state.op_ui.params.kind())
        && !state.op_ui.area.do_not_touch.picks.is_empty()
}

fn mvp_bits(mvp: &[[f32; 4]; 4]) -> [[u32; 4]; 4] {
    mvp.map(|row| row.map(f32::to_bits))
}

fn push_world_poly(
    loc: &Location,
    pts: &[[f64; 3]],
    closed: bool,
    out: &mut Vec<(Point3, Point3)>,
) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    let xform = |p: &[f64; 3]| transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
    let last = if closed { n } else { n - 1 };
    for i in 0..last {
        out.push((xform(&pts[i]), xform(&pts[(i + 1) % n])));
    }
}

fn build_world_geom(state: &State, items: &[GeoSel]) -> WorldGeom {
    let mut out = WorldGeom::default();
    for g in items {
        if state.hidden_in_view(&g.entity) {
            continue;
        }
        let Some(e) = model::find_by_name(&state.world, &g.entity) else {
            continue;
        };
        let Some(features) = model::features_of(&state.world, &e) else {
            continue;
        };
        let loc = camcraft_app::setup_align::view_pose(&state.world, &e).unwrap_or_default();
        let xform = |p: Point3| transform_point(p, loc.pos, loc.rot);
        match g.feature {
            FeatureSel::Face(fid) => {
                if let Some(face) = features.faces.iter().find(|f| f.id == fid) {
                    if let Some(tris) = model::facets_of(&state.world, &e) {
                        for t in face.facets.iter().filter_map(|&i| tris.get(i as usize)) {
                            out.tris.push([xform(t.a), xform(t.b), xform(t.c)]);
                        }
                    }
                    for l in &face.loops {
                        let pts: Vec<[f64; 3]> = l
                            .verts
                            .iter()
                            .filter_map(|&vi| features.verts.get(vi as usize).copied())
                            .collect();
                        push_world_poly(&loc, &pts, !matches!(l.kind, feature::LoopKind::Open), &mut out.edges);
                    }
                }
            }
            FeatureSel::Patch(pid) => {
                if let Some(patch) = model::patches(&features).iter().find(|p| p.id == pid)
                    && let Some(tris) = model::facets_of(&state.world, &e)
                {
                    let fids: HashSet<u64> = patch.faces.iter().copied().collect();
                    for face in features.faces.iter().filter(|f| fids.contains(&f.id)) {
                        for t in face.facets.iter().filter_map(|&i| tris.get(i as usize)) {
                            out.tris.push([xform(t.a), xform(t.b), xform(t.c)]);
                        }
                    }
                }
                if let Some(rings) = model::patch_rings(&features, pid) {
                    for r in rings.iter() {
                        let pts: Vec<[f64; 3]> = r
                            .verts
                            .iter()
                            .filter_map(|&vi| features.verts.get(vi as usize).copied())
                            .collect();
                        push_world_poly(&loc, &pts, r.closed, &mut out.edges);
                    }
                }
            }
            FeatureSel::Chain(cid) => {
                if let Some(chain) = model::resolve_chain(&features, cid) {
                    let pts = feature::chain_points(&features, &chain);
                    push_world_poly(&loc, &pts, chain.closed, &mut out.edges);
                }
            }
            // Sketch picks are refused as protection; nothing to draw.
            FeatureSel::Vertex(_) | FeatureSel::SketchChain(_) | FeatureSel::SketchLayer(_) => {}
        }
    }
    out
}

fn project_world(geom: &WorldGeom, mvp: &[[f32; 4]; 4]) -> AvoidMarks {
    let mut out = AvoidMarks::default();
    for [a, b, c] in &geom.tris {
        if let (Some(a), Some(b), Some(c)) = (
            project_uv(mvp, [a.x, a.y, a.z]),
            project_uv(mvp, [b.x, b.y, b.z]),
            project_uv(mvp, [c.x, c.y, c.z]),
        ) {
            out.faces.push([a, b, c]);
        }
    }
    for (p, q) in &geom.edges {
        if let (Some(a), Some(b)) = (
            project_uv(mvp, [p.x, p.y, p.z]),
            project_uv(mvp, [q.x, q.y, q.z]),
        ) && a != b
        {
            out.edges.push((a, b));
        }
    }
    out
}

fn world_geom(state: &State, key: &GeomKey) -> Arc<WorldGeom> {
    let hit = GEOM_MEMO.with(|m| {
        let mut m = m.borrow_mut();
        let i = m.iter().position(|e| e.key == *key)?;
        let entry = m.remove(i);
        let geom = Arc::clone(&entry.geom);
        m.push(entry);
        Some(geom)
    });
    if let Some(geom) = hit {
        return geom;
    }
    GEOM_BUILDS.with(|n| n.set(n.get().saturating_add(1)));
    let geom = Arc::new(build_world_geom(state, &key.items));
    GEOM_MEMO.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() >= CACHE_CAP {
            m.remove(0);
        }
        m.push(GeomEntry { key: key.clone(), geom: Arc::clone(&geom) });
    });
    geom
}

/// Memoized viewport-uv triangles and edge segments for the open operation's `"Do not touch"` list.
pub(crate) fn cached_marks(state: &State, aspect: f64) -> Arc<AvoidMarks> {
    if !is_active(state) {
        return Arc::clone(&EMPTY_MARKS);
    }
    let mvp = state.cam.mvp(aspect);
    let mvp_key = mvp_bits(&mvp);
    let key = GeomKey::of(state, &state.op_ui.area.do_not_touch.picks);
    let hit = MARK_MEMO.with(|m| {
        let mut m = m.borrow_mut();
        let i = m.iter().position(|e| e.mvp == mvp_key && e.key == key)?;
        let entry = m.remove(i);
        let marks = Arc::clone(&entry.marks);
        m.push(entry);
        Some(marks)
    });
    if let Some(marks) = hit {
        return marks;
    }
    let _span = crate::frame_cost::span("ui/viewport.avoid_marks.build");
    let geom = world_geom(state, &key);
    MARK_BUILDS.with(|n| n.set(n.get().saturating_add(1)));
    let marks = Arc::new(project_world(&geom, &mvp));
    MARK_MEMO.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() >= CACHE_CAP {
            m.remove(0);
        }
        m.push(MarkEntry { key, mvp: mvp_key, marks: Arc::clone(&marks) });
    });
    marks
}

/// Collect viewport-uv triangles and edge segments for the open operation's `"Do not touch"` list.
#[must_use]
pub fn avoid_marks(state: &State, aspect: f64) -> AvoidMarks {
    (*cached_marks(state, aspect)).clone()
}

/// Uncached fresh build for tests verifying cache consistency.
#[cfg(test)]
#[must_use]
pub fn avoid_marks_fresh(state: &State, aspect: f64) -> AvoidMarks {
    if !is_active(state) {
        return AvoidMarks::default();
    }
    let geom = build_world_geom(state, &state.op_ui.area.do_not_touch.picks);
    project_world(&geom, &state.cam.mvp(aspect))
}

/// Paint protected faces and edges over the viewport image `rect`.
pub fn paint(painter: &egui::Painter, rect: egui::Rect, state: &State, aspect: f64) {
    let m = cached_marks(state, aspect);
    if m.faces.is_empty() && m.edges.is_empty() {
        return;
    }
    let to_px = |uv: [f32; 2]| egui::pos2(rect.min.x + uv[0] * rect.width(), rect.min.y + uv[1] * rect.height());
    let rgb = super::egui_app::ground_ink::overlay(super::egui_app::ground_ink::Overlay::Cut);
    let r = (rgb[0] * 255.0).clamp(0.0, 255.0) as u8;
    let g = (rgb[1] * 255.0).clamp(0.0, 255.0) as u8;
    let b = (rgb[2] * 255.0).clamp(0.0, 255.0) as u8;
    if !m.faces.is_empty() {
        let fill = egui::Color32::from_rgba_unmultiplied(r, g, b, AVOID_FACE_ALPHA);
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
        painter.line_segment([to_px(*a), to_px(*bb)], egui::Stroke::new(AVOID_EDGE_PX, ink));
    }
}
