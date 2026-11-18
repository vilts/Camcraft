//! Editable spun turning and extruded milling fixture profiles (#744).
//! Provenance: clean-room CAD/CAM profile representation built over `camcraft_geometry`.

use serde::{Deserialize, Serialize};

use camcraft_geometry::dxf_profile::{
    ClosedProfile, ExtrudeAxis as GeoAxis, HalfProfile, ProfileVertex, extruded_from_dxf,
    extruded_to_dxf, spun_from_dxf, spun_to_dxf,
};
use camcraft_geometry::geometry::{Mesh, Triangle};
use camcraft_geometry::profile_derive::{derive_extruded_from_facets, derive_spun_from_facets};
use camcraft_geometry::profile_mesh::{extruded_mesh, spun_mesh};
use camcraft_tools::spun::{SpunPoint as ToolSpunPoint, SpunProfile as ToolSpunProfile};

/// Axis along which a 2D closed profile is extruded into a milling fixture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExtrudeAxis {
    X,
    Y,
    Z,
}

impl ExtrudeAxis {
    /// Human-readable axis tag (`X`, `Y`, `Z`).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::X => "X",
            Self::Y => "Y",
            Self::Z => "Z",
        }
    }

    /// Parse axis name (`"x"`, `"y"`, `"z"`, case-insensitive).
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "x" => Ok(Self::X),
            "y" => Ok(Self::Y),
            "z" => Ok(Self::Z),
            other => Err(format!("invalid extrude axis '{other}' (expected X, Y, or Z)")),
        }
    }

    #[must_use]
    pub(crate) const fn to_geo(self) -> GeoAxis {
        match self {
            Self::X => GeoAxis::X,
            Self::Y => GeoAxis::Y,
            Self::Z => GeoAxis::Z,
        }
    }
}

/// One vertex of a revolved fixture half-profile (`z` axial, `r >= 0` radial, `bulge = tan(theta/4)`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpunPoint {
    pub z: f64,
    pub r: f64,
    #[serde(default)]
    pub bulge: f64,
}

impl SpunPoint {
    /// Straight segment vertex on `(z, r)`.
    #[must_use]
    pub const fn line(z: f64, r: f64) -> Self {
        Self { z, r, bulge: 0.0 }
    }
}

/// One vertex of an extruded 2D closed fixture profile (`(u, v)` cross-section, `bulge = tan(theta/4)`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProfilePoint {
    pub u: f64,
    pub v: f64,
    #[serde(default)]
    pub bulge: f64,
}

impl ProfilePoint {
    /// Straight segment vertex at `(u, v)`.
    #[must_use]
    pub const fn line(u: f64, v: f64) -> Self {
        Self { u, v, bulge: 0.0 }
    }
}

/// Parametric 2D fixture geometry stored on `FixtureDef` and serialized in `.camproj` / `.camfixtures`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FixtureProfile {
    Spun {
        points: Vec<SpunPoint>,
    },
    Extruded {
        points: Vec<ProfilePoint>,
        axis: ExtrudeAxis,
        depth: f64,
    },
}

