//! Dual-importer STEP bench (M12.1 stage 4, #916): every STEP file under
//! testdata/models through the solidcore-side importer and through
//! truck-stepio as crates/app reads it today. Per file both import, or one
//! refuses by name; where both import, solid count, bounding box, volume,
//! area and the length unit are compared within the stated tolerances. The
//! verdicts are pinned in a table like stages 1-3: a disagreement is
//! recorded, never averaged away, and a file with no row fails the bench.
//! Provenance: our own bench design; the fixtures are the repo's own.
//!
//! Gate map (M12.1): covers camcraft_kernel::solidcore_step (read_step,
//! import_step).

mod step_util;

use std::path::PathBuf;
use std::time::Instant;

use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_step::{StepRead, read_step};
use camcraft_kernel::{KernelError, Measure};
use step_util::{
    CASE_TIMEOUT, CHORD_MM, SolidcoreMesh, TruckStep, bounded_cancel, solidcore_mesh, truck_read,
};

/// truck's numbers are a mesh's at [`CHORD_MM`]; solidcore's are exact.
/// Relative volume and area distance the two may keep.
const REL_TOL: f64 = 1e-3;
/// Bounding-box corner distance the two may keep, in chords of the mesh
/// it is judged against (a mesh stands inside its solid by its sagitta).
const BOX_CHORDS: f64 = 2.0;

/// One fixture: its length unit (mm per unit, what crates/app's
/// `step_units` reads from it, pinned in crates/app/tests/import.rs) and
/// the pinned verdicts of the importer comparison and of the mesh
/// comparison.
struct Row {
    file: &'static str,
    mm_per_unit: f64,
    import: &'static str,
    mesh: &'static str,
}

/// The pinned verdicts (readings in docs/decisions/916-solidcore-step-bench.md).
///
/// DISAGREEMENT, both Milling-test files: the port's "tight" bounding box
/// of the imported solid is Y -30..30; the part is a 60 mm disc with a
/// flat at Y = 19.719 (the file's own vertices, truck's mesh and
/// solidcore's own mesh all stop there). Volume and area agree, so the
/// solid is right and its box is not: the box takes the whole cylinder,
/// not the trimmed face. Nothing may size stock from that box until it is
/// fixed (#939).
///
/// The three tool_csys files hold a coordinate system and no solid: both
/// importers refuse them by name.
const ROWS: &[Row] = &[
    Row {
        file: "Milling-test-inch.step",
        mm_per_unit: 25.4,
        import: "disagree: bounding box",
        mesh: "watertight, within tolerance",
    },
    Row {
        file: "Milling-test.step",
        mm_per_unit: 1.0,
        import: "disagree: bounding box",
        mesh: "watertight, within tolerance",
    },
    Row {
        file: "box.step",
        mm_per_unit: 1.0,
        import: "agree",
        mesh: "watertight, within tolerance",
    },
    Row {
        file: "empty_list.step",
        mm_per_unit: 1.0,
        import: "agree",
        mesh: "watertight, within tolerance",
    },
    Row {
        file: "sample_ap214.step",
        mm_per_unit: 1000.0,
        import: "agree",
        mesh: "watertight, within tolerance",
    },
    Row {
        file: "tool_csys_box_iso13399.step",
        mm_per_unit: 1.0,
        import: "agree",
        mesh: "watertight, within tolerance",
    },
    Row {
        file: "tool_csys_degenerate.step",
        mm_per_unit: 1.0,
        import: "both refuse (solidcore: the file holds no solid)",
        mesh: "no exact solid to judge against",
    },
    Row {
        file: "tool_csys_iso13399.step",
        mm_per_unit: 1.0,
        import: "both refuse (solidcore: the file holds no solid)",
        mesh: "no exact solid to judge against",
    },
    Row {
        file: "tool_csys_none.step",
        mm_per_unit: 1.0,
        import: "both refuse (solidcore: the file holds no solid)",
        mesh: "no exact solid to judge against",
    },
];

fn models_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models")
}

