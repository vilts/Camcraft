//! Corrupt-input corpus for the kernel on trial (M12.1 retirement gate): the
//! solidcore port is driven DIRECTLY with degenerate solids and nonsense
//! numbers. It must never panic; no `catch_unwind` here on purpose, so a
//! port panic fails the test and is a finding against the port. Where the
//! port returns `Ok` for input with no valid answer, the verdict is pinned
//! in `KNOWN_PORT_VERDICTS` as a located finding, and the seam leg checks
//! the same inputs come out of `SolidcoreKernel` as `Err`.
//! Provenance: our own corpus, shaped after the Prime Directive's degenerate-
//! input list (zero/NaN/inf sizes, empty and open shells, collapsing offsets).
//!
//! Gate map (M12.1): covers camcraft_kernel::solidcore_impl and solidcore's
//! public primitive/boolean/offset/mass/query entry points, and the seam's
//! blend and union/intersect guards (camcraft_kernel::solidcore_blend).

use camcraft_kernel::{Blend, Kernel};
use camcraft_kernel::solidcore_impl::{SolidcoreKernel, VOLUME_ACCURACY};
use solidcore::blend::fillet_executive::api::fillet_edges;
use solidcore::boolean::api_brep::{boolean_difference, boolean_intersection, boolean_union};
use solidcore::geom::attribute::{AttributeStore, ObjectKey};
use solidcore::mass::integrator::properties::{PropertyAttributes, brep_volume};
use solidcore::math::Point3;
use solidcore::offset::offset_executive::p2::{OffsetOptions, offset_brep_simple};
use solidcore::primitive::api_primitives as prim;
use solidcore::query::brep::{BoundingBoxOptions, calculate_bounding_box};
use solidcore::topology::Brep;

fn o() -> Point3 {
    Point3::new(0.0, 0.0, 0.0)
}

fn unit_box(k: &mut solidcore::Kernel) -> Brep {
    let b = prim::create_box(k, o(), 10.0, 10.0, 10.0);
    assert!(b.is_ok(), "fixture box: {b:?}");
    b.unwrap_or_default()
}

fn volume(k: &solidcore::Kernel, b: &mut Brep, acc: f64) -> solidcore::Result<f64> {
    let store = AttributeStore::default();
    let key_of = |_| ObjectKey(0);
    brep_volume(
        k,
        b,
        acc,
        &PropertyAttributes {
            store: &store,
            key_of: &key_of,
        },
    )
}

fn opts(distance: f64) -> OffsetOptions {
    OffsetOptions {
        distance,
        extend_convex_edges: false,
        merge_results: false,
    }
}

#[test]
fn port_rejects_degenerate_primitive_sizes() {
    let mut k = solidcore::Kernel::default();
    for s in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let r = prim::create_box(&mut k, o(), s, 10.0, 10.0);
        assert!(r.is_err(), "create_box size {s}: {}", verdict(&mut k, r.as_ref()));
        let r = prim::create_cylinder(&mut k, o(), s, 10.0);
        assert!(r.is_err(), "create_cylinder radius {s}: {}", verdict(&mut k, r.as_ref()));
        let r = prim::create_cylinder(&mut k, o(), 2.0, s);
        assert!(r.is_err(), "create_cylinder height {s}: {}", verdict(&mut k, r.as_ref()));
    }
}

/// Compact, stable verdict on a port result: `Err`, `Ok non-solid`, or
/// `Ok solid V=<3 decimals>`.
fn verdict(k: &mut solidcore::Kernel, r: Result<&Brep, &solidcore::SolidError>) -> String {
    let Ok(b) = r else {
        return "Err".into();
    };
    let solid = solidcore::topology::integrity::check(b).is_ok()
        && solidcore::topology::brep::p2::is_manifold_solid(b).unwrap_or(false);
    if !solid {
        return "Ok non-solid".into();
    }
    // The port's B-rep is no longer `Clone` at the pinned commit (#916):
    // a copy is the kernel's own `brep_copy`.
    let Ok(mut b) = solidcore::topology::brep::p3::brep_copy(k, b) else {
        return "Ok solid copy=Err".into();
    };
    match volume(k, &mut b, VOLUME_ACCURACY) {
        Ok(v) => format!("Ok solid V={v:.3}"),
        Err(_) => "Ok solid V=Err".into(),
    }
}

