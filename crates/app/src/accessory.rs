//! Built-in workholding accessories (#789, part of #339): parallels,
//! 1-2-3 blocks, step blocks, strap (toe) clamps, soft jaws and rotary
//! subplates (#821: the base plate a 4th axis stands on), each
//! generated from a few dimensions and placed through the fixture
//! library's insert path (`commands::InsertAccessory`) - a model wearing
//! `FixtureDef`, judged by the same fixture laws as any meshed fixture.
//! Its parameters ride the marker so it can be resized
//! (`commands::SetAccessory`) and saved.
//!
//! Every item's frame has z = 0 at its underside (the table, the vise
//! floor). Parallels: two bars along x, centred, with the gap between
//! their inner faces centred on y = 0. Block, soft jaw: centred in x; the
//! block centred in y, the soft jaw's clamping face at y = 0 with its body
//! toward +y (the vise's fixed-jaw voice). Step block: steps rising toward
//! +y from y = 0, centred in x. Strap clamp: the toe tip at x = 0, the
//! strap running toward +x, centred in y; its underside at the clamp
//! height (the clamped face), the heel block under its far end, the stud
//! between them from the table up through the strap. Rotary subplate
//! (#821): the origin on its first locating pin bore, on the underside,
//! +x toward the second bore (the way `plate::mount` seats a dowel pair).
//! Lathe chucks (#1442: 3-jaw, 4-jaw, collet) break the underside rule:
//! their frame's z = 0 is the face the work stands on, +z the spindle
//! axis out of it, the body below (`camcraft_sim::chuck`).
//!
//! Provenance: our own generic geometry and figures (common inch sizes of
//! shop accessories); no vendor model copied.

use serde::{Deserialize, Serialize};

use crate::geometry::{Aabb, Point3, Triangle, quat_conjugate, quat_rotate_point};
use crate::schema::Location;

pub use camcraft_sim::chuck::{ColletChuck, JawChuck};

pub mod chuck;
pub mod mesh;
pub mod subplate;

const IN: f64 = 25.4;
/// No accessory dimension past this (mm): a typo, never a real part.
pub const MAX_DIM: f64 = 5000.0;
/// The most steps a step block may have.
pub const MAX_STEPS: u32 = 50;

/// One accessory's kind and dimensions (millimetres).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Accessory {
    /// A pair of parallels: bars `length` long (x), `thickness` across
    /// (y), `height` tall, `spacing` between their inner faces.
    Parallels { height: f64, thickness: f64, length: f64, spacing: f64 },
    /// A solid block (a 1-2-3 block by default; its holes drawn solid).
    Block { length: f64, width: f64, height: f64 },
    /// A step block: `steps` steps, each `rise` up and `run` deep, `width`
    /// across.
    StepBlock { steps: u32, rise: f64, run: f64, width: f64 },
    /// A strap (toe) clamp: strap, stud, heel block; `clamp_height` is the
    /// clamped face's height over the table (the strap's underside).
    StrapClamp {
        strap_length: f64,
        strap_width: f64,
        strap_thickness: f64,
        stud_diameter: f64,
        /// The stud (nut and washer included) above the strap's top.
        stud_height: f64,
        /// From the toe tip to the stud's centre.
        stud_offset: f64,
        heel_length: f64,
        clamp_height: f64,
    },
    /// A soft jaw: a block that stands in for a vise jaw.
    SoftJaw { width: f64, height: f64, thickness: f64 },
    /// A rotary subplate (#821): the base plate a 4th axis is bolted to,
    /// located on a fixture plate's dowel pair by two pin bores
    /// `bore_spacing` apart. `bore_x` is the first bore's distance from
    /// the -x end, `bore_y` the bores' distance from the -y edge. It does
    /// not turn: it raises the rotary's centerline by its `thickness`.
    RotarySubplate {
        length: f64,
        width: f64,
        thickness: f64,
        bore_spacing: f64,
        bore_x: f64,
        bore_y: f64,
        bore_diameter: f64,
    },
    /// A self-centring 3-jaw lathe chuck (#1442).
    ThreeJawChuck(JawChuck),
    /// A 4-jaw lathe chuck (#1442), its jaws closed evenly.
    FourJawChuck(JawChuck),
    /// A collet chuck (#1442).
    ColletChuck(ColletChuck),
}

