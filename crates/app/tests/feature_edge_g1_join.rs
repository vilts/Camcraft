//! #2209: a pickable edge is always a full feature edge. An edge between
//! two smooth faces that the tessellation cut into chord pieces (the edge
//! below a chamfer on a swept wall) chains as ONE edge, the same as the
//! edge above the chamfer; a real corner still splits.
//!
//! Gate map (#2209): covers geometry::feature::chains (the G1 seam rule in
//! `feature/g1_seam.rs`) through app::pick::feature_graph and
//! app::geometry::load_model, over the STEP fixtures of `testdata/models`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use camcraft_app::geometry::{Point3, Triangle, feature, load_model};
use camcraft_app::pick::feature_graph::{FeatureGraph, feature_graph};

/// The angle limit of the join: two pickable edges that meet at a vertex
/// of no other pickable edge, and turn by no more than this, are one edge.
const LIMIT_DEG: f64 = feature::CHAIN_TANGENT_DEG;

fn models() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models")
}

fn graph_of_file(name: &str) -> Option<FeatureGraph> {
    let mesh = load_model(&models().join(name)).ok()?;
    Some(feature_graph(&mesh.facets))
}

fn len_of(f: &feature::Features, c: &feature::Chain) -> f64 {
    let p = feature::chain_points(f, c);
    let d = |a: [f64; 3], b: [f64; 3]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    let mut l: f64 = p.windows(2).map(|w| d(w[0], w[1])).sum();
    if c.closed && p.len() > 2 {
        l += d(p[0], p[p.len() - 1]);
    }
    l
}

/// Every pair of pickable edges that meet at a vertex where no third
/// pickable edge meets, and that turn by at most [`LIMIT_DEG`] there,
/// which does NOT lie in one chain. Each such pair is a piece of a longer
/// edge that the pick would show on its own.
fn split_pieces(g: &FeatureGraph) -> Vec<(u32, u32)> {
    let f = &*g.features;
    let chain_of: HashMap<u32, usize> =
        g.chains().iter().enumerate().flat_map(|(k, c)| c.edges.iter().map(move |&e| (e, k))).collect();
    let mut at: HashMap<u32, Vec<u32>> = HashMap::new();
    for &ei in &g.edges {
        let e = f.edges[ei as usize];
        if e.a != e.b {
            at.entry(e.a).or_default().push(ei);
            at.entry(e.b).or_default().push(ei);
        }
    }
    let cos_max = LIMIT_DEG.to_radians().cos();
    let mut out = Vec::new();
    for (&v, es) in &at {
        let [x, y] = es[..] else { continue };
        let (ex, ey) = (f.edges[x as usize], f.edges[y as usize]);
        let far = |e: feature::TopEdge| if e.a == v { e.b } else { e.a };
        let (p, a, b) = (f.verts[v as usize], f.verts[far(ex) as usize], f.verts[far(ey) as usize]);
        let (u, w) = ([p[0] - a[0], p[1] - a[1], p[2] - a[2]], [b[0] - p[0], b[1] - p[1], b[2] - p[2]]);
        let m = ((u[0] * u[0] + u[1] * u[1] + u[2] * u[2]) * (w[0] * w[0] + w[1] * w[1] + w[2] * w[2])).sqrt();
        let c = (u[0] * w[0] + u[1] * w[1] + u[2] * w[2]) / m;
        if m > 0.0 && c >= cos_max && chain_of.get(&x) != chain_of.get(&y) {
            out.push((x, y));
        }
    }
    out
}

