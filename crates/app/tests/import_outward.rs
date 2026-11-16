//! Provenance: hand-built cubes and public STL/OBJ grammars; issue #1672.
//! Gate map (M4.4): covers app::geometry::import.
use camcraft_app::geometry::import::{self, Format};
use camcraft_app::geometry::{cube, Mesh, Point3, Triangle};

fn check(format: Format) {
    let outward = cube(Point3::ZERO, Point3::new(10.0, 10.0, 10.0));
    let input = Mesh::from_facets(outward.facets.iter().map(|t| Triangle::new(t.a, t.c, t.b)).collect());
    let volume = |m: &Mesh| m.facets.iter().map(|t| t.a.dot(t.b.cross(t.c)) / 6.0).sum::<f64>();
    assert!(volume(&input) < 0.0);
    let text = serialize(&input, format);
    let imported = import::parse_model_graded(text.as_bytes(), format).expect("import");
    assert!(volume(&imported.mesh) > 0.0, "inward body must turn outward");
    assert!(imported.notes.iter().any(|n| n.contains("turned outward")), "{:?}", imported.notes);
    let ext = if format == Format::Stl { "stl" } else { "obj" };
    let path = std::env::temp_dir().join(format!("camcraft-1672-{}.{ext}", std::process::id()));
    std::fs::write(&path, text).expect("fixture");
    let loaded = import::load_model_graded(&path).expect("disk import");
    std::fs::remove_file(path).expect("cleanup");
    assert_eq!(loaded.mesh, imported.mesh);
    assert_eq!(loaded.notes, imported.notes);
}

fn serialize(mesh: &Mesh, format: Format) -> String {
    let mut text = String::new();
    if format == Format::Stl { text.push_str("solid inward\n"); }
    let mut verts = Vec::new();
    for t in mesh.facets.iter() {
        let mut refs = Vec::new();
        if format == Format::Stl { text.push_str("facet normal 0 0 0\nouter loop\n"); }
        for p in [t.a, t.b, t.c] {
            let word = if format == Format::Stl { "vertex" } else { "v" };
            if format == Format::Stl {
                text.push_str(&format!("{word} {} {} {}\n", p.x, p.y, p.z));
            } else {
                let idx = match verts.iter().position(|v| *v == p) {
                    Some(i) => i,
                    None => {
                        verts.push(p);
                        text.push_str(&format!("v {} {} {}\n", p.x, p.y, p.z));
                        verts.len() - 1
                    }
                };
                refs.push(idx + 1);
            }
        }
        if format == Format::Stl {
            text.push_str("endloop\nendfacet\n");
        } else {
            text.push_str(&format!("f {} {} {}\n", refs[0], refs[1], refs[2]));
        }
    }
    if format == Format::Stl { text.push_str("endsolid inward\n"); }
    text
}

#[test]
fn inward_stl_import_turns_outward_and_reports_it() { check(Format::Stl); }

#[test]
fn inward_obj_import_turns_outward_and_reports_it() { check(Format::Obj); }

fn check_nested(format: Format) {
    let outer = cube(Point3::ZERO, Point3::new(10.0, 10.0, 10.0));
    let cavity = cube(Point3::new(3.0, 3.0, 3.0), Point3::new(7.0, 7.0, 7.0));
    let mesh = Mesh::from_facets(outer.facets.iter().copied().chain(
        cavity.facets.iter().map(|t| Triangle::new(t.a, t.c, t.b))
    ).collect());
    let volume: f64 = mesh.facets.iter().map(|t| t.a.dot(t.b.cross(t.c)) / 6.0).sum();
    assert!((volume - 936.0).abs() < 1e-9, "valid cavity fixture");
    let text = serialize(&mesh, format);
    let buffered = import::parse_model_graded(text.as_bytes(), format);
    let ext = if format == Format::Stl { "stl" } else { "obj" };
    let path = std::env::temp_dir().join(format!("camcraft-1686-nested-{}.{ext}", std::process::id()));
    std::fs::write(&path, text).expect("fixture");
    let disk = import::load_model_graded(&path);
    std::fs::remove_file(path).expect("cleanup");
    for result in [buffered, disk] {
        let error = result.expect_err("nested shells must be refused rather than reversing cavity normals");
        assert!(error.to_string().contains("nested or overlapping shells"), "{error}");
    }
}

#[test]
fn nested_stl_cavity_is_refused_before_normalization() { check_nested(Format::Stl); }

#[test]
fn nested_obj_cavity_is_refused_before_normalization() { check_nested(Format::Obj); }