/// The kinds, in the order the dialog lists them.
pub const KINDS: [&str; 9] = [
    "parallels",
    "block",
    "step_block",
    "strap_clamp",
    "soft_jaw",
    "rotary_subplate",
    "three_jaw_chuck",
    "four_jaw_chuck",
    "collet_chuck",
];

/// One named dimension: the file/MCP key, the label the UI shows, and
/// the value.
pub struct Dim<'a> {
    pub key: &'static str,
    pub label: &'static str,
    pub value: &'a mut f64,
}

impl Accessory {
    /// The default accessory of kind `kind` (a `KINDS` key).
    #[must_use]
    pub fn default_of(kind: &str) -> Option<Self> {
        Some(match kind {
            // 1/8 x 1 x 6 in parallels, 4 in apart.
            "parallels" => Self::Parallels { height: IN, thickness: 0.125 * IN, length: 6.0 * IN, spacing: 4.0 * IN },
            "block" => Self::Block { length: 3.0 * IN, width: 2.0 * IN, height: IN },
            "step_block" => Self::StepBlock { steps: 6, rise: 0.5 * IN, run: 0.75 * IN, width: IN },
            // A 1/2 in stud's strap.
            "strap_clamp" => Self::StrapClamp {
                strap_length: 6.0 * IN,
                strap_width: 1.25 * IN,
                strap_thickness: 0.625 * IN,
                stud_diameter: 0.5 * IN,
                stud_height: 1.5 * IN,
                stud_offset: 2.25 * IN,
                heel_length: 1.25 * IN,
                clamp_height: IN,
            },
            "soft_jaw" => Self::SoftJaw { width: 6.0 * IN, height: 2.0 * IN, thickness: 0.75 * IN },
            "rotary_subplate" => subplate::generic(),
            "three_jaw_chuck" | "four_jaw_chuck" | "collet_chuck" => return chuck::generic(kind),
            _ => return None,
        })
    }

