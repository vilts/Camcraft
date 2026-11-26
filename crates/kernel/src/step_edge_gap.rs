//! How far an edge of a STEP file may lie off its face's surface (#2976):
//! twice the distance uncertainty that the file states, never under
//! [`MAX_EDGE_GAP_MM`] and never over [`EDGE_GAP_LIMIT_MM`]; and the looser
//! mesh tolerance that such a model may be meshed at, which the importer
//! reports as reduced fidelity (#864) with a note in plain words.
//! Provenance: ISO 10303-41/-43 (a representation context's
//! `GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT`, read and converted to millimetres
//! by solidcore-step's units module); the rule is our own (decision 2976).
//! No HSMWorks material was consulted.

use solidcore_step::p21::Exchange;
use solidcore_step::units;

/// The farthest an edge may lie off its face's surface (mm) in a file that
/// states no uncertainty, and the least cap of any file. The reader's
/// 1 mm reach keeps edges a visible step off their faces (0.25 mm on a 1 mm
/// part reads as a solid of the wrong shape, #917). Set from data: the
/// largest gap on the fixtures that read is 1.2e-6 mm (decision 917).
pub const MAX_EDGE_GAP_MM: f64 = 0.01;

/// The cap no file can raise (mm), whatever uncertainty it states: twice
/// the stock simulation's verdict tolerance at most, and 12 times under the
/// 0.25 mm step that must stay refused (decision 2976).
pub const EDGE_GAP_LIMIT_MM: f64 = 0.02;

/// The cap is this many times the uncertainty the file states: the gap is
/// measured at a few samples and the writer rounds its numbers, so a file
/// inside its own tolerance can measure a little over it (decision 2976).
pub const STATED_FACTOR: f64 = 2.0;

/// The edge-gap cap of one file and where it came from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeGapCap {
    /// The distance uncertainty the file states (mm): the least of its
    /// representation contexts'. `None` when it states none, or only a
    /// value that is not a finite positive number.
    pub stated_mm: Option<f64>,
    /// The farthest an edge may lie off its surface (mm).
    pub cap_mm: f64,
}

impl Default for EdgeGapCap {
    fn default() -> Self {
        Self::from_stated(None)
    }
}

impl EdgeGapCap {
    /// The cap for a file that states `stated` (mm).
    #[must_use]
    pub fn from_stated(stated: Option<f64>) -> Self {
        let stated_mm = stated.filter(|s| s.is_finite() && *s > 0.0);
        let cap_mm = stated_mm.map_or(MAX_EDGE_GAP_MM, |s| {
            // `s` is finite and positive, so the product is not NaN (an
            // overflow to infinity clamps to the limit).
            (STATED_FACTOR * s).clamp(MAX_EDGE_GAP_MM, EDGE_GAP_LIMIT_MM)
        });
        Self { stated_mm, cap_mm }
    }

    /// The cap for the file `ex`.
    #[must_use]
    pub fn read(ex: &Exchange) -> Self {
        Self::from_stated(stated_uncertainty(ex))
    }

    /// The refusal of an edge `gap` mm off its surface, over the cap: the
    /// gap, the cap and where the cap came from.
    #[must_use]
    pub fn refusal(&self, edge: usize, gap: f64) -> String {
        let source = match self.stated_mm {
            Some(s) => format!(
                "the file states a tolerance of {} mm, and the reader accepts twice that, from {} mm to {} mm",
                mm(s),
                mm(MAX_EDGE_GAP_MM),
                mm(EDGE_GAP_LIMIT_MM)
            ),
            None => "the file states no tolerance".to_string(),
        };
        format!(
            "edge {edge} lies {gap:.4} mm off the face's surface, over the cap of {} mm; {source}",
            mm(self.cap_mm)
        )
    }
}

/// What the reader measured of a file's edge gaps.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EdgeGap {
    /// The file's cap.
    pub cap: EdgeGapCap,
    /// The largest gap of an edge to its face's surface (mm) that the
    /// reader kept, at most: the edges the kernel measured for their
    /// pcurves, and the edges whose tolerance the reader raised over their
    /// gap ([`raised_edge_gap`]).
    pub widest_mm: f64,
}

impl EdgeGap {
    /// The coarser mesh tolerance (mm) a model of this file may be meshed
    /// at when it cannot be meshed at `chord_mm`: twice the stated
    /// uncertainty, never over [`EDGE_GAP_LIMIT_MM`]. `None` when the file
    /// states no uncertainty, when no edge lies farther off its surface
    /// than `chord_mm` (the gap does not explain the failure), or when the
    /// looser tolerance is not coarser than `chord_mm`.
    #[must_use]
    pub fn loose_chord_mm(&self, chord_mm: f64) -> Option<f64> {
        let s = self.cap.stated_mm?;
        let loose = (STATED_FACTOR * s).min(EDGE_GAP_LIMIT_MM);
        let explains = self.widest_mm.is_finite() && self.widest_mm > chord_mm;
        (explains && loose.is_finite() && loose > chord_mm).then_some(loose)
    }

    /// The note for a model meshed at `chord_mm` because of its edge gaps,
    /// in plain words: the facts only. What the checks then do is the
    /// app's fidelity law; the app adds that sentence.
    #[must_use]
    pub fn loose_note(&self, chord_mm: f64) -> String {
        let stated = self
            .cap
            .stated_mm
            .map_or_else(|| "no tolerance".to_string(), |s| format!("a tolerance of {s:.3} mm"));
        format!(
            "This file's edges sit up to {:.3} mm off their surfaces. The file states {stated}. \
             The model is used at reduced accuracy ({chord_mm:.3} mm).",
            self.widest_mm
        )
    }
}

