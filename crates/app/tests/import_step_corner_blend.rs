//! #1548: a STEP solid whose rounded edges meet in a corner blend - a
//! rational patch with one side collapsed to the corner vertex - imports
//! closed and at its volume, instead of being refused (#1536) or silently
//! losing faces.
//!
//! Fixture: `testdata/models/fillet_corner.step`, the #1536 part's own
//! size - a 30 mm cube centred on the origin - with the three edges at
//! (-15, -15, -15) rounded R3.125: three cylinders and one corner blend, the
//! rounded edges `SURFACE_CURVE`s whose master is a pcurve. Written by
//! solidcore's step-export (the kernel the #1536 model came from), with the
//! face senses turned outward. Its closed-form volume is elementary solid
//! geometry: the cube, less a round's section r^2 (1 - pi/4) along each
//! edge outside the corner cube, less the corner cube's r^3 (1 - pi/6).
//!
//! Gate map (M4.4): covers app::geometry::import::step,
//! app::geometry::import::step_pole, app::geometry::import::step_edge3d,
//! app::geometry::import::step_guard.

use std::collections::HashMap;
use std::f64::consts::PI;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use camcraft_app::geometry::import::{self, step, Format};
use camcraft_app::geometry::{Mesh, Point3};

/// The cube spans -HALF ..= HALF on every axis.
const HALF: f64 = 15.0;
const RADIUS: f64 = 3.125;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/fillet_corner.step")
}

/// Run `f` on its own thread and fail the test - instead of hanging the
/// suite - when it has not answered within `limit`.
fn within<R: Send + 'static>(limit: Duration, f: impl FnOnce() -> R + Send + 'static) -> R {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(limit) {
        Ok(r) => r,
        Err(_) => panic!("import did not finish within {limit:?} - the hang is back"),
    }
}

fn volume(m: &Mesh) -> f64 {
    m.facets
        .iter()
        .map(|t| {
            let (a, b, c) = (t.a, t.b, t.c);
            (a.x * (b.y * c.z - b.z * c.y) - a.y * (b.x * c.z - b.z * c.x) + a.z * (b.x * c.y - b.y * c.x)) / 6.0
        })
        .sum()
}

/// The solid's volume and the area of its curved faces, in closed form.
fn closed_form() -> (f64, f64) {
    let (a, r) = (2.0 * HALF, RADIUS);
    let volume = a.powi(3) - 3.0 * r * r * (1.0 - PI / 4.0) * (a - r) - r.powi(3) * (1.0 - PI / 6.0);
    let curved = 3.0 * (PI / 2.0) * r * (a - r) + (PI / 2.0) * r * r;
    (volume, curved)
}

/// Edges of the mesh that are not shared by exactly two facets running
/// opposite ways, after welding vertices closer than `weld`: 0 for a
/// closed, consistently oriented surface. Vertices are welded by
/// neighbourhood (a grid of `weld`-sized cells, the 27 cells around each
/// point), not by rounding, so two copies of one point either side of a
/// rounding step still meet.
fn open_edges(m: &Mesh, weld: f64) -> usize {
    let cell = |p: Point3| ((p.x / weld).floor() as i64, (p.y / weld).floor() as i64, (p.z / weld).floor() as i64);
    let mut ids: Vec<Point3> = Vec::new();
    let mut grid: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    let mut id_of = |p: Point3| -> usize {
        let (cx, cy, cz) = cell(p);
        for (dx, dy, dz) in (-1..=1).flat_map(|x| (-1..=1).flat_map(move |y| (-1..=1).map(move |z| (x, y, z)))) {
            for &i in grid.get(&(cx + dx, cy + dy, cz + dz)).map(Vec::as_slice).unwrap_or(&[]) {
                let q = ids[i];
                if (q.x - p.x).abs() <= weld && (q.y - p.y).abs() <= weld && (q.z - p.z).abs() <= weld {
                    return i;
                }
            }
        }
        ids.push(p);
        grid.entry((cx, cy, cz)).or_default().push(ids.len() - 1);
        ids.len() - 1
    };
    let mut balance: HashMap<(usize, usize), (i32, u32)> = HashMap::new();
    for t in m.facets.iter() {
        let k = [id_of(t.a), id_of(t.b), id_of(t.c)];
        // A facet with two welded corners has collapsed onto an edge (the
        // sliver between two copies of a pole): it bounds nothing.
        if k[0] == k[1] || k[1] == k[2] || k[2] == k[0] {
            continue;
        }
        for i in 0..3 {
            let (a, b) = (k[i], k[(i + 1) % 3]);
            let e = balance.entry((a.min(b), a.max(b))).or_default();
            e.0 += if a < b { 1 } else { -1 };
            e.1 += 1;
        }
    }
    balance.values().filter(|(sum, uses)| *sum != 0 || *uses != 2).count()
}

/// The checks every sound import of the fixture passes.
fn assert_the_rounded_cube(m: &Mesh, what: &str) {
    let b = m.bounds().expect("non-empty");
    for (lo, hi) in [(b.min.x, b.max.x), (b.min.y, b.max.y), (b.min.z, b.max.z)] {
        assert!((lo + HALF).abs() < 1e-9 && (hi - HALF).abs() < 1e-9, "{what}: bounds {b:?}");
    }
    assert_eq!(open_edges(m, 1e-6), 0, "{what}: the mesh must be closed (every edge shared by two facets, opposite ways)");
    // Facet vertices lie on the true surface and the rounds are convex, so
    // the mesh stands inside the solid by at most the chord error over the
    // curved faces - and never outside it.
    let (want, curved) = closed_form();
    let got = volume(m);
    let short = want - got;
    assert!(
        (-1e-6..=step::CHORD_CAP * curved).contains(&short),
        "{what}: volume {got}, closed form {want}: short by {short}, allowed 0 ..= {}",
        step::CHORD_CAP * curved
    );
}