fn boolean_case(name: &str, k: &mut solidcore::Kernel) -> solidcore::Result<Brep> {
    let bx = |k: &mut solidcore::Kernel| prim::create_box(k, o(), 10.0, 10.0, 10.0);
    let sheet = |k: &mut solidcore::Kernel| {
        prim::create_plane(k, Point3::new(-5.0, -5.0, 5.0), 20.0, 20.0)
    };
    match name {
        "empty - box" => {
            let b = bx(k)?;
            boolean_difference(k, Brep::new(), b)
        }
        "box - empty" => {
            let b = bx(k)?;
            boolean_difference(k, b, Brep::new())
        }
        "box - sheet" => {
            let (a, b) = (bx(k)?, sheet(k)?);
            boolean_difference(k, a, b)
        }
        "sheet - box" => {
            let (a, b) = (sheet(k)?, bx(k)?);
            boolean_difference(k, a, b)
        }
        _ => {
            let (a, b) = (bx(k)?, bx(k)?);
            boolean_difference(k, a, b)
        }
    }
}

/// What the port does with each degenerate input today, pinned. An entry
/// that is not `Err` where the input has no valid answer is a FINDING
/// against the port (listed in the hand-off); the seam guards each one
/// (`seam_turns_every_degenerate_case_into_err`). The table is pinned so a
/// port bump that changes any verdict - fix or regression - fails here and
/// gets looked at, instead of drifting silently.
const KNOWN_PORT_VERDICTS: &[(&str, &str)] = &[
    ("box at NaN origin", "Ok non-solid"),
    ("empty - box", "Err"),
    ("box - empty", "Ok solid V=1000.000"),
    ("box - sheet", "Ok non-solid"),
    ("sheet - box", "Ok non-solid"),
    ("box - same box", "Ok non-solid"),
    // Re-triaged at the pin of #916: 1.0.1 answered a NaN distance with an
    // unchanged copy ("Ok solid V=1000.000"); the pinned commit refuses it.
    ("offset NaN", "Err"),
    ("offset inf", "Err"),
    ("offset -inf", "Err"),
    ("offset -1 (SMLib default flags)", "Ok non-solid"),
    ("offset -5 (collapse)", "Err"),
    ("offset -6 (past collapse)", "Ok non-solid"),
    ("offset -6 (past collapse, merge)", "Ok solid V=8.000"),
    ("offset -50 (past collapse)", "Ok non-solid"),
    ("offset of empty brep", "Ok non-solid"),
];

#[test]
fn port_degenerate_inputs_never_panic_and_match_pinned_verdicts() {
    let mut observed: Vec<(String, String)> = Vec::new();
    let mut k = solidcore::Kernel::default();
    let r = prim::create_box(&mut k, Point3::new(f64::NAN, 0.0, 0.0), 1.0, 1.0, 1.0);
    observed.push(("box at NaN origin".into(), verdict(&mut k, r.as_ref())));
    for name in ["empty - box", "box - empty", "box - sheet", "sheet - box", "box - same box"] {
        let mut k = solidcore::Kernel::default();
        let r = boolean_case(name, &mut k);
        observed.push((name.into(), verdict(&mut k, r.as_ref())));
    }
    let offsets: [(&str, f64, bool); 8] = [
        ("offset NaN", f64::NAN, false),
        ("offset inf", f64::INFINITY, false),
        ("offset -inf", f64::NEG_INFINITY, false),
        ("offset -1 (SMLib default flags)", -1.0, false),
        ("offset -5 (collapse)", -5.0, false),
        ("offset -6 (past collapse)", -6.0, false),
        ("offset -6 (past collapse, merge)", -6.0, true),
        ("offset -50 (past collapse)", -50.0, false),
    ];
    for (name, d, merge) in offsets {
        let mut k = solidcore::Kernel::default();
        let mut b = unit_box(&mut k);
        let mut o = opts(d);
        o.merge_results = merge;
        let r = offset_brep_simple(&mut k, &mut b, o);
        observed.push((name.into(), verdict(&mut k, r.as_ref())));
    }
    let mut k = solidcore::Kernel::default();
    let r = offset_brep_simple(&mut k, &mut Brep::new(), opts(1.0));
    observed.push(("offset of empty brep".into(), verdict(&mut k, r.as_ref())));
    for (n, v) in &observed {
        println!("{n:<36} -> {v}");
    }
    let pinned: Vec<(String, String)> = KNOWN_PORT_VERDICTS
        .iter()
        .map(|(n, v)| ((*n).to_string(), (*v).to_string()))
        .collect();
    assert_eq!(observed, pinned, "port verdicts changed - re-triage the findings");
}

