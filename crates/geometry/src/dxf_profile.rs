//! Shared 2D fixture/holder profile import, DXF round-trip export, and
//! STEP-facet silhouette derivation (#744, shared with #743).
//! Provenance: our own profile stitching/validation over Autodesk DXF primitives.

use std::f64::consts::PI;

use crate::dxf::{
    ExportEntity, LayeredPrimitive, Primitive, TextAnnotation, Unit, Units, parse, to_text,
};
use crate::geo2d::{COORD_LIMIT, Point2};
use crate::profile_mesh::{spun_loop, validate_and_orient_closed};
pub use crate::profile_derive::{derive_extruded_from_facets, derive_spun_from_facets};
pub use crate::profile_mesh::{extruded_mesh, spun_mesh};

const JOIN_TOL: f64 = 1e-3;
const AXIS_TOL: f64 = 1e-3;

/// Error returned when a DXF profile, depth, or geometry violates profile laws.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[error("{0}")]
pub struct ProfileError(pub String);

/// 2D profile vertex in `(u, v)` mm plus optional edge bulge (`tan(sweep / 4)`).
/// For spun profiles: `u = z` along centreline, `v = r >= 0` radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProfileVertex {
    pub u: f64,
    pub v: f64,
    pub bulge: f64,
}

/// Axis an extruded fixture profile spans along.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExtrudeAxis {
    X,
    Y,
    #[default]
    Z,
}

/// A turning half-profile (or closed half-section) relative to the spindle axis.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HalfProfile {
    pub points: Vec<ProfileVertex>,
}

/// A milling closed 2D profile in the plane perpendicular to the extrusion axis.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClosedProfile {
    pub points: Vec<ProfileVertex>,
}

#[derive(Clone, Copy, Debug)]
struct Seg {
    start: Point2,
    end: Point2,
    bulge: f64,
}

impl Seg {
    fn flipped(self) -> Self {
        Self { start: self.end, end: self.start, bulge: -self.bulge }
    }
}

fn dxf_scale(units: Units) -> Result<f64, ProfileError> {
    match units {
        Units::Known(u) => Ok(u.millimeters_per_unit()),
        Units::MissingOrUnitless => Ok(1.0),
        Units::Unsupported(code) => Err(ProfileError(format!(
            "DXF declares unsupported $INSUNITS code {code}"
        ))),
    }
}

fn scale_pt(p: Point2, scale: f64) -> Result<Point2, ProfileError> {
    let x = p.x * scale;
    let y = p.y * scale;
    if !(x.is_finite() && y.is_finite() && x.abs() < COORD_LIMIT && y.abs() < COORD_LIMIT) {
        return Err(ProfileError("DXF coordinate overflows millimetres".into()));
    }
    Ok(Point2::new(x, y))
}

fn reject_unsupported_skipped(doc: &crate::dxf::Document) -> Result<(), ProfileError> {
    for (kind, count) in &doc.summary.skipped {
        if *count > 0 && !matches!(kind.as_str(), "TEXT" | "MTEXT") {
            return Err(ProfileError(format!(
                "DXF contains unsupported entity '{kind}' for fixture profiles"
            )));
        }
    }
    Ok(())
}

fn is_centreline_layer(name: &str) -> bool {
    let up = name.trim().to_ascii_uppercase();
    matches!(up.as_str(), "CENTRELINE" | "CENTERLINE" | "AXIS")
}

fn is_depth_layer(name: &str) -> bool {
    let up = name.trim().to_ascii_uppercase();
    up == "DEPTH" || up.starts_with("DEPTH_") || up.starts_with("DEPTH=") || up.starts_with("DEPTH:")
}

fn parse_depth_token(s: &str, scale: f64) -> Option<f64> {
    let up = s.trim().to_ascii_uppercase();
    let rest = if let Some(r) = up.strip_prefix("DEPTH") {
        r.trim_start_matches(['=', ':', '_', ' ', '\t'])
    } else if !up.is_empty() && up.as_bytes()[0].is_ascii_digit() {
        up.as_str()
    } else {
        return None;
    };
    let split = rest
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '+' | '-' | 'e' | 'E')))
        .unwrap_or(rest.len());
    let val = rest[..split].parse::<f64>().ok()?;
    let unit_scale = match rest[split..].trim() {
        "" => scale,
        "MM" => 1.0,
        "IN" | "INCH" | "INCHES" => 25.4,
        _ => return None,
    };
    let scaled = val * unit_scale;
    (scaled.is_finite() && scaled > 0.0).then_some(scaled)
}

