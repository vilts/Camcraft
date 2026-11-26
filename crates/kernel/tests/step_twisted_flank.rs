//! #2780 at the kernel: a closed solid with one strongly twisted B-spline
//! flank, read by the solidcore STEP reader (#917). The exact model comes
//! out closed at its true volume; the variant with a vertex lifted off its
//! curves is refused by name, and so is a file whose edges stand 0.25 mm
//! off their faces. Never a solid with a wall missing.
//! The two twisted-flank files do not reach the kernel's own pcurves
//! (`camcraft_kernel::solidcore_trims`, KernelTrims): the reader makes their
//! pcurves itself. Here KernelTrims runs only on bspline_discontinuous.step
//! (the cap on an edge's gap); step_closed_form.rs (fillet_corner,
//! bar_cross_holes) pins the route through it.
//! Fixtures (our own, testdata/models/): `twisted_flank.step` (volume 8000
//! mm3, top at z = 60) and `twisted_flank_off.step` (one vertex 0.5 mm off
//! its curves and its surface).
//! Provenance: our own fixtures, written from ISO 10303-21/-42.
//!
//! Gate map (M12.1): covers camcraft_kernel::solidcore_step (import_step),
//! and camcraft_kernel::solidcore_trims (KernelTrims, the gap cap only).

mod step_util;

use std::path::PathBuf;

use camcraft_kernel::KernelError;
use camcraft_kernel::solidcore_impl::SolidcoreKernel;
use camcraft_kernel::solidcore_step::import_step;
use step_util::{CASE_TIMEOUT, bounded_cancel};

/// What one import gives: per solid its volume, bbox min z and max z.
type Solids = Vec<(f64, f64, f64)>;

fn import(file: &'static str) -> Result<Solids, KernelError> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models")
        .join(file);
    let bytes = std::fs::read(&path).unwrap_or_default();
    assert!(!bytes.is_empty(), "fixture {file} is missing");
    bounded_cancel(file, CASE_TIMEOUT, move |stop| {
        let mut k = SolidcoreKernel::default();
        import_step(&mut k, &bytes, stop).map(|i| {
            i.solids
                .iter()
                .map(|s| (s.measure.volume, s.measure.min[2], s.measure.max[2]))
                .collect()
        })
    })
}

/// A refusal names what it refused: an entity (`#n`), a face or a solid.
fn named(e: &KernelError) -> bool {
    let s = e.to_string();
    !s.trim().is_empty() && (s.contains('#') || s.contains("face") || s.contains("solid"))
}

#[test]
fn a_twisted_bspline_flank_reads_closed_at_its_volume() {
    let solids = match import("twisted_flank.step") {
        Ok(s) => s,
        Err(e) => panic!("twisted_flank.step must read through the kernel pcurves: {e}"),
    };
    assert_eq!(solids.len(), 1, "one solid: {solids:?}");
    for (v, zmin, zmax) in solids {
        // Exact measure of a closed B-rep: the port's integrator, not a mesh.
        // 8000 = 32000 - 24000 over the 40 x 40 base: the solid lies under
        // the planes z = 1.5 * min(x, y) (32000 mm3 under them) and over the
        // twisted patch z = 3xy/80 (24000 mm3 under it).
        assert!((v - 8000.0).abs() <= 0.8, "volume {v} mm3, want 8000");
        assert!(zmin.abs() <= 1e-6, "bottom at z {zmin}");
        assert!((zmax - 60.0).abs() <= 1e-3, "top at z {zmax}");
    }
}

#[test]
fn a_twisted_flank_with_a_vertex_off_its_curves_is_refused_by_name() {
    // Observed (#917): the flank's pcurve would have to move the surface by
    // up to 0.89 mm to reach the vertex, over the reach the file gives it.
    // Since solidcore fa67b3f5 (#3767) the reader builds the pcurves, and
    // its vertex check refuses the solid: the vertex is 0.343 mm off its
    // edges, over the 1e-3 mm limit.
    match import("twisted_flank_off.step") {
        Ok(solids) => panic!("twisted_flank_off.step must be refused, read {solids:?}"),
        Err(e) => {
            assert!(named(&e), "a refusal names its fault: {e}");
            let s = e.to_string();
            assert!(
                s.contains("#117 at check: a vertex sits"),
                "refusal: {s}"
            );
            assert!(s.contains("mm off its edges"), "refusal: {s}");
        }
    }
}

#[test]
fn an_edge_a_quarter_millimetre_off_its_face_is_refused_by_name() {
    // bspline_discontinuous.step (#1697): edges 0.25 mm off their faces'
    // surfaces, over `solidcore_trims::MAX_EDGE_GAP_MM`. The reader's own
    // 1 mm reach would keep it as a closed solid of the wrong shape.
    match import("bspline_discontinuous.step") {
        Ok(solids) => panic!("bspline_discontinuous.step must be refused, read {solids:?}"),
        Err(e) => {
            let s = e.to_string();
            assert!(s.contains("#16 at read: pcurves failed on"), "refusal: {s}");
            assert!(s.contains("face #53"), "refusal names the face: {s}");
            assert!(s.contains("edge 0 lies"), "refusal names the edge: {s}");
            assert!(
                s.contains("off the face's surface, over the cap of 0.01 mm"),
                "refusal: {s}"
            );
        }
    }
}
