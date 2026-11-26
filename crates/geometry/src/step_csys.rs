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
//!
//! #2789: moved from the app with its reader (`step_csys_read`), which
//! reads the file through solidcore's Part 21 parser (it was ruststep's).
//! The app keeps the nesting scan and the error type in front of it.

/// Read every named coordinate system of a STEP text. `Err` only when the
/// file itself can't be read (syntax, no DATA section, an entity id
/// defined twice, a length unit that can't be converted), with the reason;
/// a bad placement is a refusal in the report. The caller runs its own
/// nesting scan first (the app's `step_depth`).
#[cfg(not(target_arch = "wasm32"))]
pub fn read_tool_csys(text: &str) -> Result<ToolCsysReport, String> {
    crate::step_csys_read::read(text)
}

/// The browser build keeps solidcore out of its wasm: a STEP file is
/// refused by name there.
#[cfg(target_arch = "wasm32")]
pub fn read_tool_csys(_text: &str) -> Result<ToolCsysReport, String> {
    Err("STEP files open in the desktop app only, not in the browser build".into())
}

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

pub(crate) type Vec3 = [f64; 3];

pub(crate) fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