#[test]
fn a_three_fillet_corner_imports_closed_at_its_volume_within_the_chord_tolerance() {
    // As a user's import runs it: the default evaluation cap, a hard limit.
    let graded = within(Duration::from_secs(120), || import::load_model_graded(&fixture()))
        .expect("a corner blend is ordinary geometry: it must import, not be refused");
    assert_eq!(graded.coarse_mm, None, "the mesh must meet the importer's chord promise");
    assert_the_rounded_cube(&graded.mesh, "fillet_corner.step");
}

/// The same solid with every edge declared against its curve: the two
/// vertices swapped, `same_sense` false, and each use of the edge flipped.
fn with_edges_reversed(text: &str) -> String {
    let flip = |s: &str| if s == ".T." { ".F." } else { ".T." };
    text.lines()
        .map(|line| {
            let Some((head, rest)) = line.split_once('=') else { return line.to_string() };
            let fields: Vec<&str> = rest.trim_end_matches(");").split(',').collect();
            if rest.starts_with("EDGE_CURVE(") && fields.len() == 5 {
                format!("{head}={},{},{},{},{});", fields[0], fields[2], fields[1], fields[3], flip(fields[4]))
            } else if rest.starts_with("ORIENTED_EDGE(") && fields.len() == 5 {
                format!("{head}={},{},{},{},{});", fields[0], fields[1], fields[2], fields[3], flip(fields[4]))
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// An edge may run against its curve (`same_sense` false). Read through
/// its surface curve, truck turns such an edge twice and it runs
/// backwards; read from the 3D curve directly it does not.
#[test]
fn edges_declared_against_their_curves_import_the_same_closed_solid() {
    let text = std::fs::read_to_string(fixture()).expect("fixture");
    let reversed = with_edges_reversed(&text);
    assert_eq!(text.matches("EDGE_CURVE(").count(), 21, "fixture layout changed");
    assert_eq!(reversed.matches("EDGE_CURVE(").count(), 21);
    assert!(reversed.lines().filter(|l| l.contains("=EDGE_CURVE(")).all(|l| l.ends_with(",.F.);")), "every edge turned");
    assert!(text.contains("#94=EDGE_CURVE('',#17,#15,#93,.T.);") && reversed.contains("#94=EDGE_CURVE('',#15,#17,#93,.F.);"));
    let m = within(Duration::from_secs(120), move || import::parse_model(reversed.as_bytes(), Format::Step))
        .expect("the reversed-edge solid imports");
    assert_the_rounded_cube(&m, "edges reversed");
}

#[test]
fn bent_corner_blends_end_cleanly_and_bounded() {
    // The blend's weights, knots and pole bent into nonsense: each variant
    // must come back - an import of finite facets or a refusal - never a
    // hang or a crash.
    let text = std::fs::read_to_string(fixture()).expect("fixture");
    let weights = "RATIONAL_B_SPLINE_SURFACE(((1.0,1.0,2.0),";
    let knots = "B_SPLINE_SURFACE_WITH_KNOTS((5,5),(3,3),(0.,1.0),(0.,1.0),";
    let pole = "B_SPLINE_SURFACE(4,2,((#38,#69,#34),(#38,#70,#71),(#38,#72,#73),";
    for needle in [weights, knots, pole] {
        assert_eq!(text.matches(needle).count(), 1, "fixture layout changed: {needle}");
    }
    let variants = [
        (weights, "RATIONAL_B_SPLINE_SURFACE(((1.0,0.0,2.0),"),
        (weights, "RATIONAL_B_SPLINE_SURFACE(((-1.0,1.0,2.0),"),
        (weights, "RATIONAL_B_SPLINE_SURFACE(((1.0,-1.0,2.0),"),
        (weights, "RATIONAL_B_SPLINE_SURFACE(((1.0E300,1.0E-300,2.0),"),
        (knots, "B_SPLINE_SURFACE_WITH_KNOTS((5,5),(3,3),(0.,1.0),(0.,1.0E300),"),
        (knots, "B_SPLINE_SURFACE_WITH_KNOTS((5,5),(3,3),(1.0,0.),(0.,1.0),"),
        (knots, "B_SPLINE_SURFACE_WITH_KNOTS((5,5),(3,3),(0.,0.),(0.,1.0),"),
        (knots, "B_SPLINE_SURFACE_WITH_KNOTS((5,5),(3,3),(-1.0E9,1.0),(-1.0E9,1.0),"),
        // The pole pulled apart: the collapsed side opens into a curve
        // that is not the corner vertex.
        (pole, "B_SPLINE_SURFACE(4,2,((#38,#69,#34),(#34,#70,#71),(#36,#72,#73),"),
        (pole, "B_SPLINE_SURFACE(4,2,((#34,#69,#34),(#38,#70,#71),(#38,#72,#73),"),
    ];
    for (needle, bent) in variants {
        let data = text.replace(needle, bent).into_bytes();
        let started = std::time::Instant::now();
        let r = within(Duration::from_secs(90), move || {
            step::with_face_eval_cap(200_000, || import::parse_model(&data, Format::Step))
        });
        if let Ok(m) = &r {
            assert!(m.facets.iter().all(|t| [t.a, t.b, t.c].iter().all(|p: &Point3| p.is_finite())), "{bent}");
        }
        eprintln!(
            "{bent}: {} in {:?}",
            r.map(|m| format!("{} facets", m.len())).unwrap_or_else(|e| e.to_string()),
            started.elapsed()
        );
    }
}
