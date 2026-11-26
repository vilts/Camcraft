//! A tool's cutting data per workpiece material (#890): one row of
//! spindle speed and feed for each material group of the built-in table
//! ([`crate::cutting::WorkMaterial`]), so one tool library serves a shop
//! that cuts several materials with the same cutters.
//!
//! #2015: every tool also has the `Unknown` row (`material: None`, file
//! word [`UNKNOWN_WORD`]) - the numbers it runs when no row names the
//! material in force, and the fallback for a row's missing stepdown or
//! stepover. A row may state a stepdown and a stepover (mm) besides its
//! rpm and feed. Canonical form: the `Unknown` row is STORED only when it
//! says something the tool's own rpm / feed do not ([`canonical`]);
//! otherwise it is derived from them ([`unknown_of`]), so a tool with
//! just its own numbers keeps an empty table, in memory and on disk.
//!
//! Provenance: our own data model; the per-material speed/feed table is
//! the ordinary shape of a public tooling catalog's cutting-data page.
//! Pure module: no flecs, no I/O. The tool's own `rpm` / `feed` stay its
//! running numbers (what the post reads); a row only moves them through a
//! command (the app's `cutting_rows` seam), never on load.

use crate::cutting::WorkMaterial;

/// The word the files write for the `Unknown` row's material.
pub const UNKNOWN_WORD: &str = "unknown";
/// The label the editor shows for it.
pub const UNKNOWN_LABEL: &str = "Unknown";

/// One material's starting spindle speed (rpm) and feed (mm/min), and
/// (#2015) its stepdown and stepover (mm) where stated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CuttingRow {
    /// `None` is the `Unknown` row (#2015).
    pub material: Option<WorkMaterial>,
    pub rpm: f64,
    pub feed: f64,
    /// Axial depth per level, mm; `None` = not stated (the `Unknown`
    /// row's, else the operation's own).
    pub stepdown: Option<f64>,
    /// Radial width between passes, mm; `None` as `stepdown`.
    pub stepover: Option<f64>,
    /// A material-and-thickness recipe, never selected by material alone.
    pub waterjet: Option<crate::waterjet::WaterjetCutData>,
    /// #1521: a torch's recipe for one material and plate thickness,
    /// never selected by material alone.
    pub plasma: Option<crate::plasma_cut::PlasmaCutData>,
}

impl CuttingRow {
    /// A plain speed-and-feed row with no step data.
    #[must_use]
    pub const fn plain(material: Option<WorkMaterial>, rpm: f64, feed: f64) -> Self {
        Self { material, rpm, feed, stepdown: None, stepover: None, waterjet: None, plasma: None }
    }

    /// Is this the `Unknown` row?
    #[must_use]
    pub fn is_unknown(&self) -> bool {
        self.material.is_none() && !self.is_recipe()
    }

    /// Is this a material-and-thickness recipe (a waterjet's, or #1521 a
    /// torch's)? A recipe is never the row in force for a material.
    #[must_use]
    pub fn is_recipe(&self) -> bool {
        self.waterjet.is_some() || self.plasma.is_some()
    }
}

/// The row label: the material's, or "Unknown".
#[must_use]
pub fn material_label(m: Option<WorkMaterial>) -> &'static str {
    m.map_or(UNKNOWN_LABEL, WorkMaterial::label)
}

/// The row keys an editor offers, `Unknown` first.
#[must_use]
pub fn row_keys() -> Vec<Option<WorkMaterial>> {
    std::iter::once(None).chain(WorkMaterial::ALL.into_iter().map(Some)).collect()
}

/// The `Unknown` row of a table, if it has one.
#[must_use]
pub fn unknown_row(rows: &[CuttingRow]) -> Option<&CuttingRow> {
    rows.iter().find(|r| r.is_unknown())
}

/// The row in force for `material` (#2015): the row naming it, else the
/// `Unknown` row (also for an unstated material). `None` only for a
/// table with neither - a waterjet's, or an electrode's.
#[must_use]
pub fn row_for(rows: &[CuttingRow], material: Option<WorkMaterial>) -> Option<&CuttingRow> {
    material
        .and_then(|m| rows.iter().find(|r| r.material == Some(m) && !r.is_recipe()))
        .or_else(|| unknown_row(rows))
}

/// The stepdown and stepover in force for `material`: the row's own,
/// each missing one from the `Unknown` row (#2015).
#[must_use]
pub fn steps_for(rows: &[CuttingRow], material: Option<WorkMaterial>) -> (Option<f64>, Option<f64>) {
    let row = row_for(rows, material);
    let unknown = unknown_row(rows);
    let down = row.and_then(|r| r.stepdown).or_else(|| unknown.and_then(|r| r.stepdown));
    let over = row.and_then(|r| r.stepover).or_else(|| unknown.and_then(|r| r.stepover));
    (down, over)
}