#[test]
fn port_measure_rejects_empty_and_nonsense_accuracy() {
    let mut k = solidcore::Kernel::default();
    let mut empty = Brep::new();
    let v = volume(&k, &mut empty, VOLUME_ACCURACY);
    println!("volume(empty) -> {v:?}");
    assert!(v.as_ref().map_or(true, |v| *v == 0.0), "empty volume {v:?}");
    let bb = calculate_bounding_box(&k, &mut empty, BoundingBoxOptions::default());
    println!("bbox(empty) -> {bb:?}");
    let mut b = unit_box(&mut k);
    for acc in [f64::NAN, -1.0, 0.0, f64::INFINITY] {
        let v = volume(&k, &mut b, acc);
        assert!(v.is_err(), "volume at accuracy {acc}: {v:?}");
    }
}

#[test]
fn seam_turns_every_degenerate_case_into_err() {
    let mut k = SolidcoreKernel::default();
    assert!(k.make_box([0.0; 3], [0.0, 1.0, 1.0]).is_err());
    assert!(k.make_box([f64::NAN, 0.0, 0.0], [1.0; 3]).is_err());
    assert!(k.make_cylinder([0.0; 3], f64::INFINITY, 1.0).is_err());
    // identical operands -> empty result -> Err at the seam
    let a = k.make_box([0.0; 3], [10.0; 3]);
    let b = k.make_box([0.0; 3], [10.0; 3]);
    if let (Ok(a), Ok(b)) = (a, b) {
        let r = k.boolean_subtract(a, b);
        assert!(r.is_err(), "box - same box at the seam: {r:?}");
    }
    // empty operand
    let b = k.make_box([0.0; 3], [10.0; 3]);
    if let Ok(b) = b {
        assert!(k.boolean_subtract(Brep::new(), b).is_err());
    }
    // nonsense and collapsing offsets
    if let Ok(mut b) = k.make_box([0.0; 3], [10.0; 3]) {
        for d in [f64::NAN, 0.0, -5.0, -6.0] {
            let r = k.offset(&mut b, d);
            assert!(r.is_err(), "seam offset {d}: {:?}", r.map(|_| "Ok"));
        }
    }
    let m = k.measure(&mut Brep::new());
    assert!(m.is_err(), "seam measure of an empty brep: {m:?}");
}

/// The stage-2 ops at the seam: nonsense blend input and empty booleans are
/// clean `Err`s, never a panic and never an unchanged or invalid solid.
#[test]
fn seam_refuses_degenerate_blends_and_empty_booleans() {
    let mut k = SolidcoreKernel::default();
    let edge: [[f64; 3]; 1] = [[5.0, 0.0, 0.0]];
    let cases: [(&str, Vec<[f64; 3]>, f64); 10] = [
        ("NaN radius", edge.to_vec(), f64::NAN),
        ("zero radius", edge.to_vec(), 0.0),
        ("negative radius", edge.to_vec(), -1.0),
        ("inf radius", edge.to_vec(), f64::INFINITY),
        ("radius past the conservative guard", edge.to_vec(), 6.0),
        ("no picks", Vec::new(), 1.0),
        ("pick off every edge", vec![[5.0, 5.0, 5.0]], 1.0),
        ("NaN pick", vec![[f64::NAN, 0.0, 0.0]], 1.0),
        ("one edge picked twice", vec![[5.0, 0.0, 0.0], [6.0, 0.0, 0.0]], 1.0),
        ("pick at a vertex (ambiguous)", vec![[0.0, 0.0, 0.0]], 1.0),
    ];
    for (name, picks, r) in cases {
        for kind in [Blend::Round, Blend::Chamfer] {
            let r = k
                .make_box([0.0; 3], [10.0; 3])
                .and_then(|b| k.blend_edges(b, &picks, r, kind));
            println!("blend {name:<28} {kind:?} -> {:?}", r.as_ref().map(|_| "Ok"));
            assert!(r.is_err(), "seam blend {name} ({kind:?}) came back Ok");
        }
    }
    assert!(k.blend_edges(Brep::new(), &edge, 1.0, Blend::Round).is_err(), "blend of an empty brep");
    // disjoint operands: the intersection is empty -> Err
    let a = k.make_box([0.0; 3], [10.0; 3]);
    let b = k.make_box([20.0; 3], [10.0; 3]);
    if let (Ok(a), Ok(b)) = (a, b) {
        let r = k.boolean_intersect(a, b);
        println!("intersect disjoint -> {:?}", r.as_ref().map(|_| "Ok"));
        assert!(r.is_err(), "intersect of disjoint boxes came back Ok");
    }
    let b = k.make_box([0.0; 3], [10.0; 3]);
    if let Ok(b) = b {
        let r = k.boolean_intersect(b, Brep::new());
        assert!(r.is_err(), "intersect with an empty brep came back Ok");
    }
    let b = k.make_box([0.0; 3], [10.0; 3]);
    if let Ok(b) = b {
        // empty + box: either refused or exactly the box, never anything else
        let r = k.boolean_union(Brep::new(), b).and_then(|mut u| k.measure(&mut u));
        println!("union empty + box -> {r:?}");
        if let Ok(m) = r {
            assert!((m.volume - 1000.0).abs() < 1e-6, "union empty + box: {m:?}");
        }
    }
}

