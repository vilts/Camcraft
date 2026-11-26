//! STEP length units on the model read (#306, #2919): which length units
//! the file's representation contexts state, and the refusals for a length
//! unit that the reader would silently read as millimetres.
//! Provenance: the unit entities of ISO 10303-41 and the representation
//! entities of ISO 10303-43 as Part 21 files write them (public standards),
//! read through solidcore-step's public `p21` and `units` API. No HSMWorks
//! material was consulted.

use std::collections::{HashMap, HashSet};

use solidcore_step::p21::{self, Entity, Exchange, Param};
use solidcore_step::{topology, units};

/// Millimetres per unit that the reader takes for an item with no length
/// unit (`solidcore_step` `Units::default`).
const DEFAULT_MM: f64 = 1.0;

/// What the file's unit contexts state about length.
#[derive(Debug, Default)]
pub(crate) struct LengthUnits {
    /// Millimetres per unit of each context that states a length unit,
    /// distinct values in the order met.
    pub mm: Vec<f64>,
    /// The contexts that state a length unit.
    stating: HashSet<u64>,
    /// Each representation item's context, the first representation by
    /// entity id that lists it, and each piece split from a surface model
    /// in that model's context (the reader's own rules).
    context_of: HashMap<u64, u64>,
}

impl LengthUnits {
    /// Why item `item` (a solid, a sheet, or a piece split from a surface
    /// model) must be refused: the reader takes 1 mm per unit for this
    /// item, but the file states a different length unit, so the item
    /// would be 25.4 or 1000 times off. That is the case when the item's
    /// context states no length unit, or when no representation gives the
    /// item a context. `None`: the item's context states its unit, the file
    /// states no length unit (the millimetre default, #306), or every unit
    /// the file states is the millimetre, so the reader's default is the
    /// file's own unit (a file of bare topology beside a mm context).
    pub(crate) fn unstated(&self, item: u64) -> Option<String> {
        if self.mm.iter().all(|f| (f - DEFAULT_MM).abs() <= DEFAULT_MM * 1e-12) {
            return None;
        }
        match self.context_of.get(&item) {
            Some(c) if self.stating.contains(c) => None,
            Some(c) => Some(format!(
                "its context #{c} states no length unit, but the file states {:?} mm per unit",
                self.mm
            )),
            None => Some(format!(
                "no representation gives it a context, so it has no length unit, \
                 but the file states {:?} mm per unit",
                self.mm
            )),
        }
    }
}

/// The length units the file states. `Err` refuses the file by name: a
/// length unit the reader cannot convert, a context that names a unit
/// that is not in the file, or a length unit in the file that no context
/// states (the context is missing, or in a form not read). Each of these
/// would read the model at 1 mm per unit without a word (#306, #2919).
pub(crate) fn read(ex: &Exchange) -> Result<LengthUnits, String> {
    let mut out = LengthUnits::default();
    let mut ids: Vec<u64> = ex.entities.keys().copied().collect();
    ids.sort_unstable();
    for &id in &ids {
        let Some(e) = ex.get(id) else { continue };
        if let Some((items, Some(ctx))) = representation(e) {
            for item in items {
                out.context_of.entry(item).or_insert(ctx);
            }
        }
        let Some(list) = e
            .record("GLOBAL_UNIT_ASSIGNED_CONTEXT")
            .and_then(|r| r.first())
            .and_then(Param::as_list)
        else {
            continue;
        };
        let mut states_length = false;
        for u in list.iter().filter_map(Param::as_ref_id) {
            let Some(unit) = ex.get(u) else {
                return Err(format!(
                    "unit context #{id} names unit #{u}, which is not in the file"
                ));
            };
            if !unit.has("LENGTH_UNIT") {
                continue;
            }
            states_length = true;
            if convertible(ex, u, unit).is_none() {
                return Err(format!("length unit #{u} cannot be converted to millimetres"));
            }
        }
        if !states_length {
            continue;
        }
        out.stating.insert(id);
        let len = units::context(ex, id).units.len;
        if !out.mm.iter().any(|f| f.to_bits() == len.to_bits()) {
            out.mm.push(len);
        }
    }
    split_pieces(ex, &ids, &mut out.context_of);
    if out.mm.is_empty()
        && let Some(u) = unstated_length_unit(ex)
    {
        return Err(format!(
            "length unit #{u} is in the file, but no GLOBAL_UNIT_ASSIGNED_CONTEXT \
             states a length unit (the unit context is missing, or it is written \
             in a form this reader does not read)"
        ));
    }
    Ok(out)
}

/// The pieces that the topology reader splits from a surface model (each
/// closed shell and each open shell of a mixed model, #2919 review) sit in
/// the model's context: the reader's own rule, so that each piece is
/// judged in the context that the reader reads it in.
fn split_pieces(ex: &Exchange, ids: &[u64], context_of: &mut HashMap<u64, u64>) {
    for &id in ids {
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
}

/// Millimetres per unit of length unit `#u`, when the reader converts it:
/// an SI length is a (prefixed) metre, anything else must be a conversion
/// the reader can follow to one.
fn convertible(ex: &Exchange, u: u64, unit: &Entity) -> Option<f64> {
    let si_metre = unit.record("SI_UNIT").map(|si| si.get(1).and_then(Param::as_enum) == Some("METRE"));
    match (si_metre, units::unit_factor(ex, u)) {
        (Some(false), _) => None,
        (None, _) if unit.record("CONVERSION_BASED_UNIT").is_none() => None,
        (_, Some((units::UnitKind::Length, f))) if f.is_finite() && f > 0.0 => Some(f),
        _ => None,
    }
}

/// The first length-unit entity of the file. Evidence of a length unit: a
/// `LENGTH_UNIT`, an `SI_UNIT` of the metre, or a `CONVERSION_BASED_UNIT`
/// whose measure is a `LENGTH_MEASURE_WITH_UNIT`. Angle units are not.
/// The same rule as the tool-body read (`camcraft_geometry::step_units`).
fn unstated_length_unit(ex: &Exchange) -> Option<u64> {
    let metre = |a: &[Param]| {
        a.get(1)
            .and_then(Param::as_enum)
            .is_some_and(|e| e.trim_matches('.').eq_ignore_ascii_case("METRE"))
    };
    let length_measure = |a: &[Param]| {
        a.get(1)
            .and_then(Param::as_ref_id)
            .and_then(|m| ex.get(m))
            .is_some_and(|m| m.has("LENGTH_MEASURE_WITH_UNIT"))
    };
    let mut ids: Vec<u64> = ex.entities.keys().copied().collect();
    ids.sort_unstable();
    ids.into_iter().find(|&id| {
        ex.get(id).is_some_and(|e| {
            e.has("LENGTH_UNIT")
                || e.record("SI_UNIT").is_some_and(metre)
                || e.record("CONVERSION_BASED_UNIT").is_some_and(length_measure)
        })
    })
}

/// Items and context of a representation entity (plain or complex): the
/// `representation` attributes (name, items, context_of_items).
pub(crate) fn representation(e: &Entity) -> Option<(Vec<u64>, Option<u64>)> {
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
    let items = p.get(1)?.as_list()?.iter().filter_map(p21::Param::as_ref_id).collect();
    Some((items, p.get(2).and_then(Param::as_ref_id)))
}