    /// This accessory's `KINDS` key.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Parallels { .. } => "parallels",
            Self::Block { .. } => "block",
            Self::StepBlock { .. } => "step_block",
            Self::StrapClamp { .. } => "strap_clamp",
            Self::SoftJaw { .. } => "soft_jaw",
            Self::RotarySubplate { .. } => "rotary_subplate",
            Self::ThreeJawChuck(_) => "three_jaw_chuck",
            Self::FourJawChuck(_) => "four_jaw_chuck",
            Self::ColletChuck(_) => "collet_chuck",
        }
    }

    /// The kind as the UI names it, and the model name a fresh one lands
    /// under (before the free-name suffix).
    #[must_use]
    pub fn label(&self) -> &'static str {
        label_of(self.kind())
    }

    /// Every length dimension, named (the step count is apart).
    pub fn dims_mut(&mut self) -> Vec<Dim<'_>> {
        let d = |key, label, value| Dim { key, label, value };
        match self {
            Self::Parallels { height, thickness, length, spacing } => vec![
                d("height", "Height", height),
                d("thickness", "Thickness", thickness),
                d("length", "Length", length),
                d("spacing", "Spacing (gap between them)", spacing),
            ],
            Self::Block { length, width, height } => {
                vec![d("length", "Length", length), d("width", "Width", width), d("height", "Height", height)]
            }
            Self::StepBlock { rise, run, width, .. } => {
                vec![d("rise", "Step rise", rise), d("run", "Step run", run), d("width", "Width", width)]
            }
            Self::StrapClamp {
                strap_length,
                strap_width,
                strap_thickness,
                stud_diameter,
                stud_height,
                stud_offset,
                heel_length,
                clamp_height,
            } => vec![
                d("strap_length", "Strap length", strap_length),
                d("strap_width", "Strap width", strap_width),
                d("strap_thickness", "Strap thickness", strap_thickness),
                d("stud_diameter", "Stud diameter", stud_diameter),
                d("stud_height", "Stud above the strap", stud_height),
                d("stud_offset", "Stud from the toe", stud_offset),
                d("heel_length", "Heel block length", heel_length),
                d("clamp_height", "Clamp height", clamp_height),
            ],
            Self::SoftJaw { width, height, thickness } => {
                vec![d("width", "Width", width), d("height", "Height", height), d("thickness", "Thickness", thickness)]
            }
            Self::RotarySubplate { length, width, thickness, bore_spacing, bore_x, bore_y, bore_diameter } => vec![
                d("length", "Length", length),
                d("width", "Width", width),
                d("thickness", "Thickness (raises the rotary)", thickness),
                d("bore_spacing", "Pin bore spacing", bore_spacing),
                d("bore_x", "First bore from the -X end", bore_x),
                d("bore_y", "Bores from the -Y edge", bore_y),
                d("bore_diameter", "Pin bore diameter", bore_diameter),
            ],
            Self::ThreeJawChuck(c) | Self::FourJawChuck(c) => c.dims_mut().into_iter().map(|(k, l, v)| d(k, l, v)).collect(),
            Self::ColletChuck(c) => c.dims_mut().into_iter().map(|(k, l, v)| d(k, l, v)).collect(),
        }
    }

    /// Every length dimension as (key, label, value).
    #[must_use]
    pub fn dims(&self) -> Vec<(&'static str, &'static str, f64)> {
        let mut copy = self.clone();
        copy.dims_mut().into_iter().map(|d| (d.key, d.label, *d.value)).collect()
    }

    /// The step count, for a step block.
    pub fn steps_mut(&mut self) -> Option<&mut u32> {
        match self {
            Self::StepBlock { steps, .. } => Some(steps),
            _ => None,
        }
    }

    /// The accessory law: every dimension a finite length over zero and
    /// at most `MAX_DIM`; a step block 1..=`MAX_STEPS` steps; a strap
    /// clamp's stud narrower than the strap, between the toe and the heel block.
    /// Refusals name the accessory and the dimension.
    pub fn check(&self) -> Result<(), String> {
        let what = self.label();
        for (_, label, v) in self.dims() {
            let label = label.to_lowercase();
            if !v.is_finite() {
                return Err(format!("{what} {label} must be a finite length (got {v})"));
            }
            if v <= 0.0 {
                return Err(format!("{what} {label} must be more than zero (got {v} mm)"));
            }
            if v > MAX_DIM {
                return Err(format!("{what} {label} {v:.3} mm is over {MAX_DIM} mm"));
            }
        }
        match self {
            Self::StepBlock { steps, .. } if !(1..=MAX_STEPS).contains(steps) => {
                Err(format!("{what} step count must be 1 to {MAX_STEPS} (got {steps})"))
            }
            Self::StrapClamp { strap_length, strap_width, stud_diameter, stud_offset, heel_length, .. } => {
                let r = mesh::stud_radius(*stud_diameter);
                // #812: the stud passes through the strap's slot, and the
                // strap's band is drawn only as wide as the strap - a
                // wider stud would be modelled narrower than typed.
                if 2.0 * r >= *strap_width {
                    return Err(format!(
                        "{what} stud diameter {stud_diameter:.3} mm (enclosing {:.3} mm) must be less than the strap width {strap_width:.3} mm (the stud passes through the strap)",
                        2.0 * r
                    ));
                }
                if *heel_length >= *strap_length {
                    return Err(format!(
                        "{what} heel block length {heel_length:.3} mm must be less than the strap length {strap_length:.3} mm"
                    ));
                }
                if *stud_offset - r <= 0.0 {
                    return Err(format!("{what} stud from the toe {stud_offset:.3} mm puts the stud past the toe"));
                }
                if *stud_offset + r >= *strap_length - *heel_length {
                    return Err(format!("{what} stud from the toe {stud_offset:.3} mm puts the stud into the heel block"));
                }
                Ok(())
            }
            Self::RotarySubplate { .. } => subplate::check(self),
            Self::ThreeJawChuck(c) => c.check(what, 3),
            Self::FourJawChuck(c) => c.check(what, 4),
            Self::ColletChuck(c) => c.check(what),
            _ => Ok(()),
        }
    }

    /// The checked accessory's closed shells in its own frame.
    pub fn mesh(&self) -> Result<Vec<Triangle>, String> {
        self.check()?;
        mesh::solid(self)
    }

    /// The same accessory with key `key`'s dimension set to `v`; refused
    /// by name for a key this kind does not have.
    pub fn with_dim(mut self, key: &str, v: f64) -> Result<Self, String> {
        let kind = self.label();
        let keys: Vec<&str> = self.dims().iter().map(|d| d.0).collect();
        match self.dims_mut().into_iter().find(|d| d.key == key) {
            Some(d) => *d.value = v,
            None => return Err(format!("a {kind} has no dimension \"{key}\" (one of {})", keys.join(", "))),
        }
        Ok(self)
    }
}

