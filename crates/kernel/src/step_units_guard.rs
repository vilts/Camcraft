//! The unit records of a STEP file, made readable and checked before the
//! reader uses them (#2919, #2973). The pinned parser expands only some ISO
//! 10303-21 short names; the unit entities it leaves short are expanded here,
//! so the unit checks and the topology reader read the same long names. A
//! plane angle unit that the reader cannot convert to radians, or an angle
//! unit that does not say that it is one, is refused by name: the reader
//! would read its angles as radians without a word. For the same reason, a
//! solid item with no unit context, or in a context that states no angle
//! unit, is refused when the file states an angle unit that is not the
//! radian (#3079).
//! Provenance: the public ISO short-name registry
//! (https://standards.iso.org/iso/10303/tech/short_names/short-names.txt) and
//! ISO 10303-41 (NAMED_UNIT, SI_UNIT, CONVERSION_BASED_UNIT,
//! PLANE_ANGLE_UNIT, PLANE_ANGLE_MEASURE_WITH_UNIT). Read through the public
//! p21 API of solidcore-step. No HSMWorks material consulted.

use std::collections::{HashMap, HashSet};

use solidcore_step::p21::{Entity, Exchange, Param};
use solidcore_step::topology;
use solidcore_step::units::{self, UnitKind};

/// The unit short names that the pinned parser leaves short, with their long
/// names, as the ISO registry gives them.
const UNIT_SHORT_NAMES: [(&str, &str); 4] = [
    ("GC", "GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT"),
    ("GUAC", "GLOBAL_UNIT_ASSIGNED_CONTEXT"),
    ("PLANUN", "PLANE_ANGLE_UNIT"),
    ("SUNT", "SI_UNIT"),
];

/// Renames each record of [`UNIT_SHORT_NAMES`] to its long name.
pub(crate) fn expand_short_names(ex: &mut Exchange) {
    for entity in ex.entities.values_mut() {
        for record in &mut entity.records {
            if let Some((_, long)) = UNIT_SHORT_NAMES.iter().find(|(short, _)| *short == record.name) {
                record.name = (*long).into();
            }
        }
    }
}

/// `Err` names the first unit of a representation context whose angles the
/// reader cannot read in the unit that the file states.
pub(crate) fn check_angle_units(ex: &Exchange) -> Result<(), String> {
    for e in ex.entities.values() {
        let Some(list) = e
            .record("GLOBAL_UNIT_ASSIGNED_CONTEXT")
            .and_then(|r| r.first())
            .and_then(Param::as_list)
        else {
            continue;
        };
        for u in list.iter().filter_map(Param::as_ref_id) {
            let Some(unit) = ex.get(u) else { continue };
            if unit.has("PLANE_ANGLE_UNIT") {
                if !angle_resolves(ex, u, unit, 0) {
                    return Err(format!("plane angle unit #{u} cannot be converted to radians"));
                }
            } else if !unit.has("LENGTH_UNIT") && measures_angle(ex, unit) {
                return Err(format!(
                    "unit #{u} measures a plane angle but is not marked as a plane angle unit; \
                     its angles cannot be read"
                ));
            }
        }
    }
    Ok(())
}

fn si_name(unit: &Entity) -> Option<&str> {
    unit.record("SI_UNIT").and_then(|si| si.get(1)).and_then(Param::as_enum)
}

/// The unit that a conversion-based unit is a multiple of.
fn conversion_base(ex: &Exchange, unit: &Entity) -> Option<u64> {
    let measure = ex.get(unit.record("CONVERSION_BASED_UNIT")?.get(1)?.as_ref_id()?)?;
    measure.params().get(1)?.as_ref_id()
}

/// A marked plane angle unit is an SI radian, or a conversion of a unit that
/// resolves the same way, with a positive finite size. The chain is followed
/// as deep as `units::unit_factor` follows it.
fn angle_resolves(ex: &Exchange, id: u64, unit: &Entity, depth: usize) -> bool {
    if depth > 4 || !unit.has("PLANE_ANGLE_UNIT") {
        return false;
    }
    if let Some(name) = si_name(unit) {
        return name == "RADIAN";
    }
    if unit.record("CONVERSION_BASED_UNIT").is_none() {
        return false;
    }
    let base_resolves = conversion_base(ex, unit)
        .and_then(|b| ex.get(b).map(|base| angle_resolves(ex, b, base, depth + 1)))
        .unwrap_or(false);
    base_resolves
        && matches!(units::unit_factor(ex, id), Some((UnitKind::Angle, f)) if f.is_finite() && f > 0.0)
}

/// An unmarked unit that is an angle all the same: a prefixed SI radian, or
/// a conversion whose measure is a plane angle or whose base unit is one.
fn measures_angle(ex: &Exchange, unit: &Entity) -> bool {
    if si_name(unit) == Some("RADIAN") {
        let prefixed = unit.record("SI_UNIT").and_then(|si| si.first()).and_then(Param::as_enum).is_some();
        return prefixed;
    }
    let Some(cb) = unit.record("CONVERSION_BASED_UNIT") else { return false };
    let measure = cb.get(1).and_then(Param::as_ref_id).and_then(|m| ex.get(m));
    if measure.is_some_and(|m| m.has("PLANE_ANGLE_MEASURE_WITH_UNIT")) {
        return true;
    }
    conversion_base(ex, unit)
        .and_then(|b| ex.get(b))
        .is_some_and(|b| b.has("PLANE_ANGLE_UNIT") || si_name(b) == Some("RADIAN"))
}

