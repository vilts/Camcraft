//! STEP length units (#306): how many millimetres one of a file's
//! length units is, so a part saved in inches or metres imports at its
//! true size instead of having its numbers read as millimetres.
//!
//! Provenance: the unit entities of ISO 10303-41 as Part 21 files write
//! them - `GLOBAL_UNIT_ASSIGNED_CONTEXT`, `LENGTH_UNIT`, `SI_UNIT`
//! (prefix, name), `CONVERSION_BASED_UNIT` (name, measure) and
//! `LENGTH_MEASURE_WITH_UNIT` (value, unit). Public standard; nothing
//! from any CAM vendor. Read from solidcore's Part 21 exchange since
//! #2789 (it was ruststep's); moved from the app with it.

use camcraft_kernel::p21::{self, Exchange, Param};

/// How deep a conversion-based unit may chain to other units before it
/// is refused: a real file goes INCH -> millimetre in one step.
const MAX_CHAIN: usize = 4;
/// Two contexts whose units differ by less than this (relative) state
/// the same unit (a file's `25.4` and `25.400000000000002`).
const SAME_UNIT: f64 = 1e-9;
/// The most typed wrappers (`LENGTH_MEASURE(..)`) [`real`] looks through.
const MAX_TYPED: usize = 4;

/// [`mm_per_unit`] of a whole Part 21 text. `Err` also when the text does
/// not parse (the parser's message).
pub fn length_unit_mm(text: &str) -> Result<Option<f64>, String> {
    let ex = p21::parse(text).map_err(|e| e.to_string())?;
    mm_per_unit(&ex)
}

/// Millimetres in one length unit of the file, read from every
/// representation context's `GLOBAL_UNIT_ASSIGNED_CONTEXT`.
///
/// `Ok(None)`: the file has no length unit at all; the caller keeps
/// millimetres, as every import did before. `Err`: a stated unit this
/// reader can't convert, contexts that disagree, or a length unit in the
/// file that no context states (see [`unstated_length_unit`]). A refusal
/// names it, because guessing a part's scale is the silent wrong answer.
pub(crate) fn mm_per_unit(ex: &Exchange) -> Result<Option<f64>, String> {
    let found = stated_mm_per_unit(ex)?;
    match (found, unstated_length_unit(ex)) {
        (None, Some(id)) => Err(format!(
            "length unit #{id} is in the file, but no GLOBAL_UNIT_ASSIGNED_CONTEXT \
             states a length unit (the unit context is missing, or it is written \
             in a form this reader does not read)"
        )),
        _ => Ok(found),
    }
}

/// The first length-unit entity of the file, when no context states one
/// (#2789 review). The parser maps some ISO 10303-21 short names to their
/// long names (`LNGUNT`, `CNBSUN`, `LMWU`) but not the unit context's, so a
/// short-name file shows its unit but not the context that assigns it.
/// Reading it at 1 mm per unit would put an inch or metre file 25.4 or
/// 1000 times too small. Evidence of a length unit: a `LENGTH_UNIT`, an
/// `SI_UNIT` of the metre, or a `CONVERSION_BASED_UNIT` whose measure is a
/// `LENGTH_MEASURE_WITH_UNIT`. Angle units (radian, degree) are not.
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
    ex.entities.iter().find_map(|(&id, e)| {
        let length = e.has("LENGTH_UNIT")
            || e.record("SI_UNIT").is_some_and(metre)
            || e.record("CONVERSION_BASED_UNIT").is_some_and(length_measure);
        length.then_some(id)
    })
}

/// The length unit that the file's `GLOBAL_UNIT_ASSIGNED_CONTEXT`s state.
fn stated_mm_per_unit(ex: &Exchange) -> Result<Option<f64>, String> {
    let mut found: Option<f64> = None;
    for e in ex.entities.values() {
        let Some(a) = e.record("GLOBAL_UNIT_ASSIGNED_CONTEXT") else {
            continue;
        };
        let Some(units) = a.first().and_then(Param::as_list) else {
            return Err("GLOBAL_UNIT_ASSIGNED_CONTEXT has no unit list".into());
        };
        for id in units.iter().filter_map(Param::as_ref_id) {
            if !ex.get(id).is_some_and(|u| u.has("LENGTH_UNIT")) {
                continue; // the angle and solid-angle units ride along
            }
            let k = length_mm(ex, id, 0)?;
            match found {
                Some(prev) if ((prev - k) / prev).abs() > SAME_UNIT => {
                    return Err(format!(
                        "the file states two length units ({prev} mm and {k} mm per unit)"
                    ));
                }
                _ => found = Some(k),
            }
        }
    }
    Ok(found)
}

/// Millimetres in the length unit entity `#id`.
fn length_mm(ex: &Exchange, id: u64, depth: usize) -> Result<f64, String> {
    if depth > MAX_CHAIN {
        return Err(format!(
            "length unit #{id} chains through more than {MAX_CHAIN} units"
        ));
    }
    let unit = ex
        .get(id)
        .ok_or_else(|| format!("length unit #{id} is not in the file"))?;
    if let Some(si) = unit.record("SI_UNIT") {
        return si_mm(id, si);
    }
    if let Some(conv) = unit.record("CONVERSION_BASED_UNIT") {
        let Some(measure) = conv.get(1).and_then(Param::as_ref_id) else {
            return Err(format!("conversion-based unit #{id} names no measure"));
        };
        let m = ex
            .get(measure)
            .and_then(|e| e.record("LENGTH_MEASURE_WITH_UNIT"))
            .ok_or_else(|| {
                format!("conversion-based unit #{id}: #{measure} is no length measure")
            })?;
        let (value, base) = match m {
            [v, Param::Ref(base), ..] => (real(v), *base),
            _ => (None, 0),
        };
        let Some(value) = value.filter(|v| v.is_finite() && *v > 0.0) else {
            return Err(format!(
                "length measure #{measure} states no positive value"
            ));
        };
        return Ok(value * length_mm(ex, base, depth + 1)?);
    }
    Err(format!(
        "length unit #{id} is neither an SI unit nor a conversion-based one"
    ))
}

/// `SI_UNIT(prefix, name)` for a length: the metre, with its prefix.
fn si_mm(id: u64, a: &[Param]) -> Result<f64, String> {
    let word = |p: Option<&Param>| p.and_then(Param::as_enum).map(|e| e.trim_matches('.').to_ascii_uppercase());
    if word(a.get(1)).as_deref() != Some("METRE") {
        return Err(format!("length unit #{id} is an SI unit but not the metre"));
    }
    // Millimetres per prefixed metre; no prefix (`$`) is the metre itself.
    match word(a.first()).as_deref() {
        None => Ok(1000.0),
        Some("KILO") => Ok(1.0e6),
        Some("DECI") => Ok(100.0),
        Some("CENTI") => Ok(10.0),
        Some("MILLI") => Ok(1.0),
        Some("MICRO") => Ok(1.0e-3),
        Some("NANO") => Ok(1.0e-6),
        Some(p) => Err(format!("length unit #{id}: SI prefix {p} is not supported")),
    }
}

/// A measure value: a bare real, or one typed as `LENGTH_MEASURE(..)`.
/// A real that is not finite is handed out as read, so the caller names
/// it; [`Param::as_f64`] would hide it as "not a number".
pub(crate) fn real(p: &Param) -> Option<f64> {
    let mut p = p;
    for _ in 0..=MAX_TYPED {
        match p {
            Param::Real(x) => return Some(*x),
            Param::Int(i) => return Some(*i as f64),
            Param::Typed(_, v) if v.len() == 1 => p = v.first()?,
            _ => return None,
        }
    }
    None
}