/// The `Unknown` row derived from `tool`'s own rpm / feed, or `None`
/// for a kind whose table cannot hold it (a waterjet's recipes, an
/// electrode with no spindle) - so a sane tool never turns insane.
#[must_use]
pub fn derived_unknown(tool: &crate::tools::Tool) -> Option<CuttingRow> {
    let row = CuttingRow::plain(None, tool.rpm, tool.feed);
    refusal_for_kind(std::slice::from_ref(&row), tool.kind).is_none().then_some(row)
}

/// The tool's `Unknown` row: the stored one, else the derived one.
#[must_use]
pub fn unknown_of(tool: &crate::tools::Tool) -> Option<CuttingRow> {
    unknown_row(&tool.cutting).copied().or_else(|| derived_unknown(tool))
}

/// The row in force for `material` on `tool`: the row naming it, else
/// [`unknown_of`].
#[must_use]
pub fn row_in_tool(tool: &crate::tools::Tool, material: Option<WorkMaterial>) -> Option<CuttingRow> {
    material
        .and_then(|m| tool.cutting.iter().find(|r| r.material == Some(m) && !r.is_recipe()).copied())
        .or_else(|| unknown_of(tool))
}

/// [`steps_for`] over the tool's table with its `Unknown` row.
#[must_use]
pub fn steps_in_tool(tool: &crate::tools::Tool, material: Option<WorkMaterial>) -> (Option<f64>, Option<f64>) {
    steps_for(&rows_with_unknown(tool), material)
}

/// The table with the `Unknown` row in front - stored or derived - as
/// an editor shows it.
#[must_use]
pub fn rows_with_unknown(tool: &crate::tools::Tool) -> Vec<CuttingRow> {
    let mut rows = tool.cutting.clone();
    if unknown_row(&rows).is_none()
        && let Some(u) = derived_unknown(tool)
    {
        rows.insert(0, u);
    }
    rows
}

/// Store the derived `Unknown` row (before a command moves the running
/// numbers away from it, so the default is not lost). `true` when one
/// was stored.
pub fn ensure_unknown_row(tool: &mut crate::tools::Tool) -> bool {
    if unknown_row(&tool.cutting).is_some() {
        return false;
    }
    let Some(row) = derived_unknown(tool) else { return false };
    tool.cutting.insert(0, row);
    true
}

/// Drop a stored `Unknown` row that only repeats the running numbers
/// (the canonical form, module doc).
pub fn canonical(tool: &mut crate::tools::Tool) {
    let (rpm, feed) = (tool.rpm, tool.feed);
    tool.cutting.retain(|r| !is_derived_unknown(r, rpm, feed));
}

/// Is `row` exactly what [`ensure_unknown_row`] would derive from the
/// running numbers? Then a file need not write it (#2015: a tool with
/// only its own rpm / feed saves byte for byte as before).
#[must_use]
pub fn is_derived_unknown(row: &CuttingRow, rpm: f64, feed: f64) -> bool {
    *row == CuttingRow::plain(None, rpm, feed)
}

/// A waterjet recipe requires both keys; no nearest-thickness substitution.
#[must_use]
pub fn waterjet_row_for(rows: &[CuttingRow], material: WorkMaterial, thickness: f64) -> Option<&CuttingRow> {
    rows.iter().find(|r| r.material == Some(material) && r.waterjet.is_some_and(|w| w.thickness == thickness))
}

/// #1521: a torch's recipe requires both keys; no nearest-thickness
/// substitution.
#[must_use]
pub fn plasma_row_for(rows: &[CuttingRow], material: WorkMaterial, thickness: f64) -> Option<&CuttingRow> {
    rows.iter().find(|r| r.material == Some(material) && r.plasma.is_some_and(|p| p.thickness == thickness))
}

