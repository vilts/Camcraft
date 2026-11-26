//! STEP short edges (#3252). The front bottom edge of the unit box is split
//! at x = 0.0007 mm. Its long segment is a clamped cubic spline. Both
//! faces share the segments. Each split endpoint is offset in Y.
//! Provenance: our own modification of box.step, ISO 10303-21 and -42.
//! Gate map (M4.4): covers camcraft_kernel::solidcore_step (import_step,
//! build_record) and camcraft_kernel::solidcore_import (vertex_gaps).

use camcraft_kernel::machining_mesh::machining_mesh;
use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_step::import_step;
use solidcore::kernel::CancelToken;

const SPLIT_X: f64 = 0.0007;
const BOX: &str = include_str!("../../../testdata/models/box.step");

/// Offset the two topological vertices normal to the split edge. Keep the
/// adjoining curves closed at their geometric endpoints. Use a bounded
/// spline for #122 so its trim ends at the geometric corner of the box.
fn split_box(vertex_offset: f64, spline_start: f64) -> String {
    let vertex_y = -vertex_offset;
    let entities = format!(
        "#166 = CARTESIAN_POINT('', ({SPLIT_X},{vertex_y},0.));
#167 = VERTEX_POINT('', #166);
#168 = CARTESIAN_POINT('', ({spline_start},0.,0.));
#169 = EDGE_CURVE('', #167,#73,#172,.T.);
#170 = ORIENTED_EDGE('',*,*,#169,.T.);
#171 = ORIENTED_EDGE('',*,*,#169,.T.);
#172 = B_SPLINE_CURVE_WITH_KNOTS('',3,(#168,#173,#174,#159),.UNSPECIFIED.,.F.,.F.,(4,4),(0.,1.),.UNSPECIFIED.);
#173 = CARTESIAN_POINT('', (0.34,0.,0.));
#174 = CARTESIAN_POINT('', (0.67,0.,0.));"
    );
    BOX.replace("1.0E-6, #12", "0.01, #12")
        .replace("#158 = CARTESIAN_POINT('', (0.0, 0.0, 0.0));", &format!("#158 = CARTESIAN_POINT('', (0.0, {vertex_y}, 0.0));"))
        .replace("#122 = LINE('', #123, #124);", "#122 = B_SPLINE_CURVE_WITH_KNOTS('',1,(#123,#111),.UNSPECIFIED.,.F.,.F.,(2,2),(0.,1.),.UNSPECIFIED.);")
        .replace("#72, #73, #110", "#72, #167, #110")
        .replace("#21, #22", "#21, #170, #22")
        .replace("#28, #29", "#28, #171, #29")
        .replace(
            "#165 = CARTESIAN_POINT",
            &format!("{entities}\n#165 = CARTESIAN_POINT"),
        )
}

#[test]
fn preserve_short_edge_when_healing_would_make_the_gap_exceed_the_limit() {
    let gap = 0.0008;
    let bytes = split_box(gap, SPLIT_X);
    let stop = CancelToken::new();
    let imported = import_step(&mut SolidcoreKernel::default(), bytes.as_bytes(), &stop)
        .expect("the split box has a source gap below the import limit");
    let [solid] = imported.solids.as_slice() else {
        panic!("expected one solid");
    };
    assert_eq!(solid.record.edges.len(), 13);
    assert_eq!(
        solid.brep.edges().iter().count(),
        13,
        "the short edge was lost"
    );
    assert_eq!(solid.brep.vertices().iter().count(), 9);
    assert!(
        (solid.vertex_gap - gap).abs() < 1e-12,
        "{}",
        solid.vertex_gap
    );
    assert!(
        solid
            .notes
            .iter()
            .any(|note| note.contains("before healing: 13 edges")
                && note.contains("after healing: 12 edges")),
        "{:?}",
        solid.notes
    );
    let chord = 0.005;
    let mesh = machining_mesh(solid, chord, &stop).expect("split box meshes at the import chord");
    assert!(!mesh.triangles.is_empty());
    assert!(mesh.report.deviation.largest() <= chord);
}

#[test]
fn still_accept_a_healed_box_whose_gap_meets_the_limit() {
    let bytes = split_box(0.00055, SPLIT_X);
    let imported = import_step(
        &mut SolidcoreKernel::default(),
        bytes.as_bytes(),
        &CancelToken::new(),
    )
    .expect("the healed box already met the limit");
    let [solid] = imported.solids.as_slice() else {
        panic!("expected one solid");
    };
    assert_eq!(solid.brep.edge_list().len(), 12, "retain the healed body");
    assert!(solid.vertex_gap <= 0.001);
    assert!(
        !solid
            .notes
            .iter()
            .any(|note| note.starts_with("Kept the original STEP edges")),
        "unexpected source recovery: {:?}",
        solid.notes
    );
}

#[test]
fn refuse_short_edge_model_when_its_source_gap_exceeds_the_limit() {
    // Only the start of the long spline (STEP edge #169, input record edge 3)
    // has both the 0.0012 mm X gap and the 0.0005 mm Y offset: 0.0013 mm.
    let bytes = split_box(0.0005, SPLIT_X + 0.0012);
    let reason = import_step(
        &mut SolidcoreKernel::default(),
        bytes.as_bytes(),
        &CancelToken::new(),
    )
    .expect_err("the source gap exceeds the existing 0.001 mm limit")
    .to_string();
    assert!(
        reason.contains("The source endpoint gap is 0.001300 mm, above the 0.001 mm limit."),
        "name the check that refused the source: {reason}"
    );
    assert!(
        reason.contains(
            "Input-record edge index 3 (zero-based), vertex (0.000700, -0.000500, 0.000000) mm"
        ),
        "locate the source edge and the affected vertex: {reason}"
    );
    assert!(
        reason.contains("after healing: 12 edges, maximum gap 0.001965 mm"),
        "name the healed gap: {reason}"
    );
}

#[test]
fn refuse_an_unhealed_source_with_an_open_gap_between_curves() {
    // Recede the long spline from its neighbour. The source vertices still
    // pass the endpoint limit, but the two curves now end 0.0008 mm apart.
    let bytes = split_box(0., SPLIT_X + 0.0008);
    let reason = import_step(
        &mut SolidcoreKernel::default(),
        bytes.as_bytes(),
        &CancelToken::new(),
    )
    .expect_err("an unhealed source must pass the full validity check")
    .to_string();
    assert!(reason.contains("source validation failed"), "{reason}");
    assert!(reason.contains("Edge/Edge Gap"), "{reason}");
}
