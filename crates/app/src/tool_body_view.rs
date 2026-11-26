//! Tool-body editing and display reads (#371): what the tool editor and
//! the simulation need from a body without committing anything - the
//! file read, the six axis presets, the placement preview (the same
//! place-and-envelope `SetToolBody` runs), the tip/axis picks on the
//! model-frame mesh, the body posed at a tool position for drawing, and
//! the drawn spun holder's preview (#93).
//! Specced from plain linear algebra (Rodrigues' rotation formula, the
//! ray/triangle crossing `pick::soup_hit` already uses); our own design,
//! no vendor file or source consulted.
//!
//! Nothing here writes the world: picks and previews are ephemeral, and
//! the one writer stays `SetToolBody` (AGENTS.md: everything is a
//! command).

use std::path::Path;

use flecs_ecs::prelude::World;

use crate::geometry::{Point3, Triangle};
use crate::model;
use crate::schema::ToolBody;
use crate::tool_body;
use crate::tools::Tool;
use camcraft_tools::spun::{SpunPoint, SpunProfile};

/// The six model directions the editor offers as the tool axis.
pub const AXIS_CHOICES: [(&str, [f64; 3]); 6] = [
    ("+Z", [0.0, 0.0, 1.0]),
    ("-Z", [0.0, 0.0, -1.0]),
    ("+X", [1.0, 0.0, 0.0]),
    ("-X", [-1.0, 0.0, 0.0]),
    ("+Y", [0.0, 1.0, 0.0]),
    ("-Y", [0.0, -1.0, 0.0]),
];

/// The preset name of `axis`, if it is one of [`AXIS_CHOICES`] (within
/// float dust after normalizing).
#[must_use]
pub fn axis_label(axis: [f64; 3]) -> Option<&'static str> {
    let a = unit(axis)?;
    AXIS_CHOICES.iter().find(|(_, d)| dot(a, *d) > 1.0 - 1e-9).map(|(n, _)| *n)
}

/// What loading a body from disk found: the source name, its facets,
/// the auto-alignment from ISO 13399 coordinate systems (#370) if present,
/// and all named coordinate systems carried by the file.
pub struct LoadedBody {
    pub source: String,
    pub facets: Vec<Triangle>,
    /// `Ok((axis, offset))` when aligned from ISO 13399 coordinate
    /// systems; `Err(why)` when falling back to numeric placement.
    pub alignment: Result<([f64; 3], f64), String>,
    pub csys: Vec<crate::tool_body_assembly::BodyCsys>,
}

/// Read a STEP / STL / OBJ for a tool body: its file name, its
/// model-frame facets, and automatic placement from ISO 13399 coordinate
/// systems (#370) if present.
pub fn load_aligned(path: &Path) -> Result<LoadedBody, String> {
    // #361: the one body-file door (a reduced-fidelity mesh is refused).
    let crate::tool_body_file::BodyFile { source, facets } = crate::tool_body_file::read(path)?;
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    let (alignment, csys) = if ext == "step" || ext == "stp" {
        match std::fs::read(path) {
            Ok(data) => match crate::geometry::import::step_csys::read_tool_csys(&data) {
                Ok(report) => {
                    let csys = report.systems.iter().map(crate::tool_body_assembly::BodyCsys::from_named).collect();
                    (report.align_tool_body(), csys)
                }
                Err(e) => (Err(format!("coordinate systems unreadable: {e}")), Vec::new()),
            },
            Err(e) => (Err(format!("I/O error: {e}")), Vec::new()),
        }
    } else {
        (Err("not a STEP file (no coordinate systems)".into()), Vec::new())
    };
    Ok(LoadedBody { source, facets, alignment, csys })
}

/// Read a STEP / STL / OBJ for a tool body: its file name and its
/// model-frame facets. Refuses an unreadable or empty file by name.
pub fn load(path: &Path) -> Result<(String, Vec<Triangle>), String> {
    let loaded = load_aligned(path)?;
    Ok((loaded.source, loaded.facets))
}

/// The placement as `SetToolBody` would judge it, without landing it:
/// the body placed in the tool frame with its envelope, or the refusal
/// by name. (The tool-fit law is the command's; this sees the mesh.)
pub fn preview(model: &[Triangle], axis: [f64; 3], offset: f64) -> Result<ToolBody, String> {
    let placed = tool_body::place(model, axis, offset)?;
    tool_body::build("preview", axis, offset, placed)
}

/// A drawn holder profile (#93) as `SetToolBody::spun` would judge it
/// against `tool`, without landing it: the validated profile, or the
/// refusal by name (the outline's own law, then the tool fit - lathe
/// kinds and a holder reaching into the flutes). Builds no facets, so
/// the editor can run it every frame while the table is edited.
pub fn spun_preview(tool: &Tool, points: &[SpunPoint]) -> Result<SpunProfile, String> {
    let spun = SpunProfile::new(points.to_vec()).map_err(|e| format!("holder profile: {e}"))?;
    let probe = ToolBody {
        source: tool_body::SPUN_SOURCE.to_owned(),
        facets: std::sync::Arc::from(Vec::new()),
        frame: tool_body::BodyFrame::Spindle { axis: [0.0, 0.0, 1.0], offset: 0.0, profile: spun.holder_profile() },
        spun: None,
        machine_default: false,
        library: None,
        pieces: Vec::new(),
    };
    tool_body::fits(tool, &probe)?;
    Ok(spun)
}

/// What a click on the model-frame body found: the facet, the hit
/// point, the facet's corner nearest the hit, and its unit normal (`None`
/// for a zero-area facet).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyPick {
    pub facet: usize,
    pub hit: [f64; 3],
    pub vertex: [f64; 3],
    pub normal: Option<[f64; 3]>,
}

