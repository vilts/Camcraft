//! A generated ("loose") machine model (#2584): one closed block or
//! cylinder for each machine part that states a number to size it, and
//! the pose of each body for a set of axis values.
//!
//! Provenance: our own construction. The structure of a mill (bed,
//! slides, column, head, spindle) is from the machine-tool handbooks;
//! ISO 841 gives the axis nomenclature (Z is parallel to the spindle);
//! the pose is the forward kinematics of a serial chain (J. J. Craig,
//! "Introduction to Robotics"). No vendor machine model and no HSMWorks
//! material consulted.
//!
//! ## The input
//!
//! Plain data of this module ([`MachineSpec`]). This crate has no edge
//! to the machine-file reader: the caller maps its part tree into it.
//! Parts are listed parent first. A part frame is its parent's frame,
//! then the part's `offset`, then its own joint. A joint `axis` is the
//! direction in which THE PART moves (linear) or turns right-handed
//! (rotary) for a positive axis value. A part that moves the table runs
//! against the program axis: its caller states the reversed direction.
//!
//! ## The bodies (all sizes are loose, all come from stated numbers)
//!
//! With each joint at zero, each part frame is the machine frame plus
//! the offsets: the spindle nose is at the origin and the spindle runs
//! up +z.
//! - A head (a part that states `spindle` or `tool_station`): a cylinder
//!   of the spindle diameter from the nose up by the spindle length, and
//!   the TOOL ENVELOPE, a cylinder of the largest tool below the nose.
//!   The envelope is not steel: it is the room the largest tool takes.
//! - A linear part: a block as long as its travel span along its axis,
//!   with a square section of [`SECTION_RATIO`] of the span. A part that
//!   carries the head stands over the bodies it carries; each other part
//!   hangs under the parts it carries, below z = 0.
//! - A fixed part with no head below it, or the root: a table block. Its
//!   X and Y sizes are the linear travel of the machine along X and Y,
//!   its thickness is [`BED_RATIO`] of the larger one, and it hangs
//!   under the slides that it carries.
//! - A part with no number to size it (a rotary part, a linear part with
//!   no travel, a head with no spindle size) gets NO body and a named
//!   note in [`MachineModel::notes`]. No size is guessed.
//!
//! ## The laws
//!
//! - Each body is marked [`BodySource::Generated`]. A check that uses it
//!   reports [`BodySource::judged_on`]: "judged on a generated model".
//! - Degenerate input (non-finite numbers, zero or reversed travel, a
//!   zero axis, a broken parent chain) refuses by the part id and key.
//! - [`MachineModel::pose`] does not judge travel limits. That is the
//!   limit check's work; the pose shows where the stated values put the
//!   bodies.

use crate::geometry::{Point3, Triangle};

mod check;
mod pose;
mod shape;
mod vec;

pub use shape::SIDES;

use check::{Joint, Link};
use vec::{cross, unit, V3};

/// No length past this (mm): a typo, never a real machine.
pub const MAX_DIM: f64 = 100_000.0;
/// No machine has more parts than this.
pub const MAX_PARTS: usize = 256;
/// The side of a slide block's square section, as a part of its span.
pub const SECTION_RATIO: f64 = 0.25;
/// The thickness of a table block, as a part of its larger side.
pub const BED_RATIO: f64 = 0.1;

/// How a part joins its parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JointKind {
    /// No motion: a table, a bed or a head.
    Fixed,
    /// A slide along `axis`, in mm.
    Linear,
    /// A turn about `axis` through `pivot`, in degrees.
    Rotary,
}

/// The spindle body that a head states (mm). `None` is "not stated".
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpindleSpec {
    pub diameter: Option<f64>,
    pub length: Option<f64>,
}

/// The largest tool that a head's tool station states (mm).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ToolStationSpec {
    pub max_tool_length: Option<f64>,
    pub max_tool_diameter: Option<f64>,
}

/// One part of the machine, with the numbers that the caller knows.
#[derive(Clone, Debug, PartialEq)]
pub struct PartSpec {
    pub id: String,
    /// The index of the parent in [`MachineSpec::parts`]; it is lower
    /// than the index of the part. `None` on a root.
    pub parent: Option<usize>,
    pub joint: JointKind,
    /// The direction in which the part moves or turns for a positive
    /// axis value. Required on a linear and a rotary part.
    pub axis: Option<[f64; 3]>,
    /// A point on the line of a rotary axis, in the frame of the parent
    /// after `offset`. Not stated: the origin.
    pub pivot: Option<[f64; 3]>,
    /// Where the part frame is in the frame of its parent (mm).
    pub offset: [f64; 3],
    /// `(min, max)`: mm on a linear part, degrees on a rotary part.
    pub travel: Option<(f64, f64)>,
    /// `Some` on a head, with what the machine file states.
    pub spindle: Option<SpindleSpec>,
    /// `Some` on a head that states a tool station.
    pub tool_station: Option<ToolStationSpec>,
}

