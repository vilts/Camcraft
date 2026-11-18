//! #744 pins for shared DXF read/write and spun / extruded fixture profiles
//! in `camcraft_geometry`.
//!
//! Gate map (M4.4): covers camcraft_geometry::dxf (writer + annotations),
//! camcraft_geometry::dxf_profile, camcraft_geometry::profile_mesh,
//! camcraft_geometry::sketch_chains.
//! Provenance: hand-built ASCII DXF fixtures and closed-solid invariants.

use std::collections::HashMap;
use std::f64::consts::PI;

use camcraft_geometry::dxf::{Unit, Units, parse};
use camcraft_geometry::dxf_profile::{
    ClosedProfile, ExtrudeAxis, HalfProfile, ProfileVertex, extruded_from_dxf, extruded_mesh,
    extruded_to_dxf, spun_from_dxf, spun_mesh, spun_to_dxf,
};
use camcraft_geometry::geometry::{Point3, Triangle};

fn dxf_wrap(units: i32, entities: &str) -> String {
    format!(
        "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n{units}\n0\nENDSEC\n\
         0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n70\n3\n\
         0\nLAYER\n2\nCENTRELINE\n70\n0\n62\n1\n6\nCONTINUOUS\n\
         0\nLAYER\n2\nPROFILE\n70\n0\n62\n7\n6\nCONTINUOUS\n\
         0\nLAYER\n2\nDEPTH\n70\n0\n62\n3\n6\nCONTINUOUS\n\
         0\nENDTAB\n0\nENDSEC\n\
         0\nSECTION\n2\nENTITIES\n{entities}0\nENDSEC\n0\nEOF\n"
    )
}

fn signed_volume(facets: &[Triangle]) -> f64 {
    facets
        .iter()
        .map(|t| t.a.dot(t.b.cross(t.c)))
        .sum::<f64>()
        / 6.0
}

fn quant(p: Point3) -> [i64; 3] {
    [
        (p.x * 1e6).round() as i64,
        (p.y * 1e6).round() as i64,
        (p.z * 1e6).round() as i64,
    ]
}

fn assert_closed_outward_solid(facets: &[Triangle], label: &str) {
    assert!(!facets.is_empty(), "{label}: empty mesh");
    let mut directed: HashMap<([i64; 3], [i64; 3]), usize> = HashMap::new();
    for t in facets {
        let pts = [quant(t.a), quant(t.b), quant(t.c)];
        for i in 0..3 {
            let a = pts[i];
            let b = pts[(i + 1) % 3];
            assert_ne!(a, b, "{label}: degenerate triangle edge {t:?}");
            *directed.entry((a, b)).or_default() += 1;
        }
    }
    for ((a, b), count) in &directed {
        assert_eq!(*count, 1, "{label}: duplicate directed edge {a:?}->{b:?}");
        assert_eq!(
            directed.get(&(*b, *a)).copied(),
            Some(1),
            "{label}: open or mismatched opposite edge {a:?}->{b:?}"
        );
    }
    let vol = signed_volume(facets);
    assert!(vol > 0.0, "{label}: inward or zero signed volume {vol}");
}