/// The solid items whose angles the reader reads in radians by default,
/// judged by the angle unit (#3079). That is an item with no unit context,
/// or one whose context states no plane angle unit: the reader gives it the
/// default units, its angles in radians. That is right only when the file
/// states no angle unit that is not the radian.
pub(crate) struct UnlistedAngles {
    /// The first stated plane angle unit that is not the radian.
    unit: Option<u64>,
    /// Each item's context, by the reader's own rules: the first
    /// representation by entity id that lists it with a context, and each
    /// piece of a listed solid in that solid's context.
    context_of: HashMap<u64, u64>,
    /// The unit contexts that state a plane angle unit.
    stating: HashSet<u64>,
}

impl UnlistedAngles {
    pub(crate) fn new(ex: &Exchange) -> UnlistedAngles {
        let mut ids: Vec<u64> = ex.entities.keys().copied().collect();
        ids.sort_unstable();
        let mut context_of = HashMap::new();
        let mut stating = HashSet::new();
        let mut unit = None;
        for &id in &ids {
            let Some(e) = ex.get(id) else { continue };
            if let Some((items, Some(ctx))) = representation(e) {
                for item in items {
                    context_of.entry(item).or_insert(ctx);
                }
            }
            if states_angle(ex, e) {
                stating.insert(id);
            }
            if unit.is_none() {
                unit = non_radian_unit(ex, e);
            }
        }
        for &id in &ids {
            if !ex.get(id).is_some_and(|e| topology::is_solid_item(e.name())) {
                continue;
            }
            let Some(ctx) = context_of.get(&id).copied() else { continue };
            for piece in topology::items(ex, id) {
                if piece != id {
                    context_of.insert(piece, ctx);
                }
            }
        }
        UnlistedAngles { unit, context_of, stating }
    }

    /// `Some(reason)`: item `item` has no unit context, or its context
    /// states no plane angle unit, but the file states an angle unit that
    /// is not the radian, so its angles would be wrong.
    pub(crate) fn refusal(&self, item: u64) -> Option<String> {
        let unit = self.unit?;
        match self.context_of.get(&item) {
            Some(c) if self.stating.contains(c) => None,
            Some(c) => Some(format!(
                "its context #{c} states no plane angle unit, so its angles would be read in \
                 radians, but the file states plane angle unit #{unit}, which is not the radian"
            )),
            None => Some(format!(
                "no representation gives it a unit context, so its angles would be read in radians, \
                 but the file states plane angle unit #{unit}, which is not the radian"
            )),
        }
    }
}

/// Whether unit context `e` states a plane angle unit.
fn states_angle(ex: &Exchange, e: &Entity) -> bool {
    e.record("GLOBAL_UNIT_ASSIGNED_CONTEXT")
        .and_then(|r| r.first())
        .and_then(Param::as_list)
        .is_some_and(|list| {
            list.iter()
                .filter_map(Param::as_ref_id)
                .any(|u| ex.get(u).is_some_and(|unit| unit.has("PLANE_ANGLE_UNIT")))
        })
}

/// The first plane angle unit of unit context `e` that is not the radian.
fn non_radian_unit(ex: &Exchange, e: &Entity) -> Option<u64> {
    let list = e.record("GLOBAL_UNIT_ASSIGNED_CONTEXT")?.first()?.as_list()?;
    list.iter().filter_map(Param::as_ref_id).find(|&u| {
        ex.get(u).is_some_and(|unit| unit.has("PLANE_ANGLE_UNIT"))
            && !matches!(units::unit_factor(ex, u), Some((UnitKind::Angle, f)) if (f - 1.0).abs() <= 1e-12)
    })
}

/// Items and context of a representation entity, plain or complex (the
/// `representation` attributes name, items, context_of_items, ISO 10303-43).
fn representation(e: &Entity) -> Option<(Vec<u64>, Option<u64>)> {
    let p = if e.is_complex() {
        e.record("REPRESENTATION").or_else(|| {
            e.records
                .iter()
                .find(|r| r.name.ends_with("REPRESENTATION") && r.params.len() >= 3)
                .map(|r| r.params.as_slice())
        })?
    } else if e.name().ends_with("REPRESENTATION") {
        e.params()
    } else {
        return None;
    };
    let items = p.get(1)?.as_list()?.iter().filter_map(Param::as_ref_id).collect();
    Some((items, p.get(2).and_then(Param::as_ref_id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use solidcore_step::p21;

    const CONE: &str = include_str!("../../../testdata/models/short_units_degree_cone.step");

    fn expanded(text: &str) -> Exchange {
        let mut ex = p21::parse_with(text, None).expect("the fixture must parse");
        expand_short_names(&mut ex);
        ex
    }

    #[test]
    fn short_name_context_reads_degrees_and_uncertainty() {
        let c = units::context(&expanded(CONE), 11);
        assert!((c.units.ang - 1.0_f64.to_radians()).abs() < 1e-15, "degrees expected: {c:?}");
        assert_eq!(c.uncertainty, Some(2.5e-6), "the GC uncertainty must be read: {c:?}");
    }

    #[test]
    fn degree_unit_passes_the_angle_check() {
        assert_eq!(check_angle_units(&expanded(CONE)), Ok(()));
    }

    #[test]
    fn angle_unit_on_a_steradian_is_refused() {
        let text = CONE.replace("PLANUN() SUNT($,.RADIAN.)", "PLANUN() SUNT($,.STERADIAN.)");
        let err = check_angle_units(&expanded(&text)).expect_err("a degree of a steradian must be refused");
        assert!(err.contains("#13"), "{err}");
    }
}
