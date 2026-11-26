//! #3896 (U5 of #3663): the arithmetic of the Recipes tab, over the
//! window's draft only. Surface speed and feed per tooth are read from a
//! row; "Suggest" and "Copy from tool" write the draft and queue nothing.
//! Apply lands the draft as one `ChangeTool`, as for every other tool
//! number.
//! Provenance: the handbook relations Vc = pi x D x n / 1000 and
//! fz = F / (n x z), and the cutting table of `camcraft_tools::cutting`
//! through `camcraft_app::cutting::compute`; no HSMWorks material involved.

use camcraft_app::cutting;
use camcraft_app::machine::Machine;
use camcraft_app::tool_cutting_rows::CuttingRow;
use camcraft_app::tools::{ToolKind, WorkMaterial};

use crate::state::{ToolUi, is_spun_kind};

/// A rotating cutter: its rows have a spindle speed that the tool's own
/// diameter turns into a surface speed. A lathe steel's surface speed is
/// the work's diameter (its own turning data), and a torch, nozzle,
/// electrode, wire or probe turns no spindle.
#[must_use]
pub(crate) fn rotating(kind: ToolKind) -> bool {
    is_spun_kind(kind)
        && !matches!(
            kind,
            ToolKind::Torch | ToolKind::Waterjet | ToolKind::Electrode | ToolKind::Wire | ToolKind::Probe
        )
}

/// Surface speed, m/min: Vc = pi x D x n / 1000. `None` when the
/// diameter or the spindle speed is not above zero.
#[must_use]
pub(crate) fn surface_speed(d: f64, rpm: f64) -> Option<f64> {
    (d > 0.0 && rpm > 0.0 && d.is_finite() && rpm.is_finite())
        .then(|| std::f64::consts::PI * d * rpm / 1000.0)
}

/// Feed per tooth, mm: fz = F / (n x z). `None` when the spindle speed
/// is not above zero, or the flute count is not stated.
#[must_use]
pub(crate) fn feed_per_tooth(feed: f64, rpm: f64, flutes: Option<u32>) -> Option<f64> {
    let z = flutes.filter(|z| *z > 0)?;
    (rpm > 0.0 && rpm.is_finite() && feed.is_finite()).then(|| feed / (rpm * f64::from(z)))
}

/// Whether `rows` has a plain row that names material `m`.
#[must_use]
pub(crate) fn names_material(rows: &[CuttingRow], m: WorkMaterial) -> bool {
    rows.iter().any(|r| r.material == Some(m) && !r.is_recipe())
}

/// Whether `row` is the row the document's material runs: the plain row
/// that names it, else (`named` is false) the tool's default (`Unknown`)
/// row. No row is marked when no material is stated.
#[must_use]
pub(crate) fn is_marked(row: &CuttingRow, doc: Option<WorkMaterial>, named: bool) -> bool {
    match doc {
        None => false,
        Some(m) if named => row.material == Some(m) && !row.is_recipe(),
        Some(_) => row.is_unknown(),
    }
}

/// Why "Suggest" cannot speak for the draft, by name; `None` when it can.
#[must_use]
pub(crate) fn suggest_refusal(t: &ToolUi) -> Option<String> {
    let kind = t.tool_kind();
    if !cutting::tabulated(kind) {
        return Some(format!(
            "the cutting-data table speaks for end, ball, bull nose and chamfer mills, not for a {kind}"
        ));
    }
    if !(t.diameter > 0.0 && t.diameter.is_finite()) {
        return Some("the diameter must be above zero".into());
    }
    if t.tool_material.is_none() {
        return Some("state the tool material (HSS or carbide) first".into());
    }
    if t.flutes.is_none_or(|z| z == 0) {
        return Some("state the flute count first".into());
    }
    None
}

/// "Suggest": the table's spindle speed and feed for material `m` go
/// into the draft's row for `m` (a new row when the draft has none). The
/// row's stepdown and stepover stay. The machine's stated spindle range
/// and feed ceiling clamp the numbers, as on the operation page. Writes
/// only the draft; `Err` names why the table cannot speak.
pub(crate) fn suggest(t: &mut ToolUi, m: WorkMaterial, machine: Option<&Machine>) -> Result<(), String> {
    if let Some(why) = suggest_refusal(t) {
        return Err(why);
    }
    let (Some(tm), Some(z)) = (t.tool_material, t.flutes) else {
        return Err("state the tool material and the flute count first".into());
    };
    let s = cutting::compute(&t.name, &t.to_tool(), z, tm, m, None, machine);
    if !(s.rpm > 0.0 && s.feed > 0.0) {
        return Err(format!("the table gives no speed for {}", m.label()));
    }
    match t.cutting.iter_mut().find(|r| r.material == Some(m) && !r.is_recipe()) {
        Some(row) => {
            row.rpm = s.rpm;
            row.feed = s.feed;
        }
        None => t.cutting.push(CuttingRow::plain(Some(m), s.rpm, s.feed)),
    }
    Ok(())
}

/// The rows "Copy from tool" takes: the plain material rows. The source's
/// default row is its own running speed, and a torch or nozzle recipe
/// belongs to its kind.
pub(crate) fn copyable(rows: &[CuttingRow]) -> impl Iterator<Item = &CuttingRow> {
    rows.iter().filter(|r| r.material.is_some() && !r.is_recipe())
}

/// "Copy from tool": each plain material row of `from` replaces the
/// draft's row for the same material, or is added. The draft's default
/// row, its recipes and its rows of other materials stay. Returns the
/// count of rows copied.
pub(crate) fn copy_rows(t: &mut ToolUi, from: &[CuttingRow]) -> usize {
    let mut n = 0;
    for src in copyable(from) {
        match t.cutting.iter_mut().find(|r| r.material == src.material && !r.is_recipe()) {
            Some(row) => *row = *src,
            None => t.cutting.push(*src),
        }
        n += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_speed_and_feed_per_tooth_follow_the_handbook_relations() {
        let vc = surface_speed(10.0, 10_000.0).unwrap_or_default();
        assert!((vc - 314.159).abs() < 1e-3, "{vc}");
        let fz = feed_per_tooth(1200.0, 10_000.0, Some(3)).unwrap_or_default();
        assert!((fz - 0.04).abs() < 1e-12, "{fz}");
        assert_eq!(surface_speed(0.0, 1000.0), None);
        assert_eq!(feed_per_tooth(1200.0, 0.0, Some(3)), None);
        assert_eq!(feed_per_tooth(1200.0, 1000.0, None), None);
        assert_eq!(feed_per_tooth(1200.0, 1000.0, Some(0)), None);
    }

    #[test]
    fn the_marked_row_falls_back_to_the_default_row() {
        let rows = [
            CuttingRow::plain(None, 9000.0, 900.0),
            CuttingRow::plain(Some(WorkMaterial::Brass), 8000.0, 800.0),
        ];
        let marks = |doc: Option<WorkMaterial>| -> Vec<bool> {
            let named = doc.is_some_and(|m| names_material(&rows, m));
            rows.iter().map(|r| is_marked(r, doc, named)).collect()
        };
        assert_eq!(marks(Some(WorkMaterial::Brass)), [false, true]);
        assert_eq!(marks(Some(WorkMaterial::MildSteel)), [true, false], "no row names it: the default row runs");
        assert_eq!(marks(None), [false, false]);
    }
}