impl PartSpec {
    /// A part with no number stated.
    #[must_use]
    pub fn new(id: &str, parent: Option<usize>, joint: JointKind) -> Self {
        Self {
            id: id.to_owned(),
            parent,
            joint,
            axis: None,
            pivot: None,
            offset: [0.0; 3],
            travel: None,
            spindle: None,
            tool_station: None,
        }
    }

    fn is_head(&self) -> bool {
        self.spindle.is_some() || self.tool_station.is_some()
    }
}

/// The input of [`generate`]: the parts, each parent ahead of its children.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MachineSpec {
    pub parts: Vec<PartSpec>,
}

/// What a body stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    /// A table, bed, slide or column.
    Block,
    /// The spindle body.
    Spindle,
    /// The room of the largest tool below the spindle nose. Not steel.
    ToolEnvelope,
}

/// Where the shape of a body comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodySource {
    /// Made here from travel and spindle numbers: not the real steel.
    Generated,
}

impl BodySource {
    /// The words a check adds to each finding and each pass that it
    /// judged on a body of this source.
    #[must_use]
    pub fn judged_on(self) -> &'static str {
        match self {
            Self::Generated => "judged on a generated model",
        }
    }
}

/// One closed body of one part, as outward triangles.
#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    /// The index of the part in [`MachineSpec::parts`].
    pub part: usize,
    pub part_id: String,
    pub kind: BodyKind,
    pub source: BodySource,
    pub triangles: Vec<Triangle>,
}

/// The generated model: the bodies with each joint at zero, the notes,
/// and the checked chain that [`MachineModel::pose`] moves them along.
#[derive(Clone, Debug)]
pub struct MachineModel {
    /// The bodies with each axis value at zero, in part order.
    pub bodies: Vec<Body>,
    /// One line for each body that was not made, with the part id.
    pub notes: Vec<String>,
    links: Vec<Link>,
}

impl MachineModel {
    /// The bodies in the machine frame for `values`: one `(part id, axis
    /// value)` for each linear part (mm) and each rotary part (degrees).
    /// A joint with no value, a value for a fixed or unknown part, a
    /// part stated two times and a non-finite value refuse by name.
    pub fn pose(&self, values: &[(&str, f64)]) -> Result<Vec<Body>, String> {
        let frames = pose::frames(&self.links, values)?;
        let mut out = Vec::with_capacity(self.bodies.len());
        for body in &self.bodies {
            let frame = frames
                .get(body.part)
                .ok_or_else(|| format!("machine model: pose: part {} is not in the chain", body.part_id))?;
            let at = |p: Point3| {
                let [x, y, z] = frame.apply([p.x, p.y, p.z]);
                Point3::new(x, y, z)
            };
            let triangles: Vec<Triangle> =
                body.triangles.iter().map(|t| Triangle::new(at(t.a), at(t.b), at(t.c))).collect();
            if triangles.iter().any(|t| !(t.a.is_finite() && t.b.is_finite() && t.c.is_finite())) {
                return Err(format!("machine model: pose: part {}: the posed body is not finite", body.part_id));
            }
            out.push(Body { triangles, ..body.clone() });
        }
        Ok(out)
    }
}

fn note(id: &str, why: &str) -> String {
    format!("machine model: part {id}: {why}; no body is generated")
}

/// The block of a slide with `span` of travel along the unit `axis`, and
/// its height along z. A part that carries the head stands on `reach`
/// (`up`); each other part hangs under `-reach`.
fn slide(axis: V3, span: f64, up: bool, reach: f64) -> (Vec<Triangle>, f64) {
    // The world axis least along the travel gives the section its square.
    let least = (0..3).fold(0, |best, k| if axis[k].abs() < axis[best].abs() { k } else { best });
    let mut world = [0.0; 3];
    world[least] = 1.0;
    let u = unit(cross(world, axis)).unwrap_or(world);
    let v = cross(axis, u);
    let side = span * SECTION_RATIO;
    let height = span * axis[2].abs() + side * (u[2].abs() + v[2].abs());
    let centre = if up { reach + 0.5 * height } else { -reach - 0.5 * height };
    (shape::block([0.0, 0.0, centre], [axis, u, v], [0.5 * span, 0.5 * side, 0.5 * side]), height)
}

