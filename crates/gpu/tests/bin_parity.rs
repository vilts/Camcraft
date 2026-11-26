//! Parity of the conservative binning (#2620) with the f64 support facet
//! grid of #2619, read from the fixture that camcraft-motion records
//! (`fixtures/facet_grid_2619.grid`). The law: each facet that the f64
//! grid puts in a cell is in that cell's GPU (or twin) list too.
//! The CPU twin always runs; the GPU path runs when the machine has an
//! adapter with compute, and the test prints which path ran.
//! Gate map: covers camcraft_gpu::binning, camcraft_gpu::binning_gpu,
//! camcraft_gpu::shared. Our own tests.

use camcraft_gpu::{
    bin_facets, bin_on, bin_twin, gpu_required, pad_boxes, shared_device, CellLists, GridSpec,
};

const FIXTURE: &str = include_str!("fixtures/facet_grid_2619.grid");

struct Fixture {
    spec: GridSpec,
    boxes: Vec<[f64; 4]>,
    cells: Vec<Vec<u32>>,
}

fn nums<T: std::str::FromStr>(it: std::str::SplitWhitespace<'_>, line: &str) -> Vec<T> {
    it.map(|t| t.parse().unwrap_or_else(|_| panic!("bad number in fixture line: {line}"))).collect()
}

fn fixture() -> Fixture {
    let mut spec = None;
    let mut boxes = Vec::new();
    let mut cells = Vec::new();
    for line in FIXTURE.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("grid") => {
                let v: Vec<f64> = nums(it, line);
                assert_eq!(v.len(), 5, "{line}");
                let (nx, ny) = (v[3] as u32, v[4] as u32);
                spec = Some(GridSpec { x0: v[0], y0: v[1], s: v[2], nx, ny });
                cells = vec![Vec::new(); (nx * ny) as usize];
            }
            Some("box") => {
                let v: Vec<f64> = nums(it, line);
                boxes.push([v[0], v[1], v[2], v[3]]);
            }
            Some("cell") => {
                let v: Vec<u32> = nums(it, line);
                cells[v[0] as usize] = v[1..].to_vec();
            }
            _ => panic!("unknown fixture line: {line}"),
        }
    }
    Fixture { spec: spec.expect("the fixture has a grid line"), boxes, cells }
}

/// Each f64 entry is in the binned list; the lists ascend with no repeat.
fn assert_superset(f: &Fixture, got: &CellLists, path: &str) -> (usize, usize) {
    assert_eq!(got.cells(), f.cells.len(), "{path}: cell count");
    let (mut want, mut have) = (0, 0);
    for (c, cpu) in f.cells.iter().enumerate() {
        let g = got.cell(c);
        assert!(g.windows(2).all(|w| w[0] < w[1]), "{path}: cell {c} not strictly ascending: {g:?}");
        for i in cpu {
            assert!(g.binary_search(i).is_ok(), "{path}: facet {i} missing from cell {c} (f64 {cpu:?}, got {g:?})");
        }
        want += cpu.len();
        have += g.len();
    }
    (want, have)
}

/// Boxes with edges inside cells (the fixture has every edge on a cell
/// border) on a far origin, with the f64 cells of the #2619 law: floor of
/// `(v - origin) / s`, clamped to the grid. A cell map that shifts by one
/// cell stays a superset on border edges, but not here.
fn off_border() -> Fixture {
    let spec = GridSpec { x0: 1234.567, y0: -987.654, s: 0.37, nx: 23, ny: 17 };
    let cell = |v: f64, o: f64, n: u32| ((v - o) / spec.s).floor().clamp(0.0, f64::from(n - 1)) as usize;
    let mut seed = 0x2620_u64;
    let mut next = || {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    let (w, h) = (spec.s * f64::from(spec.nx), spec.s * f64::from(spec.ny));
    let mut boxes = Vec::new();
    let mut cells = vec![Vec::new(); (spec.nx * spec.ny) as usize];
    for i in 0..200u32 {
        let (x, y) = (spec.x0 + next() * w, spec.y0 + next() * h);
        let b = [x, y, x + next() * 1.5, y + next() * 1.5];
        for cy in cell(b[1], spec.y0, spec.ny)..=cell(b[3], spec.y0, spec.ny) {
            for cx in cell(b[0], spec.x0, spec.nx)..=cell(b[2], spec.x0, spec.nx) {
                cells[cy * spec.nx as usize + cx].push(i);
            }
        }
        boxes.push(b);
    }
    Fixture { spec, boxes, cells }
}

#[test]
fn twin_and_gpu_bins_hold_the_f64_cells_of_off_border_boxes() {
    let f = off_border();
    let padded = pad_boxes(&f.boxes, &f.spec).expect("the boxes pad");
    assert_superset(&f, &bin_twin(&padded).expect("twin bins"), "CPU twin (off border)");
    match shared_device() {
        Ok(gpu) => {
            let got = bin_on(gpu, &padded).unwrap_or_else(|e| panic!("GPU binning on {}: {e}", gpu.name));
            assert_superset(&f, &got, "GPU (off border)");
        }
        Err(why) => {
            assert!(!gpu_required() || why.contains("off"), "CAMCRAFT_REQUIRE_GPU=1 but {why}");
            eprintln!("*** bin_parity: GPU path SKIPPED (off border) - NOT a pass: {why} ***");
        }
    }
}

#[test]
fn twin_bins_hold_every_f64_grid_entry() {
    let f = fixture();
    assert!(f.boxes.len() > 32 && f.cells.iter().any(|c| c.len() > 1), "the fixture is not trivial");
    let padded = pad_boxes(&f.boxes, &f.spec).expect("the fixture pads");
    let (want, have) = assert_superset(&f, &bin_twin(&padded).expect("twin bins"), "CPU twin");
    eprintln!("bin_parity: CPU twin path ran ({want} f64 entries, {have} binned)");
}

#[test]
fn gpu_bins_hold_every_f64_grid_entry_when_an_adapter_exists() {
    let f = fixture();
    let padded = pad_boxes(&f.boxes, &f.spec).expect("the fixture pads");
    let gpu = match shared_device() {
        Ok(gpu) => gpu,
        Err(why) => {
            assert!(!gpu_required() || why.contains("off"), "CAMCRAFT_REQUIRE_GPU=1 but {why}");
            assert!(bin_facets(&f.boxes, &f.spec).is_err(), "no device: bin_facets must fall back");
            eprintln!("*** bin_parity: GPU path SKIPPED - NOT a pass: {why} ***");
            return;
        }
    };
    let got = bin_on(gpu, &padded).unwrap_or_else(|e| panic!("GPU binning on {}: {e}", gpu.name));
    let (want, have) = assert_superset(&f, &got, "GPU");
    let via = bin_facets(&f.boxes, &f.spec).unwrap_or_else(|e| panic!("bin_facets on {}: {e}", gpu.name));
    assert_eq!(via, got, "bin_facets and bin_on agree on one device");
    eprintln!("bin_parity: GPU path ran on the {} adapter {} ({want} f64 entries, {have} binned)", gpu.kind.label(), gpu.name);
}