impl FixtureProfile {
    /// Short variant label for UI and logs.
    #[must_use]
    pub const fn kind_label(&self) -> &'static str {
        match self {
            Self::Spun { .. } => "Spun",
            Self::Extruded { .. } => "Extruded",
        }
    }

    /// Parse a turning spun half-profile from ASCII DXF (`CENTRELINE` + half-profile).
    pub fn spun_from_dxf(bytes: &[u8]) -> Result<Self, String> {
        let half = spun_from_dxf(bytes).map_err(|e| e.to_string())?;
        Ok(Self::from_half(&half))
    }

    /// Parse a milling extruded closed profile + depth from ASCII DXF.
    pub fn extruded_from_dxf(
        bytes: &[u8],
        axis: ExtrudeAxis,
        depth_mm: Option<f64>,
    ) -> Result<Self, String> {
        let (closed, depth) = extruded_from_dxf(bytes, depth_mm).map_err(|e| e.to_string())?;
        Ok(Self::Extruded {
            points: closed
                .points
                .into_iter()
                .map(|p| ProfilePoint { u: p.u, v: p.v, bulge: p.bulge })
                .collect(),
            axis,
            depth,
        })
    }

    /// Derive a spun half-profile envelope around `+Z` from a solid triangle mesh.
    pub fn derive_spun(facets: &[Triangle]) -> Result<Self, String> {
        let half = derive_spun_from_facets(facets).map_err(|e| e.to_string())?;
        Ok(Self::from_half(&half))
    }

    /// Derive an extruded 2D silhouette + depth along `axis` from a solid triangle mesh.
    pub fn derive_extruded(facets: &[Triangle], axis: ExtrudeAxis) -> Result<Self, String> {
        let (closed, depth) = derive_extruded_from_facets(facets, axis.to_geo()).map_err(|e| e.to_string())?;
        Ok(Self::Extruded {
            points: closed
                .points
                .into_iter()
                .map(|p| ProfilePoint { u: p.u, v: p.v, bulge: p.bulge })
                .collect(),
            axis,
            depth,
        })
    }

    /// Validate profile geometry and build its closed outward-facing triangle mesh.
    pub fn mesh(&self) -> Result<Mesh, String> {
        let tris = match self {
            Self::Spun { .. } => spun_mesh(&self.to_half()).map_err(|e| e.to_string())?,
            Self::Extruded { axis, depth, .. } => {
                extruded_mesh(&self.to_closed(), axis.to_geo(), *depth).map_err(|e| e.to_string())?
            }
        };
        Ok(Mesh::from_facets(tris))
    }

    /// Bound the visual mesh approximation error for collision checks.
    /// A positive bound prevents the mesh from clearing a fixture box hit.
    #[must_use]
    pub fn collision_chord_error(&self) -> f64 {
        match self {
            Self::Spun { .. } => camcraft_geometry::profile_mesh::spun_chord_error(&self.to_half()),
            Self::Extruded { .. } => camcraft_geometry::profile_mesh::profile_chord_error(&self.to_closed().points, true),
        }
    }

    /// Validate profile coordinates, arc parameters, and manifold constraints.
    pub fn check(&self) -> Result<(), String> {
        self.mesh().map(|_| ())
    }

    /// Export this fixture profile to ASCII DXF text.
    pub fn to_dxf(&self) -> Result<String, String> {
        match self {
            Self::Spun { .. } => spun_to_dxf(&self.to_half()).map_err(|e| e.to_string()),
            Self::Extruded { depth, .. } => {
                extruded_to_dxf(&self.to_closed(), *depth).map_err(|e| e.to_string())
            }
        }
    }

    /// Convert a `#743` tool/holder `SpunProfile` into a `FixtureProfile::Spun`.
    pub fn from_tool_spun(tool: &ToolSpunProfile) -> Result<Self, String> {
        let raw = tool.points();
        if raw.len() < 2 {
            return Err("tool spun profile requires at least 2 points".into());
        }
        let mut points = Vec::with_capacity(raw.len());
        for (i, &p) in raw.iter().enumerate() {
            let bulge = if let Some(&next) = raw.get(i + 1)
                && p.arc.abs() > 1e-9
            {
                arc_to_bulge(p.z, p.r, next.z, next.r, p.arc)?
            } else {
                0.0
            };
            points.push(SpunPoint { z: p.z, r: p.r, bulge });
        }
        let prof = Self::Spun { points };
        prof.check()?;
        Ok(prof)
    }

    /// Convert a `FixtureProfile::Spun` (with `z >= 0` endpoints on axis) to `#743` `SpunProfile`.
    pub fn to_tool_spun(&self) -> Result<ToolSpunProfile, String> {
        let Self::Spun { points } = self else {
            return Err("only spun fixture profiles convert to tool SpunProfile".into());
        };
        let mut out = Vec::with_capacity(points.len());
        for (i, &p) in points.iter().enumerate() {
            let arc = if let Some(&next) = points.get(i + 1)
                && p.bulge.abs() > 1e-9
            {
                bulge_to_arc(p.z, p.r, next.z, next.r, p.bulge)
            } else {
                0.0
            };
            out.push(ToolSpunPoint { z: p.z, r: p.r, arc });
        }
        ToolSpunProfile::new(out).map_err(|e| e.to_string())
    }

    fn from_half(half: &HalfProfile) -> Self {
        Self::Spun {
            points: half
                .points
                .iter()
                .map(|p| SpunPoint { z: p.u, r: p.v, bulge: p.bulge })
                .collect(),
        }
    }

    fn to_half(&self) -> HalfProfile {
        match self {
            Self::Spun { points } => HalfProfile {
                points: points
                    .iter()
                    .map(|p| ProfileVertex { u: p.z, v: p.r, bulge: p.bulge })
                    .collect(),
            },
            Self::Extruded { .. } => HalfProfile { points: Vec::new() },
        }
    }

    fn to_closed(&self) -> ClosedProfile {
        match self {
            Self::Extruded { points, .. } => ClosedProfile {
                points: points
                    .iter()
                    .map(|p| ProfileVertex { u: p.u, v: p.v, bulge: p.bulge })
                    .collect(),
            },
            Self::Spun { .. } => ClosedProfile { points: Vec::new() },
        }
    }
}

