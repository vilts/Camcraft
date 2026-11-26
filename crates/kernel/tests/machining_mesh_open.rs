//! The open machining meshes (#917): a sheet meshes with its own boundary
//! open; a solid with a face the mesher leaves out meshes partially with
//! that face named; a face wound against its neighbours in the file is
//! named in the read notes.
//! Provenance: our own test design; mutates `box.step` in memory.
//!
//! Gate map (#917): covers camcraft_kernel::machining_mesh_open,
//! camcraft_kernel::machining_mesh, camcraft_kernel::solidcore_step_import,
//! camcraft_kernel::step_face_winding.

use std::path::PathBuf;

use camcraft_kernel::machining_mesh::{MeshRefusal, machining_mesh};
use camcraft_kernel::machining_mesh_open::{partial_mesh, sheet_mesh};
use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_step::{import_step, import_step_model};
use solidcore::kernel::CancelToken;
use solidcore_record::brep::{Curve2, Loop};

const TOL: f64 = 0.005;

fn box_text() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/box.step");
    std::fs::read_to_string(path).expect("box.step")
}

/// `box.step` with its top face taken off, kept as a surface model.
fn open_box() -> String {
    let text = box_text();
    let solid = "#16 = MANIFOLD_SOLID_BREP('', #17);";
    let shell = "#17 = CLOSED_SHELL('', (#18, #25, #32, #39, #46, #53));";
    assert!(text.contains(solid) && text.contains(shell), "fixture layout changed");
    text.replace(solid, "#16 = SHELL_BASED_SURFACE_MODEL('', (#17));")
        .replace(shell, "#17 = OPEN_SHELL('', (#18, #25, #32, #39, #46));")
}

#[test]
fn a_sheet_meshes_with_its_boundary_open() {
    let stop = CancelToken::new();
    let model = import_step_model(&mut SolidcoreKernel::default(), open_box().as_bytes(), &stop)
        .unwrap_or_else(|e| panic!("a surface model imports as a sheet: {e}"));
    assert!(model.solids.is_empty(), "no solid in a surface model");
    assert_eq!(model.sheets.len(), 1, "one sheet");
    let m = sheet_mesh(&model.sheets[0].record, TOL, &stop).expect("the sheet meshes");
    assert_eq!(m.report.faces, 5);
    assert_eq!(m.report.volume, 0.0, "an open mesh encloses no volume");
    // The missing face's four sides are open, one triangle side or more each.
    assert!(m.report.open_sides >= 4, "{:?}", m.report.open_sides);
    let mut has = [false; 5];
    m.tri_face.iter().for_each(|f| has[*f as usize] = true);
    assert!(has.iter().all(|h| *h), "every face of the sheet has triangles");
}

#[test]
fn the_strict_import_still_refuses_a_sheet_by_name() {
    let stop = CancelToken::new();
    let err = import_step(&mut SolidcoreKernel::default(), open_box().as_bytes(), &stop)
        .expect_err("import_step hands out solids only");
    assert!(err.to_string().contains("a surface model of open shells"), "{err}");
}

#[test]
fn a_face_left_out_meshes_partially_and_is_named() {
    let stop = CancelToken::new();
    let mut import = import_step(&mut SolidcoreKernel::default(), box_text().as_bytes(), &stop).expect("box.step");
    let mut s = import.solids.remove(0);
    // As machining_mesh_shortfalls: a pcurve of face 0 shifted along u past
    // what the planner accepts, so face 0 gets no triangles.
    let f0 = &mut s.record.faces[0];
    let Loop::Edges { edges } = &mut f0.loops[0] else { panic!("expected edge loop") };
    match &mut edges[0].pcurve {
        Curve2::Spline { points, .. } => points.iter_mut().for_each(|p| p[0] += 0.5),
        Curve2::RationalSpline { homogeneous, .. } => homogeneous.iter_mut().for_each(|p| p[0] += 0.5 * p[2]),
    }
    f0.domain[0][1] += 0.5;
    let strict = machining_mesh(&s, TOL, &stop);
    assert!(matches!(strict, Err(MeshRefusal::FaceLeftOut { face: 0, .. })), "the strict mesh refuses: {strict:?}");
    let open = partial_mesh(&s, TOL, &stop).expect("the partial mesh");
    assert_eq!(open.left_out.len(), 1, "{:?}", open.left_out);
    assert_eq!(open.left_out[0].face, 0);
    assert!(!open.left_out[0].reason.is_empty(), "the reason is said");
    assert!(open.mesh.report.open_sides >= 4, "the hole is open: {}", open.mesh.report.open_sides);
    assert!(open.mesh.tri_face.iter().all(|f| *f != 0), "no triangle on the left-out face");
    assert_eq!(open.mesh.report.volume, 0.0);
}