/// Validate a table against its tool kind before storing it.
#[must_use]
pub fn refusal_for_kind(rows: &[CuttingRow], kind: crate::tools::ToolKind) -> Option<String> {
    use crate::tools::ToolKind;
    if kind == ToolKind::Wire {
        return (!rows.is_empty()).then(|| "an EDM wire uses machine EDM conditions, not per-material cutting data rows".into());
    }
    if kind != ToolKind::Torch && rows.iter().any(|r| r.plasma.is_some()) {
        return Some("plasma cutting data requires a plasma torch".into());
    }
    if kind != ToolKind::Waterjet {
        if rows.iter().any(|r| r.waterjet.is_some()) {
            return Some("waterjet cutting data requires a waterjet tool".into());
        }
        // #1521: a torch's recipes have their own law; the plain rows keep theirs.
        let plain: Vec<CuttingRow> = rows.iter().filter(|r| r.plasma.is_none()).copied().collect();
        return refusal(&plain, kind == ToolKind::Torch).or_else(|| crate::plasma_cut::rows_refusal(rows));
    }
    for (i, r) in rows.iter().enumerate() {
        let Some(w) = r.waterjet else {
            return Some("waterjet cutting data requires thickness, kerf and pierce data".into());
        };
        if let Some(why) = w.refusal() { return Some(why); }
        if r.material.is_none() {
            return Some("waterjet cutting data names its workpiece material".into());
        }
        if r.rpm != 0.0 { return Some("waterjet cutting data rpm must be 0".into()); }
        if !r.feed.is_finite() || r.feed <= 0.0 {
            return Some("waterjet cutting data feed must be finite and above zero".into());
        }
        if rows[..i].iter().any(|p| p.material == r.material && p.waterjet.is_some_and(|p| p.thickness == w.thickness)) {
            return Some(format!("waterjet cutting data names {} at thickness {} twice", material_label(r.material), w.thickness));
        }
    }
    None
}

/// The refusal text for a table that cannot be stored, or `None`.
/// Every row states a finite positive feed and a finite positive rpm
/// (a torch, which turns no spindle, states rpm 0 - the #670 law), a
/// stated stepdown or stepover is finite and positive (#2015), and no
/// material (nor `Unknown`) appears twice.
#[must_use]
pub fn refusal(rows: &[CuttingRow], torch: bool) -> Option<String> {
    for (i, r) in rows.iter().enumerate() {
        let what = material_label(r.material);
        if rows[..i].iter().any(|p| p.material == r.material) {
            return Some(format!("cutting data names {what} twice"));
        }
        if !(r.feed.is_finite() && r.feed > 0.0) {
            return Some(format!("cutting data for {what}: feed must be a finite number above zero (it states {})", r.feed));
        }
        let rpm_ok = if torch { r.rpm == 0.0 } else { r.rpm.is_finite() && r.rpm > 0.0 };
        if !rpm_ok {
            let want = if torch { "0 on a torch" } else { "a finite number above zero" };
            return Some(format!("cutting data for {what}: spindle rpm must be {want} (it states {})", r.rpm));
        }
        for (name, v) in [("stepdown", r.stepdown), ("stepover", r.stepover)] {
            if let Some(v) = v
                && !(v.is_finite() && v > 0.0)
            {
                return Some(format!("cutting data for {what}: {name} must be a finite number above zero (it states {v})"));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(m: WorkMaterial, rpm: f64, feed: f64) -> CuttingRow {
        CuttingRow::plain(Some(m), rpm, feed)
    }

    #[test]
    fn the_row_for_a_material_is_found_and_none_matches_unstated() {
        let rows = [row(WorkMaterial::MildSteel, 3000.0, 400.0), row(WorkMaterial::Brass, 8000.0, 900.0)];
        assert_eq!(row_for(&rows, Some(WorkMaterial::Brass)).map(|r| r.rpm), Some(8000.0));
        assert!(row_for(&rows, Some(WorkMaterial::Plastics)).is_none());
        assert!(row_for(&rows, None).is_none());
    }

    #[test]
    fn bad_rows_are_refused_by_material() {
        let dup = [row(WorkMaterial::Brass, 1.0, 1.0), row(WorkMaterial::Brass, 2.0, 2.0)];
        assert!(refusal(&dup, false).is_some_and(|s| s.contains("Brass twice")));
        let nan = [row(WorkMaterial::MildSteel, f64::NAN, 100.0)];
        assert!(refusal(&nan, false).is_some_and(|s| s.contains("Mild steel") && s.contains("rpm")));
        let zero_feed = [row(WorkMaterial::MildSteel, 1000.0, 0.0)];
        assert!(refusal(&zero_feed, false).is_some_and(|s| s.contains("feed")));
        let inf_feed = [row(WorkMaterial::MildSteel, 1000.0, f64::INFINITY)];
        assert!(refusal(&inf_feed, false).is_some());
        assert!(refusal(&[row(WorkMaterial::MildSteel, 0.0, 2000.0)], true).is_none(), "a torch row states rpm 0");
        assert!(refusal(&[row(WorkMaterial::MildSteel, 100.0, 2000.0)], true).is_some());
        assert!(refusal(&[], false).is_none());
    }
}