/// The port's own answers on the stage-2 degenerate inputs, pinned like
/// `KNOWN_PORT_VERDICTS` (a separate table so stage 1's stays unchanged).
/// `box & empty` returning the whole box is a FINDING against the port;
/// the seam refuses empty operands up front. `box & disjoint box` is an
/// `Ok` non-solid the seam's result check turns into `Err`. The r6 round
/// is CORRECT (1000 - 360 (1 - pi/4)); the seam still refuses it on
/// purpose - its radius guard is conservative (decision 4 in
/// docs/decisions/27-solidcore-bench.md).
const KNOWN_PORT_VERDICTS_STAGE2: &[(&str, &str)] = &[
    ("box & empty", "Ok solid V=1000.000"),
    ("empty | box", "Err"),
    ("box & disjoint box", "Ok non-solid"),
    ("fillet r6 on a 10-box edge", "Ok solid V=922.743"),
    ("fillet r NaN on a 10-box edge", "Err"),
];

#[test]
fn port_stage2_degenerates_never_panic_and_match_pinned_verdicts() {
    let mut observed: Vec<(String, String)> = Vec::new();
    let mut k = solidcore::Kernel::default();
    let b = unit_box(&mut k);
    let r = boolean_intersection(&mut k, b, Brep::new());
    observed.push(("box & empty".into(), verdict(&mut k, r.as_ref())));
    let b = unit_box(&mut k);
    let r = boolean_union(&mut k, Brep::new(), b);
    observed.push(("empty | box".into(), verdict(&mut k, r.as_ref())));
    let a = unit_box(&mut k);
    let far = prim::create_box(&mut k, Point3::new(20.0, 20.0, 20.0), 10.0, 10.0, 10.0);
    let r = far.and_then(|f| boolean_intersection(&mut k, a, f));
    observed.push(("box & disjoint box".into(), verdict(&mut k, r.as_ref())));
    for (name, radius) in [("fillet r6 on a 10-box edge", 6.0), ("fillet r NaN on a 10-box edge", f64::NAN)] {
        let mut k = solidcore::Kernel::default();
        let mut b = unit_box(&mut k);
        let edge = b.edges().ids().next();
        let r = match edge {
            Some(e) => fillet_edges(&mut k, &mut b, &[e], radius, 1, 1, 1.0).map(|()| b),
            None => Ok(Brep::new()),
        };
        observed.push((name.into(), verdict(&mut k, r.as_ref())));
    }
    for (n, v) in &observed {
        println!("{n:<36} -> {v}");
    }
    let pinned: Vec<(String, String)> = KNOWN_PORT_VERDICTS_STAGE2
        .iter()
        .map(|(n, v)| ((*n).to_string(), (*v).to_string()))
        .collect();
    assert_eq!(observed, pinned, "port verdicts changed - re-triage the findings");
}