#[test]
fn a_face_wound_against_its_neighbours_is_named_once() {
    let stop = CancelToken::new();
    let good = import_step(&mut SolidcoreKernel::default(), box_text().as_bytes(), &stop).expect("box.step");
    assert!(good.notes.iter().all(|n| !n.contains("wound")), "the good box says nothing: {:?}", good.notes);
    let text = box_text();
    let face = "#32 = FACE_SURFACE('', (#33), #90, .T.);";
    let bound = "#33 = FACE_BOUND('', #34, .T.);";
    assert!(text.contains(face) && text.contains(bound), "fixture layout changed");
    let bad = text
        .replace(face, "#32 = FACE_SURFACE('', (#33), #90, .F.);")
        .replace(bound, "#33 = FACE_BOUND('', #34, .F.);");
    let read = import_step(&mut SolidcoreKernel::default(), bad.as_bytes(), &stop).expect("a reversed face imports");
    let wound: Vec<&String> = read.notes.iter().filter(|n| n.contains("wound")).collect();
    assert_eq!(wound.len(), 1, "{:?}", read.notes);
    assert!(
        wound[0].contains("shell #17: 1 face(s) wound against their neighbours were flipped: face #32 (3 of 6)"),
        "{wound:?}"
    );
}

/// `box.step` with its first shell member #18 put behind `extra` (entity
/// lines added to DATA) through the member id `via`.
fn box_with_member(via: u64, extra: &str) -> String {
    let text = box_text();
    let shell = "#17 = CLOSED_SHELL('', (#18, #25, #32, #39, #46, #53));";
    assert!(text.contains(shell), "fixture layout changed");
    let text = text.replace(shell, &format!("#17 = CLOSED_SHELL('', (#{via}, #25, #32, #39, #46, #53));"));
    let (head, tail) = text.rsplit_once("ENDSEC;").expect("DATA ends");
    format!("{head}{extra}\nENDSEC;{tail}")
}

/// Corrupt input must return (a refusal or a defined read), never recurse
/// until the stack overflows (#2904 review): no winding note is made.
fn returns_without_a_winding_note(text: &str) {
    let stop = CancelToken::new();
    if let Ok(read) = import_step(&mut SolidcoreKernel::default(), text.as_bytes(), &stop) {
        assert!(read.notes.iter().all(|n| !n.contains("wound")), "{:?}", read.notes);
    }
}

#[test]
fn an_oriented_face_that_names_itself_returns_without_a_note() {
    returns_without_a_winding_note(&box_with_member(900, "#900 = ORIENTED_FACE('', *, #900, .T.);"));
}

#[test]
fn an_oriented_face_chain_past_the_limit_returns_without_a_note() {
    // 64 wrappers: #900 -> #901 -> ... -> #963 -> #18.
    let chain: Vec<String> = (900..964)
        .map(|id| {
            let next = if id == 963 { 18 } else { id + 1 };
            format!("#{id} = ORIENTED_FACE('', *, #{next}, .T.);")
        })
        .collect();
    returns_without_a_winding_note(&box_with_member(900, &chain.join("\n")));
}