/// Cast `origin + t*dir` at the body's facets (model frame).
#[must_use]
pub fn pick(model: &[Triangle], origin: [f64; 3], dir: [f64; 3]) -> Option<BodyPick> {
    let (facet, hit) = crate::pick::soup_hit(model, origin, dir)?;
    let t = model.get(facet)?;
    let corners = [t.a, t.b, t.c].map(|p| [p.x, p.y, p.z]);
    let d2 = |q: [f64; 3]| {
        let e = sub(q, hit);
        dot(e, e)
    };
    let vertex = corners.into_iter().min_by(|a, b| d2(*a).total_cmp(&d2(*b)))?;
    let normal = unit(cross(sub(corners[1], corners[0]), sub(corners[2], corners[0])));
    Some(BodyPick { facet, hit, vertex, normal })
}

/// The tip offset that puts `point` (model frame) at the tool tip along
/// `axis`: the placement maps height as `dot(q, axis) + offset`, so the
/// tip (height 0) at `point` is `-dot(point, axis)`.
#[must_use]
pub fn tip_offset_at(point: [f64; 3], axis: [f64; 3]) -> Option<f64> {
    let a = unit(axis)?;
    let o = -dot(point, a);
    o.is_finite().then_some(o)
}

/// Tool-frame facets (tip at the origin, +Z to the spindle) posed at a
/// tool position: `tip` in world, `axis` the unit tip-to-spindle
/// direction (`sim::Frame::axis`). +Z maps onto `axis` by the shortest
/// rotation; the turn about the axis is free (the body is judged by its
/// revolved envelope), and a vertical tool takes the identity.
#[must_use]
pub fn posed(facets: &[Triangle], tip: [f64; 3], axis: [f64; 3]) -> Vec<Triangle> {
    let a = unit(axis).unwrap_or([0.0, 0.0, 1.0]);
    let (c, k) = (a[2], cross([0.0, 0.0, 1.0], a));
    let s = dot(k, k).sqrt();
    let rot = |v: [f64; 3]| -> [f64; 3] {
        if s < 1e-12 {
            // Parallel: identity, or a half turn about X when flipped.
            return if c >= 0.0 { v } else { [v[0], -v[1], -v[2]] };
        }
        // Rodrigues about the unit k by the angle whose cos is c, sin s.
        let k = [k[0] / s, k[1] / s, k[2] / s];
        let kv = cross(k, v);
        let kd = dot(k, v) * (1.0 - c);
        [v[0] * c + kv[0] * s + k[0] * kd, v[1] * c + kv[1] * s + k[1] * kd, v[2] * c + kv[2] * s + k[2] * kd]
    };
    let map = |p: Point3| {
        let r = rot([p.x, p.y, p.z]);
        Point3::new(r[0] + tip[0], r[1] + tip[1], r[2] + tip[2])
    };
    facets.iter().map(|t| Triangle::new(map(t.a), map(t.b), map(t.c))).collect()
}

/// Facets around the axis per revolved display ring.
const RING: usize = 32;

/// The whole tool for display, tool frame (tip at the origin, +Z up):
/// the cutter and shank revolved from `Tool::profile` up to the shank
/// top - carried up to the body's foot when the body stands clear of it,
/// the gap the fixture laws also fill with shank - and then the body's
/// own facets in place of the holder cylinder. Without the steel below
/// it a body floats a shank length above the tip, off the frame of a
/// part-sized view (#371 feedback).
#[must_use]
pub fn assembly(tool: &Tool, body: &ToolBody) -> Vec<Triangle> {
    let zs = tool.flute_len.max(0.0) + tool.shank_len.max(0.0);
    // A lathe-frame body (#369) has no spindle envelope: the shank stops at its own top.
    let top = body.profile().map_or(zs, |p| zs.max(p.bottom()));
    let rs = tool.shank_d.max(0.0) * 0.5;
    let mut prof: Vec<(f64, f64)> =
        tool.profile().into_iter().filter(|&(z, r)| z.is_finite() && r.is_finite() && z < zs - 1e-9).collect();
    prof.extend([(zs, rs), (top, rs), (top, 0.0)]);
    let mut out = revolve(&prof);
    out.extend(body.facets.iter().copied());
    out
}

/// A `(z, r)` meridian spun about +Z into triangles; rings at r = 0
/// close to a point, flat steps make annuli.
pub(crate) fn revolve(prof: &[(f64, f64)]) -> Vec<Triangle> {
    let at = |z: f64, r: f64, k: usize| {
        let a = std::f64::consts::TAU * k as f64 / RING as f64;
        Point3::new(r * a.cos(), r * a.sin(), z)
    };
    let mut out = Vec::new();
    for w in prof.windows(2) {
        let ((z0, r0), (z1, r1)) = (w[0], w[1]);
        if (z0 - z1).abs() < 1e-12 && (r0 - r1).abs() < 1e-12 {
            continue;
        }
        for k in 0..RING {
            let (a, b, c, d) = (at(z0, r0, k), at(z0, r0, k + 1), at(z1, r1, k + 1), at(z1, r1, k));
            if r0 > 1e-12 {
                out.push(Triangle::new(a, b, c));
            }
            if r1 > 1e-12 {
                out.push(Triangle::new(a, c, d));
            }
        }
    }
    out
}

/// The body on the tool operation `op` runs, if that tool has one.
#[must_use]
pub fn body_for_operation(world: &World, op: &str) -> Option<ToolBody> {
    let e = model::find_by_name(world, op)?;
    let tool = model::operation_of(world, &e)?.tool;
    let te = model::find_by_name(world, &tool)?;
    tool_body::body_of(world, &te)
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let n = dot(a, a).sqrt();
    (n.is_finite() && n > 1e-12).then(|| [a[0] / n, a[1] / n, a[2] / n])
}