/// The UI name of kind `kind`.
#[must_use]
pub fn label_of(kind: &str) -> &'static str {
    match kind {
        "parallels" => "Parallels",
        "block" => "1-2-3 block",
        "step_block" => "Step block",
        "strap_clamp" => "Strap clamp",
        "soft_jaw" => "Soft jaw",
        "rotary_subplate" => "Rotary subplate",
        "three_jaw_chuck" => "3-jaw chuck",
        "four_jaw_chuck" => "4-jaw chuck",
        "collet_chuck" => "Collet chuck",
        _ => "Accessory",
    }
}

/// Parallels fitted under the stock inside a built-in vise: the height,
/// the spacing, the vise's new pose and the parallels' pose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParallelsFit {
    pub height: f64,
    pub spacing: f64,
    pub vise: Location,
    pub parallels: Location,
}

/// Fit parallels `thickness` thick under stock box `stock` inside the
/// vise standing at `pose`, `jaw_height` tall and `opening` open, so the
/// stock's top stands `above` over the jaws: the parallels' height is the
/// jaw height plus `above` less the stock's height (all along the vise's
/// own z), the vise drops so its floor is that far under the stock, and
/// the pair stands on the floor against both jaws. Refused by name when
/// the stock already stands that high, or two parallels do not fit in
/// the opening.
pub fn fit_parallels(
    pose: &Location,
    jaw_height: f64,
    opening: f64,
    stock: &Aabb,
    thickness: f64,
    above: f64,
) -> Result<ParallelsFit, String> {
    if !above.is_finite() || above < 0.0 {
        return Err(format!("the stock's height over the jaws must be zero or more (got {above} mm)"));
    }
    let inv = quat_conjugate(pose.rot);
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for k in 0..8 {
        let c = Point3::new(
            if k & 1 == 0 { stock.min.x } else { stock.max.x },
            if k & 2 == 0 { stock.min.y } else { stock.max.y },
            if k & 4 == 0 { stock.min.z } else { stock.max.z },
        );
        let local = quat_rotate_point(inv, c - pose.pos);
        lo = lo.min(local.z);
        hi = hi.max(local.z);
    }
    if !(lo.is_finite() && hi.is_finite()) {
        return Err("the stock box is not finite".into());
    }
    let tall = hi - lo;
    let height = jaw_height + above - tall;
    if height <= 0.0 {
        return Err(format!(
            "the stock is {tall:.3} mm tall: on the vise floor it already stands {:.3} mm over the jaws - no parallels fit under it",
            tall - jaw_height
        ));
    }
    let spacing = opening - 2.0 * thickness;
    if spacing <= 0.0 {
        return Err(format!(
            "two {thickness:.3} mm parallels do not fit in the vise's {opening:.3} mm jaw opening"
        ));
    }
    let drop = quat_rotate_point(pose.rot, Point3::new(0.0, 0.0, lo - height));
    let vise = Location { pos: Point3::new(pose.pos.x + drop.x, pose.pos.y + drop.y, pose.pos.z + drop.z), rot: pose.rot };
    let mid = quat_rotate_point(pose.rot, Point3::new(0.0, -0.5 * opening, 0.0));
    let parallels = Location { pos: Point3::new(vise.pos.x + mid.x, vise.pos.y + mid.y, vise.pos.z + mid.z), rot: pose.rot };
    Ok(ParallelsFit { height, spacing, vise, parallels })
}
