//! Dual-importer STEP bench (M12.1 stage 4, #916): every STEP file under
//! testdata/models through the solidcore-side importer, judged against the
//! RECORDED reading of truck-stepio on the same file (rows.rs; #2790 took
//! the truck reader out of the workspace, so its side is a recording now).
//! Per file both import, or one refuses by name; where both import, solid
//! count, bounding box, volume, area and the length unit are compared
//! within the stated tolerances (verdict.rs). The verdicts are pinned: a
//! disagreement is recorded, never averaged away, and a file with no row
//! fails the bench.
//! Provenance: our own bench design; the fixtures are the repo's own.
//!
//! Gate map (M12.1): covers camcraft_kernel::solidcore_step (read_step,
//! import_step).

#[path = "step_agreement/rows.rs"]
mod rows;
mod step_util;
#[path = "step_agreement/verdict.rs"]
mod verdict;

use std::path::PathBuf;
use std::time::Instant;

use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_step::read_step;
use rows::{ROWS, Truck};
use step_util::{CASE_TIMEOUT, CHORD_MM, bounded_cancel, solidcore_mesh};
use verdict::{import_verdict, mesh_verdict, whole};

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

struct Reading {
    file: String,
    import: String,
    mesh: String,
}

fn read_one(file: &str, unit: f64, truck: &Truck) -> Reading {
    let bytes = std::fs::read(models_dir().join(file)).unwrap_or_default();
    assert!(!bytes.is_empty(), "{file}: fixture missing or empty");
    let b = bytes.clone();
    let t0 = Instant::now();
    let sc = bounded_cancel(file, CASE_TIMEOUT, move |stop| {
        let mut k = SolidcoreKernel::default();
        read_step(&mut k, &b, stop)
    });
    let t_sc = t0.elapsed();
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
    match truck {
        Truck::Refused(e) => println!("  truck      (recorded) Err {e}"),
        Truck::NotRecorded => println!("  truck      not recorded (fixture added after #2790)"),
        Truck::Read {
            shells,
            volume,
            area,
            min,
            max,
            open_edges,
            chord_mm,
        } => println!(
            "  truck      (recorded) shells={shells} V={volume:.6} A={area:.6} box={min:?}..{max:?} open={open_edges} chord={chord_mm:.4} mm"
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
        import: import_verdict(unit, &sc, truck),
        mesh: mesh_verdict(&sc, truck, &tess),
    };
    println!("  verdict    import: {} | mesh: {}", reading.import, reading.mesh);
    reading
}

/// One file's reading against its pinned row.
fn check_row(file: &str) {
    let row = ROWS.iter().find(|r| r.file == file);
    assert!(row.is_some(), "{file}: no pinned row - re-triage");
    let Some(row) = row else { return };
    let r = read_one(file, row.mm_per_unit, &row.truck);
    assert_eq!(r.file, file);
    assert_eq!(
        (r.import.as_str(), r.mesh.as_str()),
        (row.import, row.mesh),
        "{file}: STEP verdicts changed - re-triage"
    );
}

// #1715: the bench was one test, a serial loop over every file (90 s in
// debug, v0.20.0 cut). Each file is its own test now, so libtest reads
// them in parallel; the file-set test below keeps "a file with no row
// fails the bench" and keeps this list in step with ROWS.
macro_rules! per_file {
    ($($name:ident => $file:literal),* $(,)?) => {
        /// The files that have a test of their own, in ROWS order.
        const TESTED: &[&str] = &[$($file),*];
        $(
            #[test]
            fn $name() {
                check_row($file);
            }
        )*
    };
}

per_file! {
    step_milling_test_inch => "Milling-test-inch.step",
    step_milling_test => "Milling-test.step",
    step_bad_entity_mid_data => "bad_entity_mid_data.step",
    step_bar_cross_holes => "bar_cross_holes.step",
    step_box => "box.step",
    step_bspline_discontinuous => "bspline_discontinuous.step",
    step_bspline_dome => "bspline_dome.step",
    step_empty_list => "empty_list.step",
    step_fig8 => "fig8.step",
    step_fillet_corner => "fillet_corner.step",
    step_hole_on_pocket_wall => "hole_on_pocket_wall.step",
    step_lathe_holder_offset => "lathe_holder_offset.step",
    step_modular_clamp_unit => "modular_clamp_unit.step",
    step_modular_lathe_head => "modular_lathe_head.step",
    step_q02_3d_mold => "q02_3d_mold.step",
    step_sample_ap214 => "sample_ap214.step",
    step_short_units_degree_cone => "short_units_degree_cone.step",
    step_short_units_inch => "short_units_inch.step",
    step_short_units_metre => "short_units_metre.step",
    step_sphere_quarter_pole => "sphere_quarter_pole.step",
    step_spline_revolved => "spline_revolved.step",
    step_tool_csys_box_iso13399 => "tool_csys_box_iso13399.step",
    step_tool_csys_degenerate => "tool_csys_degenerate.step",
    step_tool_csys_iso13399 => "tool_csys_iso13399.step",
    step_tool_csys_none => "tool_csys_none.step",
    step_twisted_flank => "twisted_flank.step",
    step_twisted_flank_off => "twisted_flank_off.step",
}

/// Every STEP fixture has exactly one pinned row, and every row has a
/// test of its own: a new file with no row, or a row with no test,
/// fails the bench.
#[test]
fn step_files_agree_or_refuse_by_name() {
    let files = step_files();
    assert!(!files.is_empty(), "no STEP fixtures under {:?}", models_dir());
    let pinned: Vec<&str> = ROWS.iter().map(|r| r.file).collect();
    assert_eq!(files, pinned, "a fixture has no row, or a row has no fixture - re-triage");
    assert_eq!(TESTED, pinned.as_slice(), "every row needs its own test in per_file!");
}

/// The recorded truck side (#2790) keeps its shape and the findings that
/// the live truck reader gave before it left the workspace: #1697's
/// runaway face refused by name while the smooth dome meshed, the open
/// meshes of the free-form files (so their volume is short), and the
/// zero volume of the twisted flank (#2780). Every recorded mesh is finite
/// and made at a chord the app's target permits. This test took the place
/// of step_import.rs's live `truck_side_refuses_a_runaway_face_and_obeys_its_cancel`.
#[test]
fn recorded_truck_readings_keep_their_findings() {
    let truck = |file: &str| ROWS.iter().find(|r| r.file == file).map(|r| &r.truck);
    match truck("bspline_discontinuous.step") {
        Some(Truck::Refused(e)) => {
            assert!(e.contains("B-spline surface) not meshed: its subdivision did not converge"), "{e}");
        }
        _ => panic!("bspline_discontinuous.step: the runaway face is not a recorded refusal"),
    }
    for file in ["short_units_inch.step", "short_units_metre.step"] {
        assert!(
            matches!(truck(file), Some(Truck::Refused("a shell's mesh is not finite"))),
            "{file}: the recorded short-name refusal changed"
        );
    }
    assert!(
        matches!(truck("bspline_dome.step"), Some(Truck::Read { volume, .. }) if *volume > 0.0),
        "bspline_dome.step: the smooth B-spline is not a recorded mesh"
    );
    for (file, open) in [("fillet_corner.step", 121), ("bar_cross_holes.step", 1090)] {
        assert!(
            matches!(truck(file), Some(Truck::Read { open_edges, .. }) if *open_edges == open),
            "{file}: the recorded open mesh changed"
        );
    }
    assert!(
        matches!(truck("twisted_flank.step"), Some(Truck::Read { volume, .. }) if *volume == 0.0),
        "twisted_flank.step: the recorded open shell changed"
    );
    for row in ROWS {
        match &row.truck {
            Truck::Refused(e) => assert!(!e.is_empty(), "{}: a refusal with no reason", row.file),
            Truck::NotRecorded => assert!(
                row.import.starts_with("no truck reading") || row.import.starts_with("solidcore refuses"),
                "{}: a fixture with no truck reading has a truck verdict",
                row.file
            ),
            Truck::Read {
                shells,
                volume,
                area,
                min,
                max,
                chord_mm,
                ..
            } => {
                let finite = [*volume, *area, *chord_mm].iter().chain(min).chain(max).all(|v| v.is_finite());
                assert!(finite && *shells > 0, "{}: a recorded mesh that is not finite", row.file);
                assert!((0..3).all(|i| min[i] <= max[i]), "{}: a recorded box inside out", row.file);
                assert!(
                    *chord_mm > 0.0 && *chord_mm <= 2.0 * CHORD_MM,
                    "{}: a recorded chord of {chord_mm} mm",
                    row.file
                );
            }
        }
        assert_eq!(
            matches!(row.truck, Truck::Refused(_)),
            row.import.starts_with("both refuse") || row.import.starts_with("truck refuses"),
            "{}: the recorded truck side and the pinned verdict disagree",
            row.file
        );
    }
}
