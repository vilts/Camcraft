//! The machining-mesh deviation bench (M12.1, #940), kernel release lane:
//! every STEP fixture under testdata/models meshed for machining at the
//! app's chord caps (0.005 mm, #689; 0.01 mm, the metre file's floor in
//! #864), with its triangle count, time and measured deviation, beside
//! truck's mesh of the same file at the app's own target. The verdicts are
//! pinned; the numbers are printed for the decision file.
//!
//! Both meshes are also judged by one common, mesher-blind measure: each
//! triangle's 15 sample points against the NEAREST untrimmed analytic
//! surface of the record's faces (a lower bound of the distance to the
//! true face; "n/a" where the record has a spline or revolved face).
//! Provenance: our own bench design; fixtures are the repo's own; truck's
//! calls are the app's (crates/app/src/geometry/import/step.rs).
//!
//! Gate map (M12.1): covers camcraft_kernel::machining_mesh (heavy lane).

mod step_util;

use std::path::PathBuf;
use std::time::Instant;

use camcraft_kernel::P3;
use camcraft_kernel::machining_mesh::machining_mesh;
use camcraft_kernel::mesh_deviation::analytic_distance;
use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_step::read_step;
use solidcore::kernel::CancelToken;
use solidcore_record::brep;
use step_util::{CASE_TIMEOUT, TRUCK_MIN_TOL, bounded_cancel};
use truck_meshalgo::prelude::*;
use truck_stepio::r#in::Table;

const TOLS: [f64; 2] = [0.005, 0.01];

/// (file, mm per unit, pinned verdict at each of [`TOLS`]).
const ROWS: &[(&str, f64, [&str; 2])] = &[
    ("Milling-test-inch.step", 25.4, ["within", "within"]),
    ("Milling-test.step", 1.0, ["within", "within"]),
    ("box.step", 1.0, ["within", "within"]),
    ("empty_list.step", 1.0, ["within", "within"]),
    ("sample_ap214.step", 1000.0, ["within", "within"]),
    ("tool_csys_box_iso13399.step", 1.0, ["within", "within"]),
    ("tool_csys_degenerate.step", 1.0, ["no solid", "no solid"]),
    ("tool_csys_iso13399.step", 1.0, ["no solid", "no solid"]),
    ("tool_csys_none.step", 1.0, ["no solid", "no solid"]),
];

fn models() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models")
}

/// Barycentric 15-point grid (4 divisions a side).
fn grid(a: P3, b: P3, c: P3) -> impl Iterator<Item = P3> {
    (0..=4).flat_map(move |i| {
        (0..=(4 - i)).map(move |j| {
            let (u, v) = (i as f64 / 4.0, j as f64 / 4.0);
            let w = 1.0 - u - v;
            [0, 1, 2].map(|k| u * a[k] + v * b[k] + w * c[k])
        })
    })
}

/// The common measure: largest over samples of the distance to the nearest
/// analytic face surface. `None` if any face is not analytic.
fn nearest_analytic(records: &[&brep::Brep], pos: &[P3], tris: &[[usize; 3]]) -> Option<f64> {
    let surfaces: Vec<&brep::Surface> = records
        .iter()
        .flat_map(|r| r.faces.iter().map(|f| &f.surface))
        .collect();
    if surfaces
        .iter()
        .any(|s| analytic_distance(s, [0.0; 3]).is_none())
    {
        return None;
    }
    let mut worst = 0.0_f64;
    for t in tris {
        let (a, b, c) = (*pos.get(t[0])?, *pos.get(t[1])?, *pos.get(t[2])?);
        for p in grid(a, b, c) {
            let d = surfaces
                .iter()
                .filter_map(|s| analytic_distance(s, p))
                .fold(f64::INFINITY, f64::min);
            worst = worst.max(d);
        }
    }
    Some(worst)
}

/// truck's mesh of the file at the app's target for chord cap `cap` mm,
/// scaled to mm: (positions, triangles, chord used in mm).
/// A triangle soup and the chord it was made at (mm).
type Soup = (Vec<P3>, Vec<[usize; 3]>, f64);