#[test]
fn spun_fixture_imports_from_dxf_centreline_plus_half_profile_and_roundtrips() {
    // Stepped chuck body (z -20..0, r 40) + front shoulder (z 0..15, r 25 with arc bevel).
    let entities = "\
        0\nLINE\n8\nCENTRELINE\n10\n-30\n20\n0\n11\n30\n21\n0\n\
        0\nLINE\n8\nPROFILE\n10\n-20\n20\n0\n11\n-20\n21\n40\n\
        0\nLINE\n8\nPROFILE\n10\n-20\n20\n40\n11\n0\n21\n40\n\
        0\nLINE\n8\nPROFILE\n10\n0\n20\n40\n11\n0\n21\n25\n\
        0\nLINE\n8\nPROFILE\n10\n0\n20\n25\n11\n10\n21\n25\n\
        0\nARC\n8\nPROFILE\n10\n10\n20\n20\n40\n5\n50\n0\n51\n90\n\
        0\nLINE\n8\nPROFILE\n10\n15\n20\n20\n11\n15\n21\n0\n";
    let dxf = dxf_wrap(4, entities);
    let half = spun_from_dxf(dxf.as_bytes()).expect("spun DXF imports");
    let facets = spun_mesh(&half).expect("spun mesh builds");
    assert_closed_outward_solid(&facets, "spun chuck");

    let z_min = facets.iter().flat_map(|t| [t.a.z, t.b.z, t.c.z]).fold(f64::INFINITY, f64::min);
    let z_max = facets.iter().flat_map(|t| [t.a.z, t.b.z, t.c.z]).fold(f64::NEG_INFINITY, f64::max);
    let r_max = facets
        .iter()
        .flat_map(|t| [t.a, t.b, t.c])
        .map(|p| p.x.hypot(p.y))
        .fold(0.0_f64, f64::max);
    assert!((z_min - (-20.0)).abs() < 1e-6, "z_min={z_min}");
    assert!((z_max - 15.0).abs() < 1e-6, "z_max={z_max}");
    assert!((r_max - 40.0).abs() < 0.2, "r_max={r_max}");

    // DXF export -> parse -> spun_from_dxf round-trips cleanly.
    let exported = spun_to_dxf(&half).expect("export spun DXF");
    let doc = parse(exported.as_bytes()).expect("parse exported DXF");
    assert_eq!(doc.units, Units::Known(Unit::Millimeters));
    let again = spun_from_dxf(exported.as_bytes()).expect("re-import spun DXF");
    let facets_again = spun_mesh(&again).expect("re-mesh");
    assert!((signed_volume(&facets_again) - signed_volume(&facets)).abs() < 1e-3);
}