#[test]
fn no_pickable_edge_is_a_tangent_piece_of_a_longer_one_in_any_step_fixture() {
    let mut names: Vec<String> = std::fs::read_dir(models())
        .expect("testdata/models")
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.to_ascii_lowercase().ends_with(".step") || n.to_ascii_lowercase().ends_with(".stp"))
        .collect();
    names.sort();
    assert!(names.iter().any(|n| n == "fig8.step"), "fig8.step is one of the fixtures: {names:?}");
    let mut checked = 0;
    let mut bad: Vec<String> = Vec::new();
    for n in &names {
        // Fixtures that refuse to import (corrupt-input pins) have no edges.
        let Some(g) = graph_of_file(n) else { continue };
        checked += 1;
        let pieces = split_pieces(&g);
        println!("{n}: {} pickable edges, {} chains, {} split pieces", g.edges.len(), g.chains().len(), pieces.len());
        if !pieces.is_empty() {
            bad.push(format!("{n}: {} pairs, first {:?}", pieces.len(), pieces[0]));
        }
    }
    assert!(checked >= 5, "most fixtures import: {checked}");
    assert!(bad.is_empty(), "tangent pieces of one edge pick apart: {bad:#?}");
}

/// Pickable edges of fig8 whose two ends sit at height `z`.
fn rim_at(g: &FeatureGraph, z: f64) -> Vec<u32> {
    let f = &*g.features;
    g.edges
        .iter()
        .copied()
        .filter(|&ei| {
            let e = f.edges[ei as usize];
            (f.verts[e.a as usize][2] - z).abs() < 1e-3 && (f.verts[e.b as usize][2] - z).abs() < 1e-3
        })
        .collect()
}

#[test]
fn fig8_edge_below_the_chamfer_picks_whole_like_the_edge_above() {
    let g = graph_of_file("fig8.step").expect("fig8 imports");
    let f = &*g.features;
    // The heights of the horizontal pickable edges: the top rim (z 20),
    // the chamfer's lower rim, the bottom rim (z 0).
    let mut zs: Vec<i64> = g
        .edges
        .iter()
        .filter_map(|&ei| {
            let e = f.edges[ei as usize];
            let (za, zb) = (f.verts[e.a as usize][2], f.verts[e.b as usize][2]);
            ((za - zb).abs() < 1e-3).then_some((za * 1000.0).round() as i64)
        })
        .collect();
    zs.sort_unstable();
    zs.dedup();
    println!("rim heights (um): {zs:?}, pickable edges {}", g.edges.len());
    let below = zs.iter().copied().filter(|&z| z > 1 && z < 19_999).max().expect("a chamfer rim under the top") as f64 / 1000.0;
    let (top, low) = (rim_at(&g, 20.0), rim_at(&g, below));
    // What a plain Edge pick takes: the run that holds the picked edge.
    let runs = g.runs();
    let run_of = |e: u32| runs.iter().find(|r| r.edges.contains(&e));
    let runs_over = |edges: &[u32]| {
        let mut ids: Vec<u64> = edges.iter().filter_map(|&e| run_of(e).map(|r| r.id)).collect();
        ids.sort_unstable();
        ids.dedup();
        ids.len()
    };
    let longest = |edges: &[u32]| edges.iter().filter_map(|&e| run_of(e)).map(|r| len_of(f, r)).fold(0.0, f64::max);
    println!(
        "top rim: {} edges in {} runs, longest {:.2} mm; rim below the chamfer (z {below}): {} edges in {} runs, longest {:.2} mm",
        top.len(),
        runs_over(&top),
        longest(&top),
        low.len(),
        runs_over(&low),
        longest(&low)
    );
    assert!(low.len() > 100, "the rim below the chamfer is finely tessellated: {}", low.len());
    assert_eq!(runs_over(&low), runs_over(&top), "the rim below the chamfer picks in as many pieces as the rim above");
    assert!(longest(&low) > 100.0, "one pick takes a full edge, not a chord: {:.2}", longest(&low));
}

