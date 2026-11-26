//! The reader of [`crate::step_csys`]: every named `AXIS2_PLACEMENT_3D` of
//! a STEP text, read through solidcore's Part 21 parser (#2789).
//!
//! Provenance: placement semantics per ISO 10303-42 (`AXIS2_PLACEMENT_3D`:
//! location, axis, ref_direction projected normal to the axis); file
//! structure per ISO 10303-21 (an entity id names one instance; the
//! instances are in DATA sections). Nothing from any CAM vendor.

use std::collections::HashMap;

use camcraft_kernel::p21::{self, Exchange, Param};

use crate::step_csys::{dot, CsysRefusal, CsysRole, NamedCsys, ToolCsysReport, Vec3};
use crate::step_diag::{locate, Failure};
use crate::step_units::{mm_per_unit, real};

/// Two same-named placements closer than this (mm, and unit-vector
/// components) are one coordinate system written twice, not two.
const SAME_PLACEMENT: f64 = 1e-9;
/// A direction shorter than this is no direction.
const MIN_LEN: f64 = 1e-12;
/// How many ids defined twice a refusal names.
const NAMED_DUPLICATES: usize = 5;

/// See [`crate::step_csys::read_tool_csys`].
pub(crate) fn read(text: &str) -> Result<ToolCsysReport, String> {
    let ex = exchange(text)?;
    let k = mm_per_unit(&ex)
        .map_err(|e| format!("length unit: {e}"))?
        .unwrap_or(1.0);

    let mut report = ToolCsysReport::default();
    // Name (as written) -> index into `systems`, to fold repeats.
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut ambiguous: HashMap<String, Vec<u64>> = HashMap::new();
    // `entities` is a BTreeMap: id order, as before.
    for (&id, e) in &ex.entities {
        let Some(a) = e.record("AXIS2_PLACEMENT_3D") else {
            continue;
        };
        let name = a
            .first()
            .and_then(Param::as_str)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        // Unnamed placements locate surfaces, not coordinate systems.
        if name.is_empty() || name.eq_ignore_ascii_case("NONE") {
            continue;
        }
        match frame(&ex, a, k) {
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

/// The file's entities, or why the file itself can't be read.
///
/// - A syntax error is named by the locator ([`locate`]: the entity and its
///   line, a string or comment that never closes), else by the parser.
/// - A file with no DATA section is refused ("file has no DATA section");
///   the parser alone reads it as a file with no entities.
/// - An entity id defined twice is refused (#2789): the parser keeps the
///   first definition, the earlier reader kept the last, and a coordinate
///   system read from the wrong one would place the tool body wrong.
/// - Several DATA sections are read as one (ISO 10303-21 edition 3).
fn exchange(text: &str) -> Result<Exchange, String> {
    let ex = match p21::parse(text) {
        Ok(ex) => ex,
        Err(e) => {
            return Err(match locate(text) {
                Some(Failure::NoData) => no_data(),
                Some(found) => found.to_string(),
                None => e.to_string(),
            });
        }
    };
    if ex.entities.is_empty() && locate(text) == Some(Failure::NoData) {
        return Err(no_data());
    }
    if let Some(first) = ex.duplicates.first() {
        let mut ids: Vec<u64> = ex.duplicates.clone();
        ids.sort_unstable();
        ids.dedup();
        let named: Vec<String> = ids
            .iter()
            .take(NAMED_DUPLICATES)
            .map(|i| format!("#{i}"))
            .collect();
        let more = ids.len().saturating_sub(NAMED_DUPLICATES);
        let tail = if more > 0 { format!(" and {more} more") } else { String::new() };
        return Err(format!(
            "entity #{first} is defined more than once (ids defined twice: {}{tail}); \
             the file is malformed, so no coordinate system is read from it",
            named.join(", ")
        ));
    }
    Ok(ex)
}

fn no_data() -> String {
    "file has no DATA section".into()
}

/// Origin (mm), unit z and unit x of one `AXIS2_PLACEMENT_3D`.
fn frame(ex: &Exchange, a: &[Param], k: f64) -> Result<(Vec3, Vec3, Vec3), String> {
    let location = match a.get(1) {
        Some(Param::Ref(id)) => triple(ex, *id, "CARTESIAN_POINT")?,
        _ => return Err("its location is not a point".into()),
    };
    let origin = location.map(|v| v * k);
    if !origin.iter().all(|v| v.is_finite()) {
        return Err("its origin is not finite".into());
    }
    let z = match a.get(2) {
        Some(Param::Ref(id)) => unit(triple(ex, *id, "DIRECTION")?)
            .ok_or("its axis has no direction (zero length or not finite)")?,
        _ => [0.0, 0.0, 1.0], // ISO 10303-42: an absent axis is +Z
    };
    let r = match a.get(3) {
        Some(Param::Ref(id)) => unit(triple(ex, *id, "DIRECTION")?)
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
fn triple(ex: &Exchange, id: u64, kind: &str) -> Result<Vec3, String> {
    let a = ex
        .get(id)
        .and_then(|e| e.record(kind))
        .ok_or_else(|| format!("#{id} is not a {kind}"))?;
    let Some(vals) = a.get(1).and_then(Param::as_list) else {
        return Err(format!("{kind} #{id} has no coordinate list"));
    };
    match vals {
        [x, y, z] => match (real(x), real(y), real(z)) {
            (Some(x), Some(y), Some(z)) => Ok([x, y, z]),
            _ => Err(format!(
                "{kind} #{id} has a coordinate that is not a number"
            )),
        },
        _ => Err(format!("{kind} #{id} is not three-dimensional")),
    }
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