#[test]
fn extruded_fixture_reads_closed_profile_and_depth_from_annotation_or_arg() {
    // 50 x 30 mm stepped jaw block in LWPOLYLINE, depth 25 mm via TEXT annotation.
    let entities = "\
        0\nLWPOLYLINE\n8\nPROFILE\n90\n6\n70\n1\n\
        10\n0\n20\n0\n\
        10\n50\n20\n0\n\
        10\n50\n20\n15\n\
        10\n30\n20\n15\n\
        10\n30\n20\n30\n\
        10\n0\n20\n30\n\
        0\nTEXT\n8\nDEPTH\n10\n5\n20\n5\n1\nDEPTH=25\n";
    let dxf = dxf_wrap(4, entities);
    let (prof, depth) = extruded_from_dxf(dxf.as_bytes(), None).expect("extruded DXF imports");
    assert!((depth - 25.0).abs() < 1e-9, "depth={depth}");
    let facets = extruded_mesh(&prof, ExtrudeAxis::Z, depth).expect("extrude mesh");
    assert_closed_outward_solid(&facets, "extruded jaw Z");

    // Expected volume: (50*15 + 30*15) * 25 = 1200 * 25 = 30000 mm^3.
    let vol = signed_volume(&facets);
    assert!((vol - 30_000.0).abs() < 1e-3, "expected 30000, got {vol}");

    // Layer-name depth annotation (`DEPTH_1` scaled by inches = 25.4mm) and explicit mm suffix.
    let layer_dxf = dxf_wrap(
        1, // inches -> * 25.4
        "0\nLWPOLYLINE\n8\nDEPTH_1\n90\n4\n70\n1\n10\n0\n20\n0\n10\n2\n20\n0\n10\n2\n20\n1\n10\n0\n20\n1\n",
    );
    let (inch_prof, layer_depth) = extruded_from_dxf(layer_dxf.as_bytes(), None).expect("layer depth");
    assert!((layer_depth - 25.4).abs() < 1e-6);
    let mm_suffix_dxf = dxf_wrap(
        1, // inches for geometry, explicit mm on DEPTH annotation
        "0\nTEXT\n8\nDEPTH\n10\n0\n20\n0\n40\n2\n1\nDEPTH=12.5mm\n0\nLWPOLYLINE\n8\nPROFILE\n90\n4\n70\n1\n10\n0\n20\n0\n10\n2\n20\n0\n10\n2\n20\n1\n10\n0\n20\n1\n",
    );
    let (_, explicit_mm_depth) = extruded_from_dxf(mm_suffix_dxf.as_bytes(), None).expect("mm suffix");
    assert!((explicit_mm_depth - 12.5).abs() < 1e-6);
    let inch_facets = extruded_mesh(&inch_prof, ExtrudeAxis::Y, 12.0).expect("extrude Y");
    assert_closed_outward_solid(&inch_facets, "extruded jaw Y");
    let inch_vol = signed_volume(&inch_facets);
    // 2 in (50.8 mm) x 1 in (25.4 mm) * 12 mm
    assert!((inch_vol - 50.8 * 25.4 * 12.0).abs() < 1e-2, "vol={inch_vol}");

    // DXF write round-trip keeps DEPTH annotation + closed loop.
    let written = extruded_to_dxf(&prof, depth).expect("write extruded DXF");
    let (prof2, depth2) = extruded_from_dxf(written.as_bytes(), None).expect("re-read exported DXF");
    assert!((depth2 - depth).abs() < 1e-6);
    assert_eq!(prof2.points.len(), prof.points.len());

    // The exported fixture outline also remains usable by the sketch-chain API.
    let doc = parse(written.as_bytes()).expect("parse exported fixture outline");
    let chains = camcraft_geometry::sketch_chains::chain(
        &doc.primitives,
        camcraft_geometry::sketch_chains::ChainOptions::default(),
    )
    .expect("chain exported fixture outline");
    assert!(chains.open.is_empty() && chains.rejected.is_empty());
    assert_eq!(chains.closed.len(), 1);
    assert!((chains.closed[0].area - vol / depth).abs() < 1e-6);
}

#[test]
fn dxf_profile_malformed_inputs_give_named_errors_never_panic() {
    let refuse_spun = |dxf: &str, expected: &str| {
        let err = spun_from_dxf(dxf.as_bytes()).expect_err(expected).to_string();
        assert!(err.to_lowercase().contains(&expected.to_lowercase()), "{err:?} missing {expected:?}");
    };
    let refuse_ext = |dxf: &str, depth: Option<f64>, expected: &str| {
        let err = extruded_from_dxf(dxf.as_bytes(), depth).expect_err(expected).to_string();
        assert!(err.to_lowercase().contains(&expected.to_lowercase()), "{err:?} missing {expected:?}");
    };

    // Missing centreline.
    refuse_spun(
        &dxf_wrap(4, "0\nLINE\n8\nPROFILE\n10\n0\n20\n0\n11\n10\n21\n0\n"),
        "centreline",
    );
    // Crosses centreline (r < 0).
    refuse_spun(
        &dxf_wrap(
            4,
            "0\nLINE\n8\nCENTRELINE\n10\n0\n20\n0\n11\n20\n21\n0\n\
             0\nLINE\n8\nPROFILE\n10\n0\n20\n0\n11\n10\n21\n5\n\
             0\nLINE\n8\nPROFILE\n10\n10\n20\n5\n11\n20\n21\n-3\n",
        ),
        "centreline",
    );
    // Open extruded profile.
    refuse_ext(
        &dxf_wrap(4, "0\nLINE\n8\nPROFILE\n10\n0\n20\n0\n11\n10\n21\n0\n0\nLINE\n8\nPROFILE\n10\n10\n20\n0\n11\n10\n21\n10\n"),
        Some(10.0),
        "open",
    );
    // Self-crossing bowtie polygon.
    refuse_ext(
        &dxf_wrap(4, "0\nLWPOLYLINE\n8\nPROFILE\n90\n4\n70\n1\n10\n0\n20\n0\n10\n10\n20\n10\n10\n10\n20\n0\n10\n0\n20\n10\n"),
        Some(10.0),
        "cross",
    );
    // Missing or non-positive extrusion depth.
    let rect = dxf_wrap(4, "0\nLWPOLYLINE\n8\nPROFILE\n90\n4\n70\n1\n10\n0\n20\n0\n10\n10\n20\n0\n10\n10\n20\n10\n10\n0\n20\n10\n");
    refuse_ext(&rect, None, "depth");
    refuse_ext(&rect, Some(0.0), "depth");
    refuse_ext(&rect, Some(-5.0), "depth");
    // Unsupported DXF entity.
    refuse_ext(&dxf_wrap(4, "0\nSPLINE\n"), Some(10.0), "SPLINE");
    // Self-crossing half profile.
    let bowtie_half = HalfProfile {
        points: vec![
            ProfileVertex { u: 0.0, v: 0.0, bulge: 0.0 },
            ProfileVertex { u: 10.0, v: 10.0, bulge: 0.0 },
            ProfileVertex { u: 0.0, v: 10.0, bulge: 0.0 },
            ProfileVertex { u: 10.0, v: 0.0, bulge: 0.0 },
        ],
    };
    assert!(spun_mesh(&bowtie_half).is_err(), "self-crossing half-profile must refuse");
    let _ = ClosedProfile { points: vec![] };
    let _ = PI;
}