/// The least distance uncertainty (mm) that a representation context of
/// the file states, if any. The least: a face's own context is not known
/// where the cap is used, and the tighter one keeps the stricter cap.
#[must_use]
pub fn stated_uncertainty(ex: &Exchange) -> Option<f64> {
    ex.entities
        .iter()
        .filter(|(_, e)| e.record("GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT").is_some())
        .filter_map(|(id, _)| units::context(ex, *id).uncertainty)
        .filter(|u| u.is_finite() && *u > 0.0)
        .reduce(f64::min)
}

/// The least margin the reader raises an edge's tolerance by over the gap
/// it measured (solidcore-step's assembly, at the commit pinned in
/// Cargo.toml: 1.5 after the pcurves, `SURFACE_GAP_MARGIN` 3 while it makes
/// them).
const READER_EDGE_MARGIN: f64 = 1.5;

/// The largest gap (mm) of an edge of `record` to its surfaces, at most,
/// or 0: the reader raises the tolerance of an edge that lies off its
/// surfaces to a margin times its gap, so an edge tolerance over the
/// record's own is over its gap by [`READER_EDGE_MARGIN`] or more.
#[must_use]
pub fn raised_edge_gap(record: &solidcore_record::brep::Brep) -> f64 {
    record
        .edges
        .iter()
        .map(|e| e.tolerance)
        .filter(|t| t.is_finite() && *t > record.tolerance)
        .fold(0.0, f64::max)
        / READER_EDGE_MARGIN
}

/// A length in millimetres for a message: plain decimals, the trailing
/// zeros dropped; scientific under a ten-thousandth.
fn mm(x: f64) -> String {
    if x != 0.0 && x.abs() < 1e-4 {
        return format!("{x:e}");
    }
    let s = format!("{x:.6}");
    let s = s.trim_end_matches('0');
    s.trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cap_follows_the_stated_tolerance_inside_its_limits() {
        assert_eq!(EdgeGapCap::from_stated(None).cap_mm, MAX_EDGE_GAP_MM);
        assert_eq!(EdgeGapCap::from_stated(Some(1e-6)).cap_mm, MAX_EDGE_GAP_MM);
        assert!((EdgeGapCap::from_stated(Some(0.0075)).cap_mm - 0.015).abs() < 1e-12);
        assert_eq!(EdgeGapCap::from_stated(Some(0.01)).cap_mm, EDGE_GAP_LIMIT_MM);
        assert_eq!(EdgeGapCap::from_stated(Some(5.0)).cap_mm, EDGE_GAP_LIMIT_MM);
        assert_eq!(EdgeGapCap::from_stated(Some(f64::MAX)).cap_mm, EDGE_GAP_LIMIT_MM);
        for bad in [0.0, -0.01, f64::NAN, f64::INFINITY] {
            let c = EdgeGapCap::from_stated(Some(bad));
            assert_eq!((c.stated_mm, c.cap_mm), (None, MAX_EDGE_GAP_MM), "{bad}");
        }
    }

    #[test]
    fn the_refusal_names_the_gap_the_cap_and_its_source() {
        let s = EdgeGapCap::from_stated(Some(0.01)).refusal(2, 0.03);
        assert_eq!(
            s,
            "edge 2 lies 0.0300 mm off the face's surface, over the cap of 0.02 mm; \
             the file states a tolerance of 0.01 mm, and the reader accepts twice that, from 0.01 mm to 0.02 mm"
        );
        let s = EdgeGapCap::from_stated(None).refusal(0, 0.25);
        assert_eq!(
            s,
            "edge 0 lies 0.2500 mm off the face's surface, over the cap of 0.01 mm; the file states no tolerance"
        );
        assert!(EdgeGapCap::from_stated(Some(1e-6)).refusal(0, 0.25).contains("a tolerance of 1e-6 mm"));
    }

    #[test]
    fn a_looser_mesh_needs_a_stated_tolerance_and_a_gap_over_the_chord() {
        let gap = |stated, widest_mm| EdgeGap { cap: EdgeGapCap::from_stated(stated), widest_mm };
        assert_eq!(gap(Some(0.01), 0.015).loose_chord_mm(0.005), Some(0.02));
        assert_eq!(gap(Some(1.0), 0.015).loose_chord_mm(0.005), Some(EDGE_GAP_LIMIT_MM));
        assert_eq!(gap(None, 0.009).loose_chord_mm(0.005), None);
        assert_eq!(gap(Some(0.01), 0.004).loose_chord_mm(0.005), None);
        assert_eq!(gap(Some(1e-6), 0.009).loose_chord_mm(0.005), None);
        assert_eq!(gap(Some(0.01), 0.015).loose_chord_mm(0.02), None);
        assert_eq!(gap(Some(0.01), f64::NAN).loose_chord_mm(0.005), None);
        assert_eq!(
            gap(Some(0.01), 0.01507).loose_note(0.02),
            "This file's edges sit up to 0.015 mm off their surfaces. The file states a tolerance of 0.010 mm. \
             The model is used at reduced accuracy (0.020 mm)."
        );
    }
}