fn resolve_depth(
    doc: &crate::dxf::Document,
    scale: f64,
    explicit: Option<f64>,
) -> Result<f64, ProfileError> {
    if let Some(d) = explicit {
        return (d.is_finite() && d > 0.0)
            .then_some(d)
            .ok_or_else(|| ProfileError(format!("extrusion depth must be positive and finite (got {d})")));
    }
    let layers = doc.primitives.iter().map(|p| p.layer.as_str()).chain(doc.layers.iter().map(|l| l.name.as_str()));
    let candidates = doc
        .annotations
        .iter()
        .filter(|a| is_depth_layer(&a.layer) || a.text.to_ascii_uppercase().contains("DEPTH"))
        .map(|a| a.text.as_str())
        .chain(doc.annotations.iter().map(|a| a.text.as_str()))
        .chain(layers.filter(|n| is_depth_layer(n)));
    for token in candidates {
        if let Some(d) = parse_depth_token(token, scale) {
            return Ok(d);
        }
    }
    Err(ProfileError(
        "extruded fixture DXF requires a positive extrusion depth (pass depth or add DEPTH=<mm> annotation/layer)".into(),
    ))
}

fn push_arc_seg(segs: &mut Vec<Seg>, c: Point2, r: f64, a0: f64, sweep: f64) {
    let pt = |a: f64| Point2::new(c.x + r * a.cos(), c.y + r * a.sin());
    segs.push(Seg { start: pt(a0), end: pt(a0 + sweep), bulge: (sweep * 0.25).tan() });
}

fn primitives_to_segs(prims: &[&LayeredPrimitive], scale: f64) -> Result<Vec<Seg>, ProfileError> {
    let mut segs = Vec::new();
    for lp in prims {
        match lp.primitive {
            Primitive::Line { start, end } => {
                let (s, e) = (scale_pt(start, scale)?, scale_pt(end, scale)?);
                if (e.x - s.x).hypot(e.y - s.y) > 1e-9 {
                    segs.push(Seg { start: s, end: e, bulge: 0.0 });
                }
            }
            Primitive::Arc { center, radius, start_angle, sweep_angle } => {
                let (c, r) = (scale_pt(center, scale)?, radius * scale);
                if sweep_angle > PI {
                    let half = 0.5 * sweep_angle;
                    push_arc_seg(&mut segs, c, r, start_angle, half);
                    push_arc_seg(&mut segs, c, r, start_angle + half, half);
                } else {
                    push_arc_seg(&mut segs, c, r, start_angle, sweep_angle);
                }
            }
            Primitive::Circle { center, radius } => {
                let (c, r) = (scale_pt(center, scale)?, radius * scale);
                let (left, right) = (Point2::new(c.x - r, c.y), Point2::new(c.x + r, c.y));
                segs.push(Seg { start: left, end: right, bulge: 1.0 });
                segs.push(Seg { start: right, end: left, bulge: 1.0 });
            }
        }
    }
    Ok(segs)
}

fn stitch_chain(mut segs: Vec<Seg>) -> Result<(Vec<ProfileVertex>, bool), ProfileError> {
    if segs.is_empty() {
        return Err(ProfileError("DXF profile contains no non-degenerate segments".into()));
    }
    let first = segs.remove(0);
    let mut chain = vec![first];
    while !segs.is_empty() {
        let Some(head) = chain.first().map(|s| s.start) else { break };
        let Some(tail) = chain.last().map(|s| s.end) else { break };
        let dist = |a: Point2, b: Point2| (a.x - b.x).hypot(a.y - b.y);
        if let Some(pos) = segs.iter().position(|s| dist(s.start, tail) <= JOIN_TOL) {
            chain.push(segs.remove(pos));
        } else if let Some(pos) = segs.iter().position(|s| dist(s.end, tail) <= JOIN_TOL) {
            chain.push(segs.remove(pos).flipped());
        } else if let Some(pos) = segs.iter().position(|s| dist(s.end, head) <= JOIN_TOL) {
            chain.insert(0, segs.remove(pos));
        } else if let Some(pos) = segs.iter().position(|s| dist(s.start, head) <= JOIN_TOL) {
            chain.insert(0, segs.remove(pos).flipped());
        } else {
            return Err(ProfileError(
                "DXF profile contains disconnected or open gaps between segments".into(),
            ));
        }
    }
    let closed = chain
        .first()
        .zip(chain.last())
        .is_some_and(|(a, b)| (a.start.x - b.end.x).hypot(a.start.y - b.end.y) <= JOIN_TOL);
    let mut verts: Vec<ProfileVertex> = chain
        .iter()
        .map(|s| ProfileVertex { u: s.start.x, v: s.start.y, bulge: s.bulge })
        .collect();
    if !closed
        && let Some(last) = chain.last()
    {
        verts.push(ProfileVertex { u: last.end.x, v: last.end.y, bulge: 0.0 });
    }
    Ok((verts, closed))
}