#[test]
fn dxf_profile_all_segments_dropped_returns_named_refusal_for_both_imports() {
    let dxf = dxf_wrap(4, "0\nLINE\n8\nCENTRELINE\n10\n0\n20\n0\n11\n20\n21\n0\n0\nLINE\n8\nPROFILE\n10\n0\n20\n0\n11\n0.0000000001\n21\n0\n");
    for error in [
        extruded_from_dxf(dxf.as_bytes(), Some(10.0)).unwrap_err(),
        spun_from_dxf(dxf.as_bytes()).unwrap_err(),
    ] {
        assert_eq!(error.to_string(), "DXF profile contains no non-degenerate segments");
    }
}

#[test]
fn closed_annular_spun_section_preserves_closing_edge_and_bulge_on_dxf_roundtrip() {
    for bulge in [0.0, 0.25] {
        let dxf = dxf_wrap(4, &format!("0\nLINE\n8\nCENTRELINE\n10\n-5\n20\n0\n11\n15\n21\n0\n0\nLWPOLYLINE\n8\nPROFILE\n90\n4\n70\n1\n10\n0\n20\n10\n10\n10\n20\n10\n10\n10\n20\n20\n10\n0\n20\n20\n42\n{bulge}\n"));
        let half = spun_from_dxf(dxf.as_bytes()).expect("closed annular section imports");
        assert_eq!(half.points.len(), 5, "closure is explicit");
        assert_eq!(half.points[3].bulge, bulge, "closing edge retains its bulge");
        assert_eq!((half.points[0].u, half.points[0].v), (half.points[4].u, half.points[4].v));
        let facets = spun_mesh(&half).expect("annular solid");
        assert_closed_outward_solid(&facets, "annular section");
        // The bore remains empty; closure must not substitute axis endpoints.
        assert!(facets.iter().flat_map(|t| [t.a, t.b, t.c]).all(|p| p.x.hypot(p.y) >= 10.0 - 1e-6));
        let written = spun_to_dxf(&half).expect("annular DXF export");
        let again = spun_from_dxf(written.as_bytes()).expect("annular DXF reimport");
        assert_eq!(again.points, half.points);
        assert_closed_outward_solid(&spun_mesh(&again).unwrap(), "roundtripped annulus");
    }
}
