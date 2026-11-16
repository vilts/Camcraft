//! Named tool coordinate systems of a vendor STEP file (#370): every
//! named `AXIS2_PLACEMENT_3D` as plain data, classified by the ISO 13399
//! 3D-model names when it carries one.
//!
//! Provenance: placement semantics per ISO 10303-42 (`AXIS2_PLACEMENT_3D`:
//! location, axis, ref_direction projected normal to the axis); names per
//! the public ISO/TS 13399 preview pages - PCS ("primary coordinate
//! system"), MCS ("mounting coordinate system"), CIP ("coordinate system
//! in process", at the cutting part), CSIF ("coordinate system
//! interface"), CSW / CSWx / CSWx_y ("coordinate system workpiece side"):
//! <https://cdn.standards.iteh.ai/samples/60275/61617f41de0a4c28a09b1e023984d459/ISO-TS-13399-303-2016.pdf>,
//! <https://cdn.standards.iteh.ai/samples/60433/19d70fbbd8e047b9acbf2893472de6a3/ISO-TS-13399-401-2016.pdf>,
//! <https://cdn.standards.iteh.ai/samples/63601/cd0f5fbf46584959a48af592f6551940/ISO-TS-13399-405-2016.pdf>.
//! Nothing from any CAM vendor.
//!
//! This reads only; placing a body into a tool frame is the tool-body
//! model's job (#368). Coordinates are in the frame of the representation
//! that holds them, scaled to millimetres by the file's length unit.

use std::collections::HashMap;

use ruststep::ast::{Name, Parameter, Record};

use super::step_units::{args, by_id, mm_per_unit, real};
use super::ImportError;

/// Two same-named placements closer than this (mm, and unit-vector
/// components) are one coordinate system written twice, not two.
const SAME_PLACEMENT: f64 = 1e-9;
/// A direction shorter than this is no direction.
const MIN_LEN: f64 = 1e-12;

/// What an ISO 13399 name says a coordinate system is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CsysRole {
    /// "PCS": the primary (reference) coordinate system of the item.
    Primary,
    /// "MCS": where the item mounts onto an adaptive item.
    Mounting,
    /// "CIP": the coordinate system at the cutting part.
    InProcess,
    /// "CSIF": an interface coordinate system of a component.
    Interface,
    /// "CSW" (both `None`), "CSWx" (level) or "CSWx_y" (level, number):
    /// where the next item mounts on the workpiece side.
    WorkpieceSide {
        level: Option<u32>,
        number: Option<u32>,
    },
    /// Any other name: kept as data, never used for alignment.
    Other,
}

impl CsysRole {
    /// The role an ISO 13399 name states. Case and surrounding blanks
    /// are ignored; anything else (`PCS_1`, `CSW_A`) is `Other`.
    pub fn classify(name: &str) -> CsysRole {
        let n = name.trim().to_ascii_uppercase();
        match n.as_str() {
            "PCS" => return CsysRole::Primary,
            "MCS" => return CsysRole::Mounting,
            "CIP" => return CsysRole::InProcess,
            "CSIF" => return CsysRole::Interface,
            "CSW" => {
                return CsysRole::WorkpieceSide {
                    level: None,
                    number: None,
                }
            }
            _ => {}
        }
        let Some(rest) = n.strip_prefix("CSW") else {
            return CsysRole::Other;
        };
        let digits = |s: &str| -> Option<u32> {
            if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            s.parse().ok()
        };
        match rest.split_once('_') {
            None => match digits(rest) {
                Some(level) => CsysRole::WorkpieceSide {
                    level: Some(level),
                    number: None,
                },
                None => CsysRole::Other,
            },
            Some((l, k)) => match (digits(l), digits(k)) {
                (Some(level), Some(number)) => CsysRole::WorkpieceSide {
                    level: Some(level),
                    number: Some(number),
                },
                _ => CsysRole::Other,
            },
        }
    }
}

/// One named coordinate system: a right-handed orthonormal frame.
#[derive(Clone, Debug, PartialEq)]
pub struct NamedCsys {
    /// The name as the file writes it.
    pub name: String,
    /// The `AXIS2_PLACEMENT_3D` entity id (first one, if written twice).
    pub entity: u64,
    pub role: CsysRole,
    /// Origin, millimetres.
    pub origin: [f64; 3],
    /// Unit z axis (the placement's `axis`).
    pub z: [f64; 3],
    /// Unit x axis (`ref_direction` projected normal to z).
    pub x: [f64; 3],
}

impl NamedCsys {
    /// Unit y axis, z x x.
    pub fn y(&self) -> [f64; 3] {
        cross(self.z, self.x)
    }
}

/// A named placement this reader would not use, and why.
#[derive(Clone, Debug, PartialEq)]
pub struct CsysRefusal {
    pub name: String,
    pub entity: u64,
    pub reason: String,
}

