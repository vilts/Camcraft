//! A tool's cutting data per workpiece material (#890): one row of
//! spindle speed and feed for each material group of the built-in table
//! ([`crate::cutting::WorkMaterial`]), so one tool library serves a shop
//! that cuts several materials with the same cutters.
//!
//! Provenance: our own data model; the per-material speed/feed table is
//! the ordinary shape of a public tooling catalog's cutting-data page.
//! Pure module: no flecs, no I/O. The tool's own `rpm` / `feed` stay its
//! running numbers (what the post reads) and the fallback when no row
//! names the document's material; a row only moves them through a
//! command (the app's `cutting_rows` seam), never on load.

use crate::cutting::WorkMaterial;

/// One material's starting spindle speed (rpm) and feed (mm/min).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CuttingRow {
    pub material: WorkMaterial,
    pub rpm: f64,
    pub feed: f64,
}

/// The row for `material`, if the table names it. `None` for an
/// unstated material: nothing is matched to "no material".
#[must_use]
pub fn row_for(rows: &[CuttingRow], material: Option<WorkMaterial>) -> Option<&CuttingRow> {
    let m = material?;
    rows.iter().find(|r| r.material == m)
}

/// The refusal text for a table that cannot be stored, or `None`.
/// Every row states a finite positive feed and a finite positive rpm
/// (a torch, which turns no spindle, states rpm 0 - the #670 law), and
/// no material appears twice.
#[must_use]
pub fn refusal(rows: &[CuttingRow], torch: bool) -> Option<String> {
    for (i, r) in rows.iter().enumerate() {
        let what = r.material.label();
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
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(m: WorkMaterial, rpm: f64, feed: f64) -> CuttingRow {
        CuttingRow { material: m, rpm, feed }
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
