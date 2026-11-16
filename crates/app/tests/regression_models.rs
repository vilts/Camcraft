//! The golden-model regression lane (M5.3 G): import a model,
//! generate a real operation over it, verify the program, post it -
//! and JUDGE THE NUMBERS, not the bytes. A byte golden says nothing
//! moved; a table of measured lengths, counts and minutes says
//! exactly how much. Floats ride a tolerance (microns on paths, a
//! billionth on time), counts and lines ride exact equality, and the
//! verify gate must speak clean before any number is trusted. The
//! wavy slab is generated in code (deterministic by construction,
//! f32-quantized through a binary STL so every platform reads the
//! same vertices) and `cube.stl` is the committed file the import
//! door has read since M1.1.
//!
//! Gate map (M4.4): covers app::cli's lane-mates (geometry import,
//! commands, generation, verify, posting) end to end.

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::stock::StockShape;
use camcraft_app::tool_list;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

use std::path::{Path, PathBuf};

fn tmp(tag: &str, ext: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-regress-{tag}-{}.{}", std::process::id(), ext));
    let _ = std::fs::remove_file(&p);
    p
}

fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name)
}

/// Microns: the quantization floor the kernels already agree to
/// (M4.2b), so a passing number means "nothing moved", not "we
/// stopped looking".
const TOL_MM: f64 = 1e-6;
/// A billionth of a minute - the same law for the time sum.
const TOL_MIN: f64 = 1e-9;

/// The one pipeline both lanes share: door in, program out, judged.
struct Measured {
    facets: usize,
    moves: usize,
    cut_mm: f64,
    minutes: f64,
    posted_lines: usize,
}

/// Import, stock it, face it, generate, verify, post: the whole
/// chain in the order a headless job would drive it. Every refusal
/// here is a panic with the stage named - the lane must speak.
fn pipeline(model_path: &Path, op_name: &str, stock: StockShape, cut_z: f64, stepover: f64) -> Measured {
    let world = new_world();
    let mut history = History::default();

    let imp =
        ImportModel::new(model_path).unwrap_or_else(|e| panic!("{op_name}: import door refused: {e}"));
    let model_name = imp.name.clone();
    history.execute(&world, imp);
    let facets = model::find_by_name(&world, &model_name)
        .and_then(|e| model::facets_of(&world, &e))
        .map_or(0, |f| f.len());

    let cmd = SetStock::new(&world, stock, Default::default());
    history.execute(&world, cmd.unwrap_or_else(|e| panic!("{op_name}: stock refused: {e}")));
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0400-2FL")
        .map(|(_, t)| t)
        .expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("catalog tool"));
    let params = OpParams::Face { holes: Default::default(), blend_turns: false,
        tilt: None,
        stepover,
        clearance_z: Height::fixed(cut_z + 6.0),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    let cmd = CreateOperation::new(&world, op_name, "EM-0400-2FL", SrcRef::Stock, params);
    history.execute(&world, cmd.unwrap_or_else(|e| panic!("{op_name}: create refused: {e}")));
    let cmd = GenerateOperation::new(&world, op_name).unwrap_or_else(|e| panic!("{op_name}: generate refused: {e}"));
    history.execute(&world, cmd);

    // The program-level gate speaks first: a number measured over a
    // path the gate would block is not a golden, it is a bug.
    let problems = posting::verification_gate(&world);
    assert!(problems.is_empty(), "{op_name}: the gate would block: {problems:?}");

    let rows = tool_list::program(&world);
    let (cut_mm, minutes) = rows.iter().fold((0.0, 0.0), |a, r| (a.0 + r.cut_mm, a.1 + r.minutes));
    let moves = model::find_by_name(&world, op_name)
        .and_then(|e| model::toolpath_of(&world, &e))
        .map_or(0, |t| t.path.len());

    let nc = tmp(op_name, "nc");
    let posted = posting::post_setup(&world, &nc).unwrap_or_else(|e| panic!("{op_name}: post refused: {e}"));
    let posted_lines = posted.lines;
    let _ = std::fs::remove_file(&nc);

    Measured { facets, moves, cut_mm, minutes, posted_lines }
}

fn close(tag: &str, got: f64, want: f64, tol: f64) {
    assert!((got - want).abs() <= tol, "{tag}: measured {got} vs golden {want} (tol {tol})");
}

