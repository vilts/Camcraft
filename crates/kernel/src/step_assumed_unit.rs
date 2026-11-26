//! A length unit that the caller states for a STEP file that states none
//! (#1874). The unit is written into the file's representation contexts
//! before the reader reads them, so that each later step (the topology
//! read, the stated tolerance, the edge-gap cap) reads the model in that
//! unit, with no scale applied afterwards.
//! Provenance: the unit entities of ISO 10303-41 (`LENGTH_UNIT`,
//! `NAMED_UNIT`, `SI_UNIT`, `CONVERSION_BASED_UNIT`,
//! `LENGTH_MEASURE_WITH_UNIT`) and the `GLOBAL_UNIT_ASSIGNED_CONTEXT` of
//! ISO 10303-41/-43, as Part 21 files write them (public standards), built
//! through solidcore-step's public p21 types. No HSMWorks material
//! consulted.

use std::collections::BTreeSet;

use solidcore_step::p21::{Entity, Exchange, Param, Record};

/// The most millimetres per unit a caller may state: a metre, the largest
/// unit a machined part is drawn in.
const MAX_MM_PER_UNIT: f64 = 1000.0;
/// The fewest: a micrometre.
const MIN_MM_PER_UNIT: f64 = 0.001;

fn record(name: &str, params: Vec<Param>) -> Record {
    Record { name: name.into(), params }
}

/// A free entity id after every id of the file.
fn next_id(ex: &Exchange) -> Result<u64, String> {
    ex.entities
        .keys()
        .next_back()
        .copied()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| "the file uses the last entity id, so no unit can be added".into())
}

/// Writes the length unit of `mm_per_unit` millimetres into every
/// representation context of `ex`. The caller calls it only for a file
/// whose contexts state no length unit (`step_length_units::read` found
/// none). A context that already lists units (an angle unit) keeps them,
/// and the length unit joins the list.
///
/// The millimetre is an SI unit; any other size is a conversion-based unit
/// of that many millimetres, as a CAD system writes an inch.
pub(crate) fn assume(ex: &mut Exchange, mm_per_unit: f64) -> Result<(), String> {
    if !(mm_per_unit.is_finite() && (MIN_MM_PER_UNIT..=MAX_MM_PER_UNIT).contains(&mm_per_unit)) {
        return Err(format!(
            "a stated length unit of {mm_per_unit} mm is outside {MIN_MM_PER_UNIT}..={MAX_MM_PER_UNIT} mm"
        ));
    }
    let contexts: BTreeSet<u64> = ex
        .entities
        .values()
        .filter_map(|e| crate::step_length_units::representation(e).and_then(|(_, ctx)| ctx))
        .filter(|ctx| ex.entities.contains_key(ctx))
        .collect();
    if contexts.is_empty() {
        return Err("the file has no representation context to state a length unit in".into());
    }
    let mm = next_id(ex)?;
    ex.entities.insert(
        mm,
        Entity {
            records: vec![
                record("LENGTH_UNIT", vec![]),
                record("NAMED_UNIT", vec![Param::Derived]),
                record("SI_UNIT", vec![Param::Enum("MILLI".into()), Param::Enum("METRE".into())]),
            ],
        },
    );
    let unit = if (mm_per_unit - 1.0).abs() <= f64::EPSILON {
        mm
    } else {
        let measure = next_id(ex)?;
        ex.entities.insert(
            measure,
            Entity {
                records: vec![record(
                    "LENGTH_MEASURE_WITH_UNIT",
                    vec![Param::Typed("LENGTH_MEASURE".into(), vec![Param::Real(mm_per_unit)]), Param::Ref(mm)],
                )],
            },
        );
        let conv = next_id(ex)?;
        ex.entities.insert(
            conv,
            Entity {
                records: vec![
                    record("CONVERSION_BASED_UNIT", vec![Param::Str("STATED BY THE CALLER".into()), Param::Ref(measure)]),
                    record("LENGTH_UNIT", vec![]),
                    record("NAMED_UNIT", vec![Param::Derived]),
                ],
            },
        );
        conv
    };
    for ctx in contexts {
        let Some(e) = ex.entities.get_mut(&ctx) else { continue };
        let listed = e
            .records
            .iter_mut()
            .find(|r| r.name == "GLOBAL_UNIT_ASSIGNED_CONTEXT")
            .and_then(|r| r.params.first_mut());
        match listed {
            Some(Param::List(units)) => units.push(Param::Ref(unit)),
            Some(_) => return Err(format!("unit context #{ctx} has no unit list")),
            None => e.records.push(record("GLOBAL_UNIT_ASSIGNED_CONTEXT", vec![Param::List(vec![Param::Ref(unit)])])),
        }
    }
    Ok(())
}
