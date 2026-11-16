//! M1.1 acceptance: good files import, corrupt files error - never crash.
//! The corrupt corpus is built in code (deterministic) so it is byte-exact
//! and reviewable; cube.stl / box.obj live in testdata/models/ for manual
//! runs and are the same unit cube.
//!
//! Gate map (M4.4): covers app::geometry.

use std::path::{Path, PathBuf};

use camcraft_app::geometry::import::{self, Format, ImportError, MAX_FACETS};
use camcraft_app::geometry::{cube, Mesh, Point3};

fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name)
}

/// All facets must face away from the mesh center (imported winding sanity).
fn assert_outward(m: &Mesh) {
    let b = m.bounds().expect("non-empty mesh");
    let c = b.center();
    for t in m.facets.iter() {
        let centroid = Point3::new(
            (t.a.x + t.b.x + t.c.x) / 3.0,
            (t.a.y + t.b.y + t.c.y) / 3.0,
            (t.a.z + t.b.z + t.c.z) / 3.0,
        );
        assert!(t.normal().dot(centroid - c) > 0.0, "inward facet {t:?}");
    }
}

fn cube_reference() -> Mesh {
    cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0))
}

// ---------- good files ----------

#[test]
fn ascii_stl_cube() {
    let m = import::load_model(&testdata("cube.stl")).expect("cube.stl must import");
    assert_eq!(m.len(), 12);
    assert_outward(&m);
    let b = m.bounds().expect("bounds");
    assert!(b.min.x == 0.0 && b.max.z == 1.0);
}

#[test]
fn obj_cube_matches_primitive() {
    let m = import::load_model(&testdata("box.obj")).expect("box.obj must import");
    assert_eq!(m, cube_reference(), "quads must fan-triangulate like cube()");
    assert_outward(&m);
}

#[test]
fn binary_stl_roundtrip_of_ascii_content() {
    let ascii = import::load_model(&testdata("cube.stl")).expect("ascii");
    let mut bytes = mesh_to_binary(&ascii);
    // header says "solid" on purpose: size math must still pick binary
    bytes[0..5].copy_from_slice(b"solid");
    let m = import::parse_model(&bytes, Format::Stl).expect("binary");
    assert_eq!(m.len(), 12);
    for (a, b) in ascii.facets.iter().zip(m.facets.iter()) {
        assert_eq!(a.a.x, b.a.x);
        assert_eq!(a.c.z, b.c.z);
    }
}

#[test]
fn obj_negative_and_slashed_references() {
    let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf -3/-2/-1 -2 -1\n";
    let m = import::parse_model(text.as_bytes(), Format::Obj).expect("obj");
    assert_eq!(m.len(), 1);
}

#[test]
fn obj_polygon_fan() {
    // 5-gon must yield 3 triangles sharing vertex 1
    let text = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0.5 0.5 0\nf 1 2 3 4 5\n";
    let m = import::parse_model(text.as_bytes(), Format::Obj).expect("obj");
    assert_eq!(m.len(), 3);
}

#[test]
fn obj_quads_sharing_edge() {
    let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 1 1 0\nf 1 2 4 3\nf 1 3 4 2\n";
    let m = import::parse_model(text.as_bytes(), Format::Obj).expect("obj");
    assert_eq!(m.len(), 4);
}

// ---------- corrupt files: must be clean errors ----------

#[test]
fn empty_is_not_a_mesh() {
    assert!(matches!(import::parse_model(b"", Format::Stl), Err(ImportError::Empty)));
    assert!(matches!(import::parse_model(b"", Format::Obj), Err(ImportError::Empty)));
}

#[test]
fn truncated_binary_stl() {
    let bytes = mesh_to_binary(&cube_reference());
    let cut = &bytes[..bytes.len() - 17];
    match import::parse_model(cut, Format::Stl) {
        Err(ImportError::Stl(msg)) => assert!(msg.contains("does not match"), "{msg}"),
        other => panic!("expected Stl size error, got {other:?}"),
    }
}

#[test]
fn huge_facet_count_is_refused_before_allocating() {
    let mut bytes = vec![0u8; 84];
    bytes[80..84].copy_from_slice(&(u32::MAX).to_le_bytes());
    match import::parse_model(&bytes, Format::Stl) {
        Err(ImportError::TooLarge) => {}
        other => panic!("expected TooLarge, got {other:?}"),
    }
}

#[test]
fn ascii_nan_vertex_rejected() {
    let text = "solid x\nfacet normal 0 0 0\nouter loop\nvertex NaN 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid x\n";
    match import::parse_model(text.as_bytes(), Format::Stl) {
        Err(ImportError::Stl(msg)) => assert!(msg.contains("non-finite"), "{msg}"),
        other => panic!("expected non-finite error, got {other:?}"),
    }
}