/// The bodies of a head: the spindle cylinder and the tool envelope, with
/// a note for each that the numbers do not size. Gives the spindle length
/// that the parts above stand on (zero with no spindle body).
fn head(p: &PartSpec, made: &mut Vec<(BodyKind, Vec<Triangle>)>, notes: &mut Vec<String>) -> f64 {
    let missing = |pairs: [(&str, Option<f64>); 2]| -> String {
        let names: Vec<String> = pairs.iter().filter(|(_, v)| v.is_none()).map(|(n, _)| format!("no {n}")).collect();
        names.join(" and ")
    };
    let spindle = p.spindle.unwrap_or_default();
    let mut length = 0.0;
    if let (Some(d), Some(len)) = (spindle.diameter, spindle.length) {
        made.push((BodyKind::Spindle, shape::cylinder(0.5 * d, 0.0, len)));
        length = len;
    } else {
        let what = missing([("spindle diameter", spindle.diameter), ("spindle length", spindle.length)]);
        notes.push(format!("machine model: part {}: {what} is stated; no body is generated for the spindle", p.id));
    }
    let station = p.tool_station.unwrap_or_default();
    if let (Some(len), Some(d)) = (station.max_tool_length, station.max_tool_diameter) {
        made.push((BodyKind::ToolEnvelope, shape::cylinder(0.5 * d, -len, 0.0)));
    } else {
        let what = missing([("max tool length", station.max_tool_length), ("max tool diameter", station.max_tool_diameter)]);
        notes.push(format!(
            "machine model: part {}: the tool station states {what}; no body is generated for the tool envelope",
            p.id
        ));
    }
    length
}

/// Make the model of `spec`. Degenerate input refuses by the part id and
/// the key. A part with no number to size it is not an error: it gets no
/// body and a note.
pub fn generate(spec: &MachineSpec) -> Result<MachineModel, String> {
    let links = check::links(spec)?;
    let n = links.len();
    // `check::links` proved that each parent index is lower than its
    // part's, so one pass from the last part up reads each child first.
    let mut carries = vec![false; n];
    // The linear travel of the machine along X and Y: the table's sizes.
    let mut plan = [0.0_f64; 2];
    for (i, (p, link)) in spec.parts.iter().zip(&links).enumerate().rev() {
        carries[i] |= p.is_head();
        if let (true, Some(parent)) = (carries[i], link.parent) {
            carries[parent] = true;
        }
        if let (Joint::Linear { axis }, Some((lo, hi))) = (link.joint, p.travel) {
            plan[0] += (hi - lo) * axis[0].abs();
            plan[1] += (hi - lo) * axis[1].abs();
        }
    }
    // How far the bodies that a part carries reach: over the nose plane
    // for a part that carries the head (`up`), under z = 0 for the others.
    let (mut up, mut down) = (vec![0.0_f64; n], vec![0.0_f64; n]);
    let mut bodies = Vec::new();
    let mut notes: Vec<(usize, String)> = Vec::new();
    for (i, (p, link)) in spec.parts.iter().zip(&links).enumerate().rev() {
        let mut made = Vec::new();
        let mut said = Vec::new();
        let mut height = if p.is_head() { head(p, &mut made, &mut said) } else { 0.0 };
        match (link.joint, p.travel) {
            (Joint::Linear { axis }, Some((lo, hi))) => {
                let reach = if carries[i] { up[i] + height } else { down[i] };
                let (triangles, h) = slide(axis, hi - lo, carries[i], reach);
                made.push((BodyKind::Block, triangles));
                height += h;
            }
            (Joint::Linear { .. }, None) => said.push(note(&p.id, "no travel is stated to size it")),
            (Joint::Rotary { .. } | Joint::Fixed, _) if p.is_head() => {}
            (Joint::Rotary { .. }, _) => said.push(note(&p.id, "a rotary part states an angle and no length")),
            (Joint::Fixed, _) if link.parent.is_none() || !carries[i] => {
                if plan[0] > 0.0 && plan[1] > 0.0 {
                    height = BED_RATIO * plan[0].max(plan[1]);
                    let centre = [0.0, 0.0, -down[i] - 0.5 * height];
                    let dirs = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
                    made.push((BodyKind::Block, shape::block(centre, dirs, [0.5 * plan[0], 0.5 * plan[1], 0.5 * height])));
                } else {
                    said.push(note(&p.id, "no linear travel along both X and Y is stated to size it"));
                }
            }
            (Joint::Fixed, _) => said.push(note(&p.id, "a fixed part between an axis and the head states no size")),
        }
        if let Some(parent) = link.parent {
            let reach = if carries[i] { &mut up } else { &mut down };
            reach[parent] = reach[parent].max(reach[i] + height);
        }
        notes.extend(said.into_iter().map(|s| (i, s)));
        bodies.extend(made.into_iter().map(|(kind, triangles)| Body {
            part: i,
            part_id: p.id.clone(),
            kind,
            source: BodySource::Generated,
            triangles,
        }));
    }
    // The pass ran from the last part up: give the output in part order.
    bodies.sort_by_key(|b| b.part);
    notes.sort_by_key(|(i, _)| *i);
    Ok(MachineModel { bodies, notes: notes.into_iter().map(|(_, s)| s).collect(), links })
}