fn pick_profile_primitives(doc: &crate::dxf::Document) -> Vec<&LayeredPrimitive> {
    let has_profile_layer = doc
        .primitives
        .iter()
        .any(|p| p.layer.eq_ignore_ascii_case("PROFILE"));
    if has_profile_layer {
        doc.primitives
            .iter()
            .filter(|p| p.layer.eq_ignore_ascii_case("PROFILE"))
            .collect()
    } else {
        doc.primitives
            .iter()
            .filter(|p| !is_centreline_layer(&p.layer))
            .collect()
    }
}

/// Extract a turning fixture spun half-profile from DXF bytes (`CENTRELINE` + half-profile).
pub fn spun_from_dxf(bytes: &[u8]) -> Result<HalfProfile, ProfileError> {
    let doc = parse(bytes).map_err(|e| ProfileError(e.to_string()))?;
    reject_unsupported_skipped(&doc)?;
    let scale = dxf_scale(doc.units)?;
    let centrelines: Vec<_> = doc
        .primitives
        .iter()
        .filter(|p| is_centreline_layer(&p.layer))
        .collect();
    if centrelines.is_empty() {
        return Err(ProfileError(
            "DXF spun profile requires a centreline LINE on layer CENTRELINE (or CENTERLINE / AXIS)".into(),
        ));
    }
    if centrelines.len() > 1 {
        return Err(ProfileError(
            "DXF spun profile must have exactly one centreline on CENTRELINE".into(),
        ));
    }
    let Primitive::Line { start: c0, end: c1 } = centrelines[0].primitive else {
        return Err(ProfileError("DXF centreline entity must be a LINE".into()));
    };
    let a0 = scale_pt(c0, scale)?;
    let a1 = scale_pt(c1, scale)?;
    let len = (a1.x - a0.x).hypot(a1.y - a0.y);
    if len <= 1e-9 {
        return Err(ProfileError("DXF centreline has zero length".into()));
    }
    let u_dir = Point2::new((a1.x - a0.x) / len, (a1.y - a0.y) / len);
    let v_dir = Point2::new(-u_dir.y, u_dir.x);
    let v_off = a0.x * v_dir.x + a0.y * v_dir.y;
    let to_zr = |p: Point2| (p.x * u_dir.x + p.y * u_dir.y, (p.x * v_dir.x + p.y * v_dir.y) - v_off);

    let prof_prims = pick_profile_primitives(&doc);
    if prof_prims.is_empty() {
        return Err(ProfileError("DXF spun profile contains no profile entities".into()));
    }
    let raw_segs = primitives_to_segs(&prof_prims, scale)?;
    let mut r_vals = Vec::with_capacity(raw_segs.len() * 2);
    for s in &raw_segs {
        r_vals.push(to_zr(s.start).1);
        r_vals.push(to_zr(s.end).1);
    }
    let r_min = r_vals.iter().copied().fold(f64::INFINITY, f64::min);
    let r_max = r_vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if r_min < -AXIS_TOL && r_max > AXIS_TOL {
        return Err(ProfileError(
            "DXF spun half-profile crosses both sides of the centreline".into(),
        ));
    }
    let flip_v = r_max <= AXIS_TOL && r_min < -AXIS_TOL;
    let zr_segs: Vec<Seg> = raw_segs
        .into_iter()
        .map(|s| {
            let (z0, mut r0) = to_zr(s.start);
            let (z1, mut r1) = to_zr(s.end);
            if flip_v {
                r0 = -r0;
                r1 = -r1;
            }
            Seg {
                start: Point2::new(z0, if r0.abs() < AXIS_TOL { 0.0 } else { r0 }),
                end: Point2::new(z1, if r1.abs() < AXIS_TOL { 0.0 } else { r1 }),
                bulge: if flip_v { -s.bulge } else { s.bulge },
            }
        })
        .collect();
    let (mut verts, closed) = stitch_chain(zr_segs)?;
    // HalfProfile encodes closure with a repeated endpoint. The preceding
    // vertex keeps the closing edge bulge, including for annular sections.
    if closed && let Some(&first) = verts.first() {
        verts.push(ProfileVertex { bulge: 0.0, ..first });
    }
    let half = HalfProfile { points: verts };
    let _ = spun_loop(&half)?;
    Ok(half)
}