#[test]
fn ascii_garbage_rejected() {
    assert!(matches!(
        import::parse_model(b"solid x\nthis is not stl\nendsolid x\n", Format::Stl),
        Err(ImportError::Stl(_))
    ));
}

#[test]
fn obj_reference_out_of_range_rejected() {
    let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 99\n";
    match import::parse_model(text.as_bytes(), Format::Obj) {
        Err(ImportError::Obj(msg)) => assert!(msg.contains("out of range"), "{msg}"),
        other => panic!("expected range error, got {other:?}"),
    }
}

#[test]
fn obj_degenerate_facet_rejected() {
    let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 1 2\n";
    assert!(matches!(
        import::parse_model(text.as_bytes(), Format::Obj),
        Err(ImportError::Obj(_))
    ));
}

#[test]
fn binary_garbage_never_panics() {
    // any random bytes must return Err, this loop is a smoke fuzz
    for seed in 0..64u64 {
        let mut data = Vec::new();
        let mut x = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        for _ in 0..96 {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            data.push((x >> 33) as u8);
        }
        let _ = import::parse_model(&data, Format::Stl);
        let _ = import::parse_model(&data, Format::Obj);
    }
}

#[test]
fn unsupported_extension() {
    // Dispatch happens on the extension, before any file read.
    let err = import::load_model(Path::new("model.iges")).err();
    assert!(matches!(err, Some(ImportError::Unsupported(_))));
}

// ---------- STEP (truck tessellator) ----------

#[test]
fn step_box_matches_primitive_corner_set() {
    let m = import::load_model(&testdata("box.step")).expect("box.step must import");
    assert_eq!(m.len(), 12, "a box is 12 triangles, whatever the fan order");
    let b = m.bounds().expect("bounds");
    assert!(b.min.x == 0.0 && b.min.y == 0.0 && b.min.z == 0.0, "{b:?}");
    assert!(b.max.x == 1.0 && b.max.y == 1.0 && b.max.z == 1.0, "{b:?}");
    for t in m.facets.iter() {
        let n = t.normal();
        assert!(n.x != 0.0 || n.y != 0.0 || n.z != 0.0, "degenerate facet leaked: {t:?}");
    }
}

/// Provenance: this AP214 file is from the ruststep project's Apache-2.0
/// test corpus (ricosjp/ruststep) - a real-world export, not our geometry.
#[test]
fn step_real_world_file_imports() {
    let m = import::load_model(&testdata("sample_ap214.step")).expect("must import");
    assert!(m.len() > 100, "faceted part should tessellate to >100 facets, got {}", m.len());
    let b = m.bounds().expect("bounds");
    assert!(b.min.x.is_finite() && b.max.z.is_finite());
}

/// #306: the AP214 sample states its lengths in metres
/// (`CONVERSION_BASED_UNIT('METRE', ..)` over `SI_UNIT($, .METRE.)`), so
/// it imports at its true size in millimetres, about 65 mm across - not
/// 0.065 mm, its numbers read as millimetres.
#[test]
fn a_step_file_in_metres_imports_at_its_size_in_millimetres() {
    let m = import::load_model(&testdata("sample_ap214.step")).expect("must import");
    let b = m.bounds().expect("bounds");
    let width = b.max.x - b.min.x;
    assert!((60.0..70.0).contains(&width), "the part is {width} mm wide");
}

/// The millimetre file (`SI_UNIT(.MILLI., .METRE.)`) keeps its numbers.
#[test]
fn a_step_file_in_millimetres_keeps_its_numbers() {
    let m = import::load_model(&testdata("Milling-test.step")).expect("must import");
    let b = m.bounds().expect("bounds");
    // The part's true +X is 30 mm, reached by a curved face: the mesh's
    // extreme vertex stands within the chord cap (0.005 mm, #689) of it.
    assert!(
        (b.max.x - 30.0).abs() < 0.005 && (b.max.z - 20.0).abs() < 1e-9,
        "max corner ({}, {}, {})",
        b.max.x,
        b.max.y,
        b.max.z
    );
}

/// #887: the empty parameter list `()` is legal in ISO 10303-21, but ruststep
/// 0.4.0 (the newest crates.io release) cannot parse it, so one dead
/// `DRAUGHTING_MODEL('', (), #2);` entity - what HOOPS Exchange exports -
/// rejected the whole file with a syntax error pointing at `DATA;`. The
/// fixture is box.step plus that one entity; the `[patch.crates-io]` pin in
/// the root Cargo.toml (upstream fix ricosjp/ruststep@90fdc935) is what makes
/// this pass - without it, `load_model` fails.
#[test]
fn step_file_with_empty_parameter_list_imports() {
    let m = import::load_model(&testdata("empty_list.step")).expect("must import");
    // Same box as box.step - the dead entity must not change the mesh.
    assert_eq!(m.len(), 12);
    let b = m.bounds().expect("bounds");
    assert!(b.min.x == 0.0 && b.max.z == 1.0, "{b:?}");
}

