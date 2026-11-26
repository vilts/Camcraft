//! The input check of the generated machine model (#2584): each part
//! list is proved to be a tree with finite, real numbers before a body is
//! made. A refusal names the part id and the key.
//!
//! Provenance: our own validation law (AGENTS.md "Prime Directive":
//! validate early, refuse by name).

use super::vec::{unit, V3};
use super::{JointKind, MachineSpec, PartSpec, MAX_DIM, MAX_PARTS};

/// The joint of a checked part: the axis is a unit vector.
#[derive(Clone, Copy, Debug)]
pub(super) enum Joint {
    Fixed,
    Linear { axis: V3 },
    Rotary { axis: V3, pivot: V3 },
}

/// A checked part: what the pose needs.
#[derive(Clone, Debug)]
pub(super) struct Link {
    pub id: String,
    pub parent: Option<usize>,
    pub offset: V3,
    pub joint: Joint,
}

/// A stated size: finite, more than zero, at most [`MAX_DIM`].
fn length(id: &str, key: &str, value: Option<f64>) -> Result<(), String> {
    let Some(v) = value else { return Ok(()) };
    if !v.is_finite() {
        return Err(format!("machine model: part {id}: {key} must be a finite length (got {v})"));
    }
    if v <= 0.0 {
        return Err(format!("machine model: part {id}: {key} must be more than zero (got {v} mm)"));
    }
    if v > MAX_DIM {
        return Err(format!("machine model: part {id}: {key} {v} mm is over {MAX_DIM} mm"));
    }
    Ok(())
}

/// A stated position: three finite numbers, each at most [`MAX_DIM`].
fn position(id: &str, key: &str, v: V3) -> Result<V3, String> {
    if v.iter().any(|x| !x.is_finite()) {
        return Err(format!("machine model: part {id}: {key} must be three finite numbers (got {v:?})"));
    }
    if v.iter().any(|x| x.abs() > MAX_DIM) {
        return Err(format!("machine model: part {id}: {key} {v:?} is over {MAX_DIM} mm"));
    }
    Ok(v)
}

/// The unit direction of a linear or rotary joint.
fn direction(id: &str, axis: Option<V3>) -> Result<V3, String> {
    let Some(v) = axis else {
        return Err(format!("machine model: part {id}: a linear or rotary part needs an axis, and the input states none"));
    };
    if v.iter().any(|x| !x.is_finite()) {
        return Err(format!("machine model: part {id}: axis must be three finite numbers (got {v:?})"));
    }
    unit(v).ok_or_else(|| format!("machine model: part {id}: axis states the zero direction"))
}

/// `travel`: finite, `max` more than `min`; a linear span at most
/// [`MAX_DIM`]. A fixed part states none.
fn travel(p: &PartSpec) -> Result<(), String> {
    let id = &p.id;
    let Some((lo, hi)) = p.travel else { return Ok(()) };
    if p.joint == JointKind::Fixed {
        return Err(format!("machine model: part {id}: a fixed part states travel ({lo}..{hi})"));
    }
    if !lo.is_finite() || !hi.is_finite() {
        return Err(format!("machine model: part {id}: travel must be two finite numbers (got {lo}..{hi})"));
    }
    if hi <= lo {
        return Err(format!("machine model: part {id}: travel must have max more than min (got {lo}..{hi})"));
    }
    if p.joint == JointKind::Linear && (hi - lo > MAX_DIM || lo.abs() > MAX_DIM || hi.abs() > MAX_DIM) {
        return Err(format!("machine model: part {id}: travel {lo}..{hi} mm is over {MAX_DIM} mm"));
    }
    Ok(())
}

/// Check `spec` and give one [`Link`] for each part, in the same order.
pub(super) fn links(spec: &MachineSpec) -> Result<Vec<Link>, String> {
    if spec.parts.is_empty() {
        return Err("machine model: the input states no part".to_owned());
    }
    if spec.parts.len() > MAX_PARTS {
        return Err(format!("machine model: {} parts is over {MAX_PARTS} parts", spec.parts.len()));
    }
    let mut out: Vec<Link> = Vec::with_capacity(spec.parts.len());
    for (i, p) in spec.parts.iter().enumerate() {
        let id = &p.id;
        if id.trim().is_empty() {
            return Err(format!("machine model: the part at index {i}: id must be a word that is not empty"));
        }
        if out.iter().any(|seen| &seen.id == id) {
            return Err(format!("machine model: part {id}: id is stated by two parts"));
        }
        if let Some(parent) = p.parent.filter(|parent| *parent >= i) {
            return Err(format!(
                "machine model: part {id}: parent must be a part ahead of it in the list (got index {parent} at index {i})"
            ));
        }
        travel(p)?;
        let joint = match p.joint {
            JointKind::Fixed => Joint::Fixed,
            JointKind::Linear => Joint::Linear { axis: direction(id, p.axis)? },
            JointKind::Rotary => Joint::Rotary {
                axis: direction(id, p.axis)?,
                pivot: position(id, "pivot", p.pivot.unwrap_or([0.0; 3]))?,
            },
        };
        if let Some(s) = p.spindle {
            length(id, "spindle diameter", s.diameter)?;
            length(id, "spindle length", s.length)?;
        }
        if let Some(s) = p.tool_station {
            length(id, "max tool length", s.max_tool_length)?;
            length(id, "max tool diameter", s.max_tool_diameter)?;
        }
        out.push(Link { id: id.clone(), parent: p.parent, offset: position(id, "offset", p.offset)?, joint });
    }
    Ok(out)
}