fn truck_soup(bytes: &[u8], k: f64, cap: f64) -> Result<Soup, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "not UTF-8".to_string())?;
    let ex = ruststep::parser::parse(text).map_err(|e| format!("syntax: {e}"))?;
    let table = Table::from_data_section(ex.data.first().ok_or("no DATA")?);
    let mut ids: Vec<&u64> = table.shell.keys().collect();
    ids.sort_unstable();
    let (mut pos, mut tris, mut chord) = (Vec::new(), Vec::new(), 0.0_f64);
    for id in ids {
        let Some(shell) = table.shell.get(id) else {
            continue;
        };
        let cs = table
            .to_compressed_shell(shell)
            .map_err(|e| e.to_string())?;
        let extent = cs
            .vertices
            .iter()
            .flat_map(|p| [p.x.abs(), p.y.abs(), p.z.abs()])
            .fold(0.0_f64, f64::max);
        let tol = (extent * 1e-3).min(cap / k).max(TRUCK_MIN_TOL);
        chord = chord.max(tol * k);
        let m = cs.robust_triangulation(tol).to_polygon();
        let base = pos.len();
        pos.extend(m.positions().iter().map(|p| [p.x * k, p.y * k, p.z * k]));
        for f in m.face_iter() {
            let Some(a) = f.first() else { continue };
            for w in f.get(1..).unwrap_or_default().windows(2) {
                if let [b, c] = w {
                    tris.push([base + a.pos, base + b.pos, base + c.pos]);
                }
            }
        }
    }
    if tris.is_empty() {
        return Err("no triangles".into());
    }
    Ok((pos, tris, chord))
}

fn fmt(d: Option<f64>) -> String {
    d.map_or("n/a".into(), |d| format!("{d:.2e}"))
}

#[test]
#[ignore = "kernel release lane: meshes every STEP fixture twice per tolerance (cargo test --release -p camcraft-kernel -- --ignored)"]
fn machining_mesh_deviation_bench() {
    let mut observed = Vec::new();
    for &(file, unit, _) in ROWS {
        let bytes = std::fs::read(models().join(file)).unwrap_or_default();
        assert!(!bytes.is_empty(), "{file}: fixture missing");
        let b = bytes.clone();
        let read = bounded_cancel(file, CASE_TIMEOUT, move |stop| {
            read_step(&mut SolidcoreKernel::default(), &b, stop)
        });
        let solids = read.map(|r| r.solids).unwrap_or_default();
        let mut verdicts = [String::new(), String::new()];
        for (slot, tol) in verdicts.iter_mut().zip(TOLS) {
            if solids.is_empty() {
                *slot = "no solid".into();
                println!("{file} @ {tol}: no solid");
                continue;
            }
            let t0 = Instant::now();
            let mut tris = 0usize;
            let (mut par, mut ana, mut gate, mut mine) = (0.0_f64, 0.0_f64, 0.0_f64, Some(0.0_f64));
            let mut verdict = "within".to_string();
            let records: Vec<&brep::Brep> = solids.iter().map(|s| &s.record).collect();
            for s in &solids {
                match machining_mesh(s, tol, &CancelToken::new()) {
                    Ok(m) => {
                        tris += m.triangles.len();
                        par = par.max(m.report.deviation.parametric);
                        ana = ana.max(m.report.deviation.analytic);
                        gate = gate.max(m.report.deviation.gated);
                        let t: Vec<[usize; 3]> =
                            m.triangles.iter().map(|t| t.map(|i| i as usize)).collect();
                        mine = mine
                            .zip(nearest_analytic(&records, &m.positions, &t))
                            .map(|(a, b)| a.max(b));
                        for n in &m.report.notes {
                            println!("  note: {n}");
                        }
                    }
                    Err(e) => verdict = format!("refused: {e}"),
                }
            }
            let t_sc = t0.elapsed();
            let t0 = Instant::now();
            let truck = truck_soup(&bytes, unit, tol);
            let t_tr = t0.elapsed();
            let truck_line = match &truck {
                Ok((p, t, chord)) => format!(
                    "truck: tris={} chord={chord:.4} common={} ({t_tr:.1?})",
                    t.len(),
                    fmt(nearest_analytic(&records, p, t))
                ),
                Err(e) => format!("truck: {e}"),
            };
            println!(
                "{file} @ {tol} mm: sc-tess tris={tris} judged={gate:.2e} parametric={par:.2e} analytic={ana:.2e} common={} ({t_sc:.1?}) | {truck_line} | {verdict}",
                fmt(mine)
            );
            if verdict == "within" {
                assert!(gate <= tol, "{file}: judged {gate} past {tol}");
            }
            *slot = verdict;
        }
        observed.push((file.to_string(), verdicts));
    }
    let pinned: Vec<(String, [String; 2])> = ROWS
        .iter()
        .map(|(f, _, v)| (f.to_string(), v.map(String::from)))
        .collect();
    assert_eq!(
        observed, pinned,
        "machining-mesh verdicts changed - re-triage"
    );
}