#[test]
fn step_garbage_is_clean_error_never_panic() {
    // Valid-syntax-but-wrong shapes: every one must Err (or at worst import
    // empty), none may panic, per the Prime Directive.
    let header_only = "ISO-10303-21;\nHEADER;\nENDSEC;\nEND-ISO-10303-21;\n";
    let no_data = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('a'),'2;1');\nFILE_NAME('','2020',(()),(()),'','','');\nFILE_SCHEMA(('AUTOMOTIVE_DESIGN'));\nENDSEC;\nEND-ISO-10303-21;\n";
    let not_utf8 = [0xFFu8, 0xFE, 0x00, 0x01];
    let empty_data = "ISO-10303-21;\nDATA;\nENDSEC;\nEND-ISO-10303-21;\n";
    for case in [
        import::parse_model(b"", Format::Step),
        import::parse_model(b"hello step", Format::Step),
        import::parse_model(header_only.as_bytes(), Format::Step),
        import::parse_model(no_data.as_bytes(), Format::Step),
        import::parse_model(empty_data.as_bytes(), Format::Step),
        import::parse_model(&not_utf8, Format::Step),
    ] {
        assert!(case.is_err(), "corrupt STEP must error, got Ok");
    }
}

#[test]
fn step_binary_garbage_never_panics() {
    // deterministic smoke-fuzz over the STEP parser
    for seed in 0..32u64 {
        let mut data = b"ISO-10303-21;DATA;".to_vec();
        let mut x = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        for _ in 0..256 {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            data.push((x >> 33) as u8);
        }
        let _ = import::parse_model(&data, Format::Step);
    }
}

#[test]
fn facet_cap_is_30_million() {
    // M5.3 E: the 10M+ roadmap models walk through this door.
    assert_eq!(MAX_FACETS, 30_000_000);
}

// ---------- helpers ----------

fn mesh_to_binary(m: &Mesh) -> Vec<u8> {
    let mut out = vec![0u8; 84];
    let n = m.facets.len() as u32;
    out[80..84].copy_from_slice(&n.to_le_bytes());
    for t in m.facets.iter() {
        let mut rec = [0u8; 50];
        for (pi, p) in [t.a, t.b, t.c].iter().enumerate() {
            for (ci, c) in [p.x, p.y, p.z].iter().enumerate() {
                let at = 12 + pi * 12 + ci * 4;
                rec[at..at + 4].copy_from_slice(&(*c as f32).to_le_bytes());
            }
        }
        out.extend_from_slice(&rec);
    }
    out
}

/// Parse error text for a STEP buffer (must be an error).
fn step_err(text: &str) -> String {
    match import::parse_model(text.as_bytes(), Format::Step) {
        Err(e) => e.to_string(),
        Ok(_) => String::new(),
    }
}

/// #912: one unparsable entity in the middle of DATA - the refusal names
/// that entity's `#id` and its real line, not `DATA;`.
#[test]
fn step_parse_error_names_the_bad_entity_and_line() {
    let path = testdata("bad_entity_mid_data.step");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let want = text.lines().position(|l| l.starts_with("#9 =")).map_or(0, |i| i + 1);
    let err = import::load_model(&path).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(err.contains("entity #9"), "{err}");
    assert!(err.contains(&format!("line {want}")), "want line {want}: {err}");
}

/// #912: a bad entity spanning lines, after strings and comments that hold
/// `;` and quotes, still reports its own first line.
#[test]
fn step_parse_error_line_survives_strings_and_comments() {
    let f = "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n/* a; comment\n over lines */\n#1 = A('it''s; ok');\n\n#2 = B(\n  1,,\n  2);\n#3 = C();\nENDSEC;\nEND-ISO-10303-21;\n";
    let err = step_err(f);
    assert!(err.contains("entity #2") && err.contains("line 9"), "{err}");
}

/// #912: unterminated string / comment and a file with no DATA section are
/// clean refusals naming what is wrong.
#[test]
fn step_parse_error_unterminated_and_no_data() {
    let s = step_err("ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1 = A('open);\n#2 = B();\nENDSEC;\nEND-ISO-10303-21;\n");
    assert!(s.contains("string") && s.contains("line 5") && s.contains("#1"), "{s}");
    let c = step_err("ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1 = A();\n/* never closed\n#2 = B();\n");
    assert!(c.contains("comment") && c.contains("line 6"), "{c}");
    let n = step_err("ISO-10303-21;\nHEADER;\nFILE_SCHEMA(('X'));\nENDSEC;\nEND-ISO-10303-21;\n");
    assert!(n.contains("DATA"), "{n}");
}