/// Every named coordinate system of a file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolCsysReport {
    /// Named, well-formed and unambiguous, in entity order.
    pub systems: Vec<NamedCsys>,
    /// Named but degenerate, non-finite or ambiguous.
    pub refused: Vec<CsysRefusal>,
}

impl ToolCsysReport {
    /// The one coordinate system with `role`, or why there isn't one.
    pub fn find(&self, role: CsysRole) -> Result<&NamedCsys, String> {
        let hits: Vec<&NamedCsys> = self.systems.iter().filter(|c| c.role == role).collect();
        match hits.as_slice() {
            [one] => Ok(one),
            [] => {
                let refused: Vec<&str> = self
                    .refused
                    .iter()
                    .filter(|r| CsysRole::classify(&r.name) == role)
                    .map(|r| r.reason.as_str())
                    .collect();
                if refused.is_empty() {
                    Err(format!("the file has no {role:?} coordinate system"))
                } else {
                    Err(format!("{role:?} refused: {}", refused.join("; ")))
                }
            }
            many => Err(format!(
                "{} coordinate systems claim {role:?} ({})",
                many.len(),
                many.iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }

    /// `None` when at least one ISO 13399-named coordinate system was
    /// read; otherwise the reason a caller falls back to numeric
    /// placement.
    pub fn none_found(&self) -> Option<String> {
        if self.systems.iter().any(|c| c.role != CsysRole::Other) {
            return None;
        }
        let refused_iso: Vec<String> = self
            .refused
            .iter()
            .filter(|r| CsysRole::classify(&r.name) != CsysRole::Other)
            .map(|r| format!("{} (#{}): {}", r.name, r.entity, r.reason))
            .collect();
        if !refused_iso.is_empty() {
            return Some(format!(
                "every ISO 13399 coordinate system was refused: {}",
                refused_iso.join("; ")
            ));
        }
        if self.systems.is_empty() && self.refused.is_empty() {
            return Some("the file names no coordinate systems".into());
        }
        let names: Vec<&str> = self
            .systems
            .iter()
            .map(|c| c.name.as_str())
            .chain(self.refused.iter().map(|r| r.name.as_str()))
            .collect();
        Some(format!(
            "no named coordinate system is an ISO 13399 one (PCS, MCS, CIP, CSIF, CSW): found {}",
            names.join(", ")
        ))
    }

    /// Attempt automatic tool body alignment from the file's ISO 13399
    /// coordinate systems (#370): returns the unit tool axis (pointing from
    /// the cutting point toward the spindle) and the model origin's height
    /// above the tip.
    ///
    /// Requires a valid, unambiguous CIP (coordinate system in process, at
    /// the cutting part - ISO/TS 13399-303 §3.3); its origin is the cutting
    /// point and its unit z is the tool axis. When CIP is absent, ambiguous,
    /// or degenerate, returns `Err` with the named reason to fall back to
    /// numeric placement.
    pub fn align_tool_body(&self) -> Result<([f64; 3], f64), String> {
        let cip = self.find(CsysRole::InProcess).map_err(|e| self.none_found().unwrap_or(e))?;
        let axis = cip.z;
        let offset = -dot(cip.origin, axis);
        if !offset.is_finite() {
            return Err("calculated tip offset is not finite".into());
        }
        Ok((axis, offset))
    }
}

/// Read every named coordinate system of a STEP file. `Err` only when
/// the file itself can't be read (syntax, no DATA section, a length unit
/// that can't be converted); a bad placement is a refusal in the report.
pub fn read_tool_csys(data: &[u8]) -> Result<ToolCsysReport, ImportError> {
    super::step_depth::check(data)?; // #1075: refuse before ruststep can overflow
    if data.is_empty() {
        return Err(ImportError::Empty);
    }
    let text = std::str::from_utf8(data)
        .map_err(|_| ImportError::Step("file is not valid UTF-8".into()))?;
    let exchange = ruststep::parser::parse(text)
        .map_err(|e| ImportError::Step(format!("syntax error: {e}")))?;
    let Some(section) = exchange.data.first() else {
        return Err(ImportError::Step("file has no DATA section".into()));
    };
    let k = mm_per_unit(section)
        .map_err(|e| ImportError::Step(format!("length unit: {e}")))?
        .unwrap_or(1.0);
    let records = by_id(section);
    let mut ids: Vec<u64> = records.keys().copied().collect();
    ids.sort_unstable();

    let mut report = ToolCsysReport::default();
    // Name (as written) -> index into `systems`, to fold repeats.
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut ambiguous: HashMap<String, Vec<u64>> = HashMap::new();
    for id in ids {
        let Some(rec) = records
            .get(&id)
            .and_then(|rs| rs.iter().find(|r| r.name == "AXIS2_PLACEMENT_3D"))
        else {
            continue;
        };
        let name = match args(rec).first() {
            Some(Parameter::String(s)) => s.trim().to_string(),
            _ => String::new(),
        };
        // Unnamed placements locate surfaces, not coordinate systems.
        if name.is_empty() || name.eq_ignore_ascii_case("NONE") {
            continue;
        }
        match frame(&records, rec, k) {
            Err(reason) => report.refused.push(CsysRefusal {
                name,
                entity: id,
                reason,
            }),
            Ok((origin, z, x)) => {
                let c = NamedCsys {
                    role: CsysRole::classify(&name),
                    name: name.clone(),
                    entity: id,
                    origin,
                    z,
                    x,
                };
                match seen.get(&name).and_then(|&i| report.systems.get(i)) {
                    Some(prev) if same(prev, &c) => {}
                    Some(prev) => ambiguous
                        .entry(name)
                        .or_insert_with(|| vec![prev.entity])
                        .push(id),
                    None => {
                        seen.insert(name, report.systems.len());
                        report.systems.push(c);
                    }
                }
            }
        }
    }
    // A name written at two different placements says nothing reliable.
    report.systems.retain(|c| !ambiguous.contains_key(&c.name));
    let mut amb: Vec<(String, Vec<u64>)> = ambiguous.into_iter().collect();
    amb.sort();
    for (name, ents) in amb {
        let list: Vec<String> = ents.iter().map(|e| format!("#{e}")).collect();
        report.refused.push(CsysRefusal {
            reason: format!(
                "'{name}' is written at {} different placements ({})",
                ents.len(),
                list.join(", ")
            ),
            entity: ents.first().copied().unwrap_or_default(),
            name,
        });
    }
    Ok(report)
}

type Vec3 = [f64; 3];

/// Origin (mm), unit z and unit x of one `AXIS2_PLACEMENT_3D`.
fn frame(
    records: &HashMap<u64, Vec<&Record>>,
    rec: &Record,
    k: f64,
) -> Result<(Vec3, Vec3, Vec3), String> {
    let a = args(rec);
    let location = match a.get(1) {
        Some(Parameter::Ref(Name::Entity(id))) => triple(records, *id, "CARTESIAN_POINT")?,
        _ => return Err("its location is not a point".into()),
    };
    let origin = location.map(|v| v * k);
    if !origin.iter().all(|v| v.is_finite()) {
        return Err("its origin is not finite".into());
    }
    let z = match a.get(2) {
        Some(Parameter::Ref(Name::Entity(id))) => unit(triple(records, *id, "DIRECTION")?)
            .ok_or("its axis has no direction (zero length or not finite)")?,
        _ => [0.0, 0.0, 1.0], // ISO 10303-42: an absent axis is +Z
    };
    let r = match a.get(3) {
        Some(Parameter::Ref(Name::Entity(id))) => unit(triple(records, *id, "DIRECTION")?)
            .ok_or("its ref direction has no direction (zero length or not finite)")?,
        _ => [1.0, 0.0, 0.0],
    };
    // The x axis is ref_direction's part normal to z.
    let d = dot(r, z);
    let x = unit([r[0] - d * z[0], r[1] - d * z[1], r[2] - d * z[2]])
        .filter(|_| d.abs() < 1.0 - 1e-9)
        .ok_or("its ref direction is parallel to its axis")?;
    Ok((origin, z, x))
}

/// The three coordinates of a `CARTESIAN_POINT` / `DIRECTION`.
fn triple(records: &HashMap<u64, Vec<&Record>>, id: u64, kind: &str) -> Result<Vec3, String> {
    let rec = records
        .get(&id)
        .and_then(|rs| rs.iter().find(|r| r.name == kind))
        .ok_or_else(|| format!("#{id} is not a {kind}"))?;
    let Some(Parameter::List(vals)) = args(rec).get(1) else {
        return Err(format!("{kind} #{id} has no coordinate list"));
    };
    match vals.as_slice() {
        [x, y, z] => match (real(x), real(y), real(z)) {
            (Some(x), Some(y), Some(z)) => Ok([x, y, z]),
            _ => Err(format!(
                "{kind} #{id} has a coordinate that is not a number"
            )),
        },
        _ => Err(format!("{kind} #{id} is not three-dimensional")),
    }
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// `v` scaled to unit length, or `None` if it has none.
fn unit(v: Vec3) -> Option<Vec3> {
    let len = dot(v, v).sqrt();
    (len.is_finite() && len > MIN_LEN && v.iter().all(|c| c.is_finite()))
        .then(|| v.map(|c| c / len))
}

fn same(a: &NamedCsys, b: &NamedCsys) -> bool {
    let close = |p: Vec3, q: Vec3| {
        p.iter()
            .zip(q)
            .all(|(u, v)| (u - v).abs() <= SAME_PLACEMENT)
    };
    close(a.origin, b.origin) && close(a.z, b.z) && close(a.x, b.x)
}