/// The wavy slab: a 60x60 grid at 5 mm over a polynomial top z =
/// 8 + (x/30)^2 + (y/30)^2 - corners 10, centre 8 - closed with
/// walls and a flat bottom. Written as BINARY STL so the import
/// door's f32 read is the only quantizer, and the generator, the
/// dexel and the post all see the same vertices on every platform.
fn write_wavy_slab(path: &Path) {
    type Tri = (f64, f64, f64, f64, f64, f64, f64, f64, f64);
    let f = |x: f64, y: f64| 8.0 + (x / 30.0) * (x / 30.0) + (y / 30.0) * (y / 30.0);
    let g = |i: usize| -30.0 + 5.0 * i as f64;
    let mut facets: Vec<Tri> = Vec::new();
    let mut tri = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| {
        facets.push((a.0, a.1, f(a.0, a.1), b.0, b.1, f(b.0, b.1), c.0, c.1, f(c.0, c.1)));
    };
    // Top, CCW seen from +z (outward by the right-hand rule).
    for i in 0..12 {
        for j in 0..12 {
            let (x0, x1) = (g(i), g(i + 1));
            let (y0, y1) = (g(j), g(j + 1));
            tri((x0, y0), (x1, y0), (x1, y1));
            tri((x0, y0), (x1, y1), (x0, y1));
        }
    }
    // Bottom, normal -z: one quad, two triangles, wound the other way.
    let bl = (g(0), g(0));
    let br = (g(12), g(0));
    let tl = (g(0), g(12));
    let tr = (g(12), g(12));
    let mut bot = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| {
        facets.push((a.0, a.1, 0.0, b.0, b.1, 0.0, c.0, c.1, 0.0));
    };
    bot(bl, tl, br);
    bot(tl, tr, br);
    // Walls: the boundary loop CCW seen from +z, outward (dy, -dx).
    let mut loop_edges = Vec::new();
    for i in 0..12 {
        loop_edges.push(((g(i), g(0)), (g(i + 1), g(0))));
    }
    for j in 0..12 {
        loop_edges.push(((g(12), g(j)), (g(12), g(j + 1))));
    }
    for i in 0..12 {
        loop_edges.push(((g(12 - i), g(12)), (g(11 - i), g(12))));
    }
    for j in 0..12 {
        loop_edges.push(((g(0), g(12 - j)), (g(0), g(11 - j))));
    }
    for (a, b) in loop_edges {
        let (ta, tb) = ((a.0, a.1, f(a.0, a.1)), (b.0, b.1, f(b.0, b.1)));
        let (ba, bb) = ((a.0, a.1, 0.0), (b.0, b.1, 0.0));
        facets.push((ta.0, ta.1, ta.2, ba.0, ba.1, ba.2, bb.0, bb.1, bb.2));
        facets.push((ta.0, ta.1, ta.2, bb.0, bb.1, bb.2, tb.0, tb.1, tb.2));
    }

    let mut bytes = vec![0u8; 84];
    bytes[..6].copy_from_slice(b"CAMCRF");
    bytes[80..84].copy_from_slice(&(facets.len() as u32).to_le_bytes());
    for (ax, ay, az, bx, by, bz, cx, cy, cz) in facets {
        let (ax, ay, az, bx, by, bz, cx, cy, cz) =
            (ax as f32, ay as f32, az as f32, bx as f32, by as f32, bz as f32, cx as f32, cy as f32, cz as f32);
        // The stored normal is recomputed from the winding by the
        // importer (E1's lying-header law); the zeros say so here.
        bytes.extend_from_slice(&[0u8; 12]);
        for v in [(ax, ay, az), (bx, by, bz), (cx, cy, cz)] {
            bytes.extend_from_slice(&v.0.to_le_bytes());
            bytes.extend_from_slice(&v.1.to_le_bytes());
            bytes.extend_from_slice(&v.2.to_le_bytes());
        }
        bytes.extend_from_slice(&0u16.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("the slab file lands");
}

#[test]
fn wavy_slab_faces_to_a_measured_program() {
    // Goldens measured 2026-10-30 on the M5.3 G tree; the law is the
    // tolerance, the numbers are the witness. Facets and lines are
    // exact (integers by construction); length and time ride the
    // micron floor.
    let slab = tmp("slab", "stl");
    write_wavy_slab(&slab);
    // The face takes 0.5 mm off the BOX stock down to the model's
    // highest point: the slab (top 8..10) is the part the gate judges
    // against, so the plane must land at 10.0 - below it the program
    // would carve the part itself, which is exactly what a gouge is.
    let m = pipeline(
        &slab,
        "Face1",
        StockShape::Box { dx: 60.0, dy: 60.0, dz: 10.5 },
        10.0,
        3.0,
    );
    assert_eq!(m.facets, 386, "top 288 + walls 96 + bottom 2");
    assert_eq!(m.moves, 42, "path moves, the approach rapid (#690) included");
    close("cut_mm", m.cut_mm, 1342.0, TOL_MM);
    close("minutes", m.minutes, 1.474753430541823, TOL_MIN); // + the approach rapid (#690)
    assert_eq!(m.posted_lines, 59, "line count is exact (the operation and tool comments of #257, the approach of #690, the G21 of #901 and the first traverse's stated height of #1633 included)");
}

#[test]
fn cube_stl_faces_to_a_measured_program() {
    // The committed file, the same door M1.1's acceptance reads: 12
    // facets exactly, a 2x2 box faced to the cube's own top (z 1).
    let m = pipeline(
        &testdata("cube.stl"),
        "Face2",
        StockShape::Box { dx: 2.0, dy: 2.0, dz: 1.5 },
        1.0,
        0.8,
    );
    assert_eq!(m.facets, 12, "the unit cube's shell");
    assert_eq!(m.moves, 4, "path moves, the approach rapid (#690) included");
    close("cut_mm", m.cut_mm, 12.0, TOL_MM);
    close("minutes", m.minutes, 0.35983333333333334, TOL_MIN); // + the approach rapid (#690)
    assert_eq!(m.posted_lines, 21, "line count is exact (the operation and tool comments of #257, the approach of #690, the G21 of #901 and the first traverse's stated height of #1633 included)");
}