/// Extract a milling extruded closed profile + depth from DXF bytes.
pub fn extruded_from_dxf(
    bytes: &[u8],
    depth_mm: Option<f64>,
) -> Result<(ClosedProfile, f64), ProfileError> {
    let doc = parse(bytes).map_err(|e| ProfileError(e.to_string()))?;
    reject_unsupported_skipped(&doc)?;
    let scale = dxf_scale(doc.units)?;
    let depth = resolve_depth(&doc, scale, depth_mm)?;
    let prof_prims = pick_profile_primitives(&doc);
    if prof_prims.is_empty() {
        return Err(ProfileError("DXF extruded profile contains no geometry".into()));
    }
    let segs = primitives_to_segs(&prof_prims, scale)?;
    let (verts, closed) = stitch_chain(segs)?;
    if !closed {
        let gap = verts
            .first()
            .zip(verts.last())
            .map_or(0.0, |(a, b)| (a.u - b.u).hypot(a.v - b.v));
        return Err(ProfileError(format!(
            "DXF extruded profile is open (endpoint gap {gap:.3} mm)"
        )));
    }
    let profile = ClosedProfile { points: verts };
    let _ = validate_and_orient_closed(&profile.points)?;
    Ok((profile, depth))
}

/// Serialize a spun half-profile into ASCII DXF (`CENTRELINE` + `PROFILE` layers).
pub fn spun_to_dxf(profile: &HalfProfile) -> Result<String, ProfileError> {
    let _ = spun_loop(profile)?;
    if profile.points.len() < 2 {
        return Err(ProfileError("spun half-profile needs at least two points".into()));
    }
    let (mut z_lo, mut z_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in &profile.points {
        z_lo = z_lo.min(p.u);
        z_hi = z_hi.max(p.u);
    }
    let pad = ((z_hi - z_lo) * 0.25).max(5.0);
    let mut entities = vec![ExportEntity::Line {
        layer: "CENTRELINE".into(),
        start: Point2::new(z_lo - pad, 0.0),
        end: Point2::new(z_hi + pad, 0.0),
    }];
    entities.push(ExportEntity::LwPolyline {
        layer: "PROFILE".into(),
        closed: false,
        vertices: profile
            .points
            .iter()
            .map(|p| (Point2::new(p.u, p.v), p.bulge))
            .collect(),
    });
    to_text(Some(Unit::Millimeters), &["CENTRELINE", "PROFILE"], &entities, &[])
        .map_err(|e| ProfileError(e.to_string()))
}

/// Serialize an extruded closed profile and depth into ASCII DXF (`PROFILE` + `DEPTH`).
pub fn extruded_to_dxf(profile: &ClosedProfile, depth_mm: f64) -> Result<String, ProfileError> {
    if !depth_mm.is_finite() || depth_mm <= 0.0 {
        return Err(ProfileError("extrusion depth must be positive and finite".into()));
    }
    let _ = validate_and_orient_closed(&profile.points)?;
    let entities = vec![ExportEntity::LwPolyline {
        layer: "PROFILE".into(),
        closed: true,
        vertices: profile
            .points
            .iter()
            .map(|p| (Point2::new(p.u, p.v), p.bulge))
            .collect(),
    }];
    let anns = vec![TextAnnotation {
        layer: "DEPTH".into(),
        text: format!("DEPTH={depth_mm}"),
        pos: Point2::new(0.0, 0.0),
    }];
    to_text(Some(Unit::Millimeters), &["PROFILE", "DEPTH"], &entities, &anns)
        .map_err(|e| ProfileError(e.to_string()))
}
