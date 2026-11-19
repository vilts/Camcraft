//! DXF half-profile conversion for tool holders (#743): exports `SpunProfile`
//! and aligned STEP holder envelopes to clean-room ASCII DXF (explicit
//! `CENTRELINE` layer + open `PROFILE` chain of lines and arcs), and imports
//! DXF half-profiles back into validated [`SpunProfile`] values with named
//! refusals for every invalid or unaligned case.
//! Provenance: Autodesk DXF Reference (ASCII HEADER/TABLES/ENTITIES) and planar
//! projection onto the ISO 13399 aligned tool axis; original implementation.

use std::f64::consts::TAU;
use std::path::Path;

use camcraft_geometry::dxf::{self, Layer, LayeredPrimitive, Primitive, Unit, Units, write};
use camcraft_geometry::geo2d::Point2;
use camcraft_tools::holder_profile::HolderProfile;
use camcraft_tools::spun::{MIN_SEGMENT, SpunPoint, SpunProfile};

use crate::commands::ToolBodySpec;
use crate::geometry::Triangle;
use crate::schema::ToolBody;
use crate::tool_body::{self, BodyFrame};

pub use camcraft_tools::tool_profile_dxf::{CENTRELINE_LAYER, dxf_to_spun, dxf_to_spun_with_unit};
pub const PROFILE_LAYER: &str = "PROFILE";

/// Export a validated [`SpunProfile`] as ASCII DXF in millimetres with a
/// `CENTRELINE` line from `(0, 0)` to `(0, z_top)` and open `PROFILE`
/// entities in `(x = r, y = z)` on the `+X` side.
pub fn spun_to_dxf(profile: &SpunProfile) -> Result<String, String> {
    let points = profile.points();
    if points.len() < 3 {
        return Err("cannot export empty holder profile to DXF".into());
    }
    let z_top = points.iter().map(|p| p.z).fold(0.0_f64, f64::max);
    if !(z_top.is_finite() && z_top >= MIN_SEGMENT) {
        return Err("holder profile has no positive length along the tool axis".into());
    }
    let mut doc = dxf::Document {
        units: Units::Known(Unit::Millimeters),
        elevation: Some(0.0),
        layers: vec![
            Layer {
                name: CENTRELINE_LAYER.into(),
                flags: 0,
                color: 5,
                line_type: "CENTER".into(),
            },
            Layer {
                name: PROFILE_LAYER.into(),
                flags: 0,
                color: 7,
                line_type: "CONTINUOUS".into(),
            },
        ],
        primitives: Vec::with_capacity(points.len()),
        ..dxf::Document::default()
    };
    doc.primitives.push(LayeredPrimitive {
        layer: CENTRELINE_LAYER.into(),
        primitive: Primitive::Line {
            start: Point2::new(0.0, 0.0),
            end: Point2::new(0.0, z_top),
        },
    });
    for w in points.windows(2) {
        doc.primitives.push(LayeredPrimitive {
            layer: PROFILE_LAYER.into(),
            primitive: segment_to_dxf(w[0], w[1])?,
        });
    }
    write(&doc).map_err(|e| format!("DXF export failed: {e}"))
}

fn segment_to_dxf(a: SpunPoint, b: SpunPoint) -> Result<Primitive, String> {
    let pa = Point2::new(a.r, a.z);
    let pb = Point2::new(b.r, b.z);
    if a.arc == 0.0 {
        return Ok(Primitive::Line { start: pa, end: pb });
    }
    let dz = b.z - a.z;
    let dr = b.r - a.r;
    let len = dz.hypot(dr);
    if !(len.is_finite() && len > 0.0) {
        return Err("holder profile has a zero-length arc segment".into());
    }
    let rad = a.arc.abs().max(len * 0.5);
    let half = (len * 0.5 / rad).clamp(0.0, 1.0).asin();
    let back = rad * half.cos();
    let n = (dz / len, -dr / len);
    let mid = ((a.r + b.r) * 0.5, (a.z + b.z) * 0.5);
    let s = a.arc.signum();
    let center = Point2::new(mid.0 - s * back * n.0, mid.1 - s * back * n.1);
    let ta = (a.z - center.y).atan2(a.r - center.x);
    let tb = (b.z - center.y).atan2(b.r - center.x);
    let (start_angle, sweep_angle) = if a.arc > 0.0 {
        (ta, (tb - ta).rem_euclid(TAU))
    } else {
        (tb, (ta - tb).rem_euclid(TAU))
    };
    Ok(Primitive::Arc {
        center,
        radius: rad,
        start_angle,
        sweep_angle: sweep_angle.clamp(1e-12, TAU - 1e-12),
    })
}

/// Convert a spindle-frame [`ToolBody`] to a conservative [`SpunProfile`].
pub fn tool_body_to_spun(body: &ToolBody) -> Result<SpunProfile, String> {
    if let Some(ref spun) = body.spun {
        return Ok(spun.clone());
    }
    match &body.frame {
        BodyFrame::Spindle { profile, .. } => SpunProfile::conservative_from_bands(profile.bands())
            .map_err(|e| format!("holder profile conversion failed: {e}")),
        BodyFrame::Lathe { .. } => {
            Err("a lathe-frame holder body has no revolved spindle half-profile".into())
        }
    }
}

/// Export a spindle-frame [`ToolBody`] as a conservative half-profile DXF.
pub fn tool_body_to_dxf(body: &ToolBody) -> Result<String, String> {
    let spun = tool_body_to_spun(body)?;
    spun_to_dxf(&spun)
}

/// Convert placed tool-frame triangles (axis `+Z`, tip at `z = 0`) to a
/// conservative [`SpunProfile`] that envelops every non-revolved feature.
pub fn placed_mesh_to_spun_profile(tris: &[Triangle]) -> Result<SpunProfile, String> {
    let raw: Vec<[[f64; 3]; 3]> = tris
        .iter()
        .map(|t| {
            [
                [t.a.x, t.a.y, t.a.z],
                [t.b.x, t.b.y, t.b.z],
                [t.c.x, t.c.y, t.c.z],
            ]
        })
        .collect();
    let hp = HolderProfile::envelope(&raw).map_err(|e| e.to_string())?;
    SpunProfile::conservative_from_bands(hp.bands())
        .map_err(|e| format!("conservative half-profile conversion failed: {e}"))
}

/// Read a STEP file, require an unambiguous ISO 13399 alignment (#370), and
/// convert its conservative outer envelope into an editable [`SpunProfile`].
pub fn step_to_spun_profile(path: &Path) -> Result<SpunProfile, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if ext != "step" && ext != "stp" {
        return Err(
            "expected a STEP (.step or .stp) file with ISO 13399 coordinate systems; refusing to guess an axis".into(),
        );
    }
    let spec = ToolBodySpec::from_step_auto(path)?;
    let placed = tool_body::place(&spec.facets, spec.axis, spec.offset)?;
    placed_mesh_to_spun_profile(&placed)
}

/// Convert an ISO 13399 aligned STEP holder/body directly into ASCII DXF.
pub fn step_to_dxf(path: &Path) -> Result<String, String> {
    let spun = step_to_spun_profile(path)?;
    spun_to_dxf(&spun)
}