/// Every `.step` / `.stp` file under testdata/models, sorted by name.
fn step_files() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(models_dir())
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| {
                    let l = n.to_ascii_lowercase();
                    l.ends_with(".step") || l.ends_with(".stp")
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// The solidcore side of one file, summed over its solids.
struct Whole {
    solids: usize,
    measure: Measure,
    area: f64,
    gap: f64,
}

fn whole(read: &StepRead) -> Option<Whole> {
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

/// The importer verdict of one file.
fn import_verdict(
    row_unit: f64,
    sc: &Result<StepRead, KernelError>,
    tr: &Result<TruckStep, String>,
) -> String {
    let sc_whole = match sc {
        Err(e) => Err(format!("{e}")),
        Ok(r) => match (r.refused.first(), whole(r)) {
            (Some(f), _) => Err(format!("#{} at {}: {}", f.item, f.stage, f.reason)),
            (None, None) => Err("the file holds no solid".into()),
            (None, Some(w)) => Ok((r, w)),
        },
    };
    match (sc_whole, tr) {
        (Err(a), Err(_)) => format!("both refuse (solidcore: {})", first_words(&a)),
        (Err(a), Ok(_)) => format!("solidcore refuses: {}", first_words(&a)),
        (Ok(_), Err(b)) => format!("truck refuses: {}", first_words(b)),
        (Ok((r, w)), Ok(t)) => {
            let mut off = Vec::new();
            let unit = match r.length_units_mm.as_slice() {
                [] => 1.0,
                [one] => *one,
                _ => f64::NAN,
            };
            if rel(unit, row_unit) > 1e-12 || unit.is_nan() {
                off.push("unit");
            }
            if w.solids != t.shells {
                off.push("solid count");
            }
            let tm = Measure {
                volume: t.mesh.volume,
                min: t.mesh.min,
                max: t.mesh.max,
            };
            if w.measure.bbox_deviation(&tm) > BOX_CHORDS * t.chord_mm {
                off.push("bounding box");
            }
            if rel(t.mesh.volume, w.measure.volume) > REL_TOL {
                off.push("volume");
            }
            if rel(t.mesh.area, w.area) > REL_TOL {
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

/// The first few words of a refusal: enough to name it, short enough to
/// stay stable when a count or an id in its tail moves.
fn first_words(s: &str) -> String {
    s.split_whitespace().take(6).collect::<Vec<_>>().join(" ")
}

/// The mesh verdict of one file: solidcore's mesher against the exact
/// solid, and against truck's mesh at the same chord.
fn mesh_verdict(
    sc: &Result<StepRead, KernelError>,
    tr: &Result<TruckStep, String>,
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
            if let Ok(t) = tr {
                let d = (0..3).fold(0.0_f64, |d, i| {
                    d.max((m.mesh.min[i] - t.mesh.min[i]).abs())
                        .max((m.mesh.max[i] - t.mesh.max[i]).abs())
                });
                if d > BOX_CHORDS * t.chord_mm.max(CHORD_MM) {
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

struct Reading {
    file: String,
    import: String,
    mesh: String,
}

fn read_one(file: &str, unit: f64) -> Reading {
    let bytes = std::fs::read(models_dir().join(file)).unwrap_or_default();
    assert!(!bytes.is_empty(), "{file}: fixture missing or empty");
    let b = bytes.clone();
    let t0 = Instant::now();
    let sc = bounded_cancel(file, CASE_TIMEOUT, move |stop| {
        let mut k = SolidcoreKernel::default();
        read_step(&mut k, &b, stop)
    });
    let t_sc = t0.elapsed();
    let b = bytes.clone();
    let t0 = Instant::now();
    let tr = bounded_cancel(file, CASE_TIMEOUT, move |_| truck_read(&b, unit));
    let t_tr = t0.elapsed();
    let t0 = Instant::now();
    let tess = bounded_cancel(file, CASE_TIMEOUT, move |_| solidcore_mesh(&bytes));
    let t_tess = t0.elapsed();

    println!("== {file}");
    match &sc {
        Err(e) => println!("  solidcore  Err {e}"),
        Ok(r) => {
            if let Some(w) = whole(r) {
                println!(
                    "  solidcore  solids={} V={:.6} A={:.6} box={:?}..{:?} gap={:.2e} units={:?} ({t_sc:.1?})",
                    w.solids, w.measure.volume, w.area, w.measure.min, w.measure.max, w.gap,
                    r.length_units_mm
                );
            } else {
                println!("  solidcore  no solid, units={:?} ({t_sc:.1?})", r.length_units_mm);
            }
            for f in &r.refused {
                println!("  solidcore  refused #{} at {}: {}", f.item, f.stage, f.reason);
            }
            for n in r.notes.iter().chain(r.solids.iter().flat_map(|s| &s.notes)) {
                println!("  solidcore  note: {n}");
            }
        }
    }
    match &tr {
        Err(e) => println!("  truck      Err {e}"),
        Ok(t) => println!(
            "  truck      shells={} V={:.6} A={:.6} box={:?}..{:?} tris={} open={} chord={:.4} mm ({t_tr:.1?})",
            t.shells, t.mesh.volume, t.mesh.area, t.mesh.min, t.mesh.max, t.mesh.triangles,
            t.mesh.open_edges, t.chord_mm
        ),
    }
    match &tess {
        Err(e) => println!("  sc-tess    Err {e}"),
        Ok(m) => println!(
            "  sc-tess    V={:.6} A={:.6} box={:?}..{:?} tris={} open={} left out={:?} over tolerance={} ({t_tess:.1?})",
            m.mesh.volume, m.mesh.area, m.mesh.min, m.mesh.max, m.mesh.triangles,
            m.mesh.open_edges, m.refused_faces, m.over_tolerance
        ),
    }
    let reading = Reading {
        file: file.to_string(),
        import: import_verdict(unit, &sc, &tr),
        mesh: mesh_verdict(&sc, &tr, &tess),
    };
    println!("  verdict    import: {} | mesh: {}", reading.import, reading.mesh);
    reading
}

#[test]
fn step_files_agree_or_refuse_by_name() {
    let files = step_files();
    assert!(!files.is_empty(), "no STEP fixtures under {:?}", models_dir());
    let mut observed = Vec::new();
    for f in &files {
        let unit = ROWS.iter().find(|r| r.file == f).map_or(1.0, |r| r.mm_per_unit);
        let r = read_one(f, unit);
        observed.push((r.file, r.import, r.mesh));
    }
    let pinned: Vec<(String, String, String)> = ROWS
        .iter()
        .map(|r| (r.file.to_string(), r.import.to_string(), r.mesh.to_string()))
        .collect();
    assert_eq!(
        observed, pinned,
        "STEP verdicts changed, or a fixture has no row - re-triage"
    );
}