fn bulge_midpoint_r(z0: f64, r0: f64, z1: f64, r1: f64, bulge: f64) -> f64 {
    let dz = z1 - z0;
    let dr = r1 - r0;
    let chord = dz.hypot(dr);
    if chord < 1e-9 || bulge.abs() < 1e-9 {
        return 0.5 * (r0 + r1);
    }
    let h = chord * 0.5 * (1.0 - bulge * bulge) / (2.0 * bulge);
    let cy = 0.5 * (r0 + r1) + (dz / chord) * h;
    let cx = 0.5 * (z0 + z1) + (-dr / chord) * h;
    let radius = (z0 - cx).hypot(r0 - cy);
    let a0 = (r0 - cy).atan2(z0 - cx);
    let mid_a = a0 + 2.0 * bulge.atan();
    cy + radius * mid_a.sin()
}

fn arc_to_bulge(z0: f64, r0: f64, z1: f64, r1: f64, arc: f64) -> Result<f64, String> {
    let chord = (z1 - z0).hypot(r1 - r0);
    if chord < 1e-9 {
        return Ok(0.0);
    }
    let rad = arc.abs();
    let half = 0.5 * chord;
    if rad < half - 1e-6 {
        return Err(format!("arc radius {rad:.3} is smaller than half chord {half:.3}"));
    }
    let theta = 2.0 * (half / rad).min(1.0).asin();
    let mag = (theta * 0.25).tan();
    let avg_r = 0.5 * (r0 + r1);
    let mid_pos = bulge_midpoint_r(z0, r0, z1, r1, mag);
    let want_higher_r = arc > 0.0;
    if (mid_pos >= avg_r) == want_higher_r {
        Ok(mag)
    } else {
        Ok(-mag)
    }
}

fn bulge_to_arc(z0: f64, r0: f64, z1: f64, r1: f64, bulge: f64) -> f64 {
    let chord = (z1 - z0).hypot(r1 - r0);
    if chord < 1e-9 || bulge.abs() < 1e-9 {
        return 0.0;
    }
    let theta = 4.0 * bulge.atan();
    let sin_half = (0.5 * theta.abs()).sin();
    if sin_half < 1e-9 {
        return 0.0;
    }
    let rad = (0.5 * chord) / sin_half;
    let mid_r = bulge_midpoint_r(z0, r0, z1, r1, bulge);
    let avg_r = 0.5 * (r0 + r1);
    if (mid_r - avg_r).abs() > 1e-9 {
        if mid_r > avg_r { rad } else { -rad }
    } else if bulge < 0.0 {
        rad
    } else {
        -rad
    }
}
