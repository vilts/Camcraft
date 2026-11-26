//! The verdicts of the STEP bench (step_agreement.rs): the solidcore reading
//! of a file, live, judged against the recorded truck reading of its row
//! (rows.rs) and against solidcore's own mesh.
//! Provenance: our own bench design (M12.1 retirement gate, #916).
//!
//! Gate map (M12.1): covers camcraft_kernel::solidcore_step (read_step,
//! import_step).

use camcraft_kernel::solidcore_step::StepRead;
use camcraft_kernel::{KernelError, Measure};

use crate::rows::Truck;
use crate::step_util::{CHORD_MM, SolidcoreMesh};

/// truck's numbers are a mesh's at [`CHORD_MM`]; solidcore's are exact.
/// Relative volume and area distance the two may keep.
const REL_TOL: f64 = 1e-3;
/// Bounding-box corner distance the two may keep, in chords of the mesh
/// it is judged against (a mesh stands inside its solid by its sagitta).
const BOX_CHORDS: f64 = 2.0;

/// The solidcore side of one file, summed over its solids.
pub struct Whole {
    pub solids: usize,
    pub measure: Measure,
    pub area: f64,
    pub gap: f64,
}

pub fn whole(read: &StepRead) -> Option<Whole> {
    let first = read.solids.first()?;
    let mut w = Whole {
        solids: read.solids.len(),
        measure: Measure {
            volume: 0.0,
            ..first.measure
        },
        area: 0.0,
        gap: 0.0,
    };
    for s in &read.solids {
        w.measure.volume += s.measure.volume;
        w.area += s.area;
        w.gap = w.gap.max(s.vertex_gap);
        for i in 0..3 {
            w.measure.min[i] = w.measure.min[i].min(s.measure.min[i]);
            w.measure.max[i] = w.measure.max[i].max(s.measure.max[i]);
        }
    }
    Some(w)
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs().max(f64::MIN_POSITIVE)
}

/// The first few words of a refusal: enough to name it, short enough to
/// stay stable when a count or an id in its tail moves.
fn first_words(s: &str) -> String {
    s.split_whitespace().take(6).collect::<Vec<_>>().join(" ")
}

/// The importer verdict of one file: the live solidcore reading against
/// the recorded truck reading.
pub fn import_verdict(row_unit: f64, sc: &Result<StepRead, KernelError>, tr: &Truck) -> String {
    let sc_whole = match sc {
        Err(e) => Err(format!("{e}")),
        Ok(r) => match (r.refused.first(), whole(r)) {
            (Some(f), _) => Err(format!("#{} at {}: {}", f.item, f.stage, f.reason)),
            (None, None) => Err("the file holds no solid".into()),
            (None, Some(w)) => Ok((r, w)),
        },
    };
    match (sc_whole, tr) {
        (Err(a), Truck::Refused(_)) => format!("both refuse (solidcore: {})", first_words(&a)),
        (Err(a), Truck::Read { .. } | Truck::NotRecorded) => format!("solidcore refuses: {}", first_words(&a)),
        (Ok(_), Truck::Refused(b)) => format!("truck refuses: {}", first_words(b)),
        (Ok((r, _)), Truck::NotRecorded) => {
            let unit = match r.length_units_mm.as_slice() {
                [] => 1.0,
                [one] => *one,
                _ => f64::NAN,
            };
            if unit.is_nan() || rel(unit, row_unit) > 1e-12 {
                "no truck reading; off: unit".into()
            } else {
                "no truck reading; unit agrees".into()
            }
        }
        (
            Ok((r, w)),
            Truck::Read {
                shells,
                volume,
                area,
                min,
                max,
                chord_mm,
                ..
            },
        ) => {
            let mut off = Vec::new();
            let unit = match r.length_units_mm.as_slice() {
                [] => 1.0,
                [one] => *one,
                _ => f64::NAN,
            };
            if rel(unit, row_unit) > 1e-12 || unit.is_nan() {
                off.push("unit");
            }
            if w.solids != *shells {
                off.push("solid count");
            }
            let tm = Measure {
                volume: *volume,
                min: *min,
                max: *max,
            };
            if w.measure.bbox_deviation(&tm) > BOX_CHORDS * chord_mm {
                off.push("bounding box");
            }
            if rel(*volume, w.measure.volume) > REL_TOL {
                off.push("volume");
            }
            if rel(*area, w.area) > REL_TOL {
                off.push("area");
            }
            if off.is_empty() {
                "agree".into()
            } else {
                format!("disagree: {}", off.join(", "))
            }
        }
    }
}

/// The mesh verdict of one file: solidcore's mesher against the exact
/// solid, and against the recorded truck mesh at the same chord.
pub fn mesh_verdict(
    sc: &Result<StepRead, KernelError>,
    tr: &Truck,
    tess: &Result<SolidcoreMesh, String>,
) -> String {
    let Some(w) = sc.as_ref().ok().and_then(whole) else {
        return "no exact solid to judge against".into();
    };
    match tess {
        Err(e) => format!("no mesh: {}", first_words(e)),
        Ok(m) => {
            let mut off = Vec::new();
            if !m.refused_faces.is_empty() {
                off.push("faces left out");
            }
            if m.mesh.open_edges > 0 {
                off.push("open");
            }
            if rel(m.mesh.volume, w.measure.volume) > REL_TOL {
                off.push("volume");
            }
            if rel(m.mesh.area, w.area) > REL_TOL {
                off.push("area");
            }
            // The two meshes' boxes: each stands a chord inside the solid.
            if let Truck::Read {
                min, max, chord_mm, ..
            } = tr
            {
                let d = (0..3).fold(0.0_f64, |d, i| {
                    d.max((m.mesh.min[i] - min[i]).abs())
                        .max((m.mesh.max[i] - max[i]).abs())
                });
                if d > BOX_CHORDS * chord_mm.max(CHORD_MM) {
                    off.push("box off truck's mesh");
                }
            }
            if off.is_empty() {
                "watertight, within tolerance".into()
            } else {
                format!("off: {}", off.join(", "))
            }
        }
    }
}