fn tri(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Triangle {
    let p = |v: [f64; 3]| Point3::new(v[0], v[1], v[2]);
    Triangle::new(p(a), p(b), p(c))
}

/// A two-step band: a strip of "chamfer" facets on top of a strip of
/// "wall" facets, each strip `n` quads along a polyline in XY. The wall is
/// vertical, the chamfer leans 45 degrees, so the line between them is a
/// crease cut into `n` pieces with no face in common on either side.
fn band(path: &[[f64; 2]]) -> Vec<Triangle> {
    // Outward unit normal of each segment (the path runs counter-clockwise).
    let seg_n: Vec<[f64; 2]> = path
        .windows(2)
        .map(|w| {
            let (dx, dy) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
            let l = (dx * dx + dy * dy).sqrt();
            [dy / l, -dx / l]
        })
        .collect();
    // The chamfer's top line: each path vertex moved 1 mm inward along the
    // mean normal of its segments, so neighbouring chamfer facets share it.
    let top: Vec<[f64; 3]> = (0..path.len())
        .map(|i| {
            let (a, b) = (seg_n[i.saturating_sub(1).min(seg_n.len() - 1)], seg_n[i.min(seg_n.len() - 1)]);
            let (mx, my) = (a[0] + b[0], a[1] + b[1]);
            let l = (mx * mx + my * my).sqrt();
            let k = if l > 0.0 { 2.0 / (l * l) } else { 0.0 }; // miter: 1 mm from each leg
            [path[i][0] - mx * k, path[i][1] - my * k, 6.0]
        })
        .collect();
    let mut out = Vec::new();
    for i in 0..path.len() - 1 {
        let (p0, p1) = (path[i], path[i + 1]);
        let (a0, a1) = ([p0[0], p0[1], 0.0], [p1[0], p1[1], 0.0]);
        let (b0, b1) = ([p0[0], p0[1], 5.0], [p1[0], p1[1], 5.0]);
        let (c0, c1) = (top[i], top[i + 1]);
        out.push(tri(a0, a1, b1));
        out.push(tri(a0, b1, b0));
        out.push(tri(b0, b1, c1));
        out.push(tri(b0, c1, c0));
    }
    out
}

/// The rim pieces between the wall and the chamfer, z = 5.
fn mid_rim_chains(tris: &[Triangle]) -> Vec<Vec<u32>> {
    let g = feature_graph(tris);
    let rim = rim_at(&g, 5.0);
    let f = &*g.features;
    let mut out: Vec<Vec<u32>> = feature::chains(f)
        .into_iter()
        .map(|c| c.edges.into_iter().filter(|e| rim.contains(e)).collect::<Vec<u32>>())
        .filter(|c| !c.is_empty())
        .collect();
    out.sort();
    out
}

#[test]
fn a_smooth_crease_cut_into_chords_chains_as_one_edge() {
    // A quarter circle of radius 20 in 12 chords (7.5 degrees each).
    let path: Vec<[f64; 2]> = (0..=12)
        .map(|i| {
            let t = (i as f64) * std::f64::consts::FRAC_PI_2 / 12.0;
            [20.0 * t.cos(), 20.0 * t.sin()]
        })
        .collect();
    let chains = mid_rim_chains(&band(&path));
    assert_eq!(chains.len(), 1, "one edge, not {} pieces: {chains:?}", chains.len());
    assert_eq!(chains[0].len(), 12, "the joined edge keeps every chord");
}

#[test]
fn a_sharp_corner_on_the_same_crease_still_splits() {
    // An L: two straight legs meeting at 90 degrees, three pieces each.
    let path = [[0.0, 30.0], [0.0, 20.0], [0.0, 10.0], [0.0, 0.0], [10.0, 0.0], [20.0, 0.0], [30.0, 0.0]];
    let chains = mid_rim_chains(&band(&path));
    assert_eq!(chains.len(), 2, "the corner splits the crease: {chains:?}");
    assert!(chains.iter().all(|c| c.len() == 3), "each leg is one edge of three pieces: {chains:?}");
}

#[test]
fn a_degenerate_band_never_panics() {
    // Repeated points, a zero-length leg and a NaN corner.
    let path = [[0.0, 0.0], [0.0, 0.0], [10.0, 0.0], [10.0, f64::NAN], [20.0, 0.0]];
    let tris = band(&path);
    let g = feature_graph(&tris);
    let _ = feature::chains(&g.features);
    let _ = feature::edge_runs(&g.features);
}

