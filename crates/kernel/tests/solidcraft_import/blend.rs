//! A Solidcraft design whose final body has a fillet (#2578): the body's
//! record holds a `blend` surface kind (Solidcraft 0.44 writes it). The
//! fixture is the sample design with its final body replaced by a block
//! with one variable round, made and encoded with the pinned solidcore
//! crates (`solidcore-ops`, `native::encode`). The import must keep the
//! body that the modeller made: its blend face, its volume and its box.
//! Provenance: our own fixture, built by the test.

use camcraft_kernel::machining_mesh::machining_mesh;
use serde_json::json;
use solidcore::kernel::{CancelToken, UpstreamFixes};
use solidcore_record::brep::{Brep as Record, Surface, envelope};
use solidcore_record::native::{self, Runtime, Saved};

use super::{import, last_slice};
use crate::sc_util::Parts;

/// The 10 x 20 x 30 block with its top edge at x = 10 rounded by a radius
/// law from 2 mm (at y = 0) to 5 mm (at y = 20): a variable round, which
/// the record keeps as a procedural blend definition, as Solidcraft writes
/// its fillets.
fn filleted_block() -> (Record, f64) {
    solidcore_ops::session::set_thread_fixes(UpstreamFixes::NAMES.iter().copied())
        .expect("every fix on, as Solidcraft runs");
    let b = solidcore_ops::make_box([0.0; 3], [10.0, 20.0, 30.0]).expect("block");
    let (record, order) = b.to_record().expect("record of the block");
    let at = |i: u32| record.vertices[i as usize].point;
    let edge = record
        .edges
        .iter()
        .position(|e| {
            let (p, q) = (at(e.vertices[0]), at(e.vertices[1]));
            p[0] == 10.0 && q[0] == 10.0 && p[2] == 30.0 && q[2] == 30.0
        })
        .expect("the top edge at x = 10");
    let law = ([0.0, 0.0, 1.0, 1.0], [2.0, 5.0]);
    solidcore_ops::fillet_variable_ordered(&b, &order, edge, [10.0, 0.0, 30.0], 1, &law.0, &law.1)
        .expect("round the edge");
    let volume = b.volume(1e-8).expect("volume of the filleted block");
    let (record, _) = b.to_record().expect("record of the filleted block");
    (record, volume)
}

/// The sample design with `record` as its final body, re-keyed.
fn design_with(record: Record) -> Vec<u8> {
    let saved = Saved {
        solid: record,
        metadata: envelope::Metadata {
            version: envelope::VERSION,
            color: None,
            paint: None,
            boundaries: None,
            face_sources: None,
            history: None,
            preserve_boundaries: false,
        },
        runtime: Runtime {
            size: 0.0,
            coarse_missing: None,
        },
    };
    let body = native::encode(&saved).expect("encode the filleted body");
    let mut p = Parts::sample();
    let (at, _) = last_slice(&p);
    let mut payload = p.get("cache/bodies.bin");
    payload.truncate(at);
    payload.extend_from_slice(&body);
    p.set("cache/bodies.bin", payload);
    p.manifest(|m| m["slices"][13][1] = json!(body.len()));
    p.rekey();
    p.build()
}

#[test]
fn a_design_with_a_fillet_imports_its_blend_body() {
    let (record, volume) = filleted_block();
    let blends = |r: &Record| {
        r.faces
            .iter()
            .filter(|f| matches!(f.surface, Surface::Blend { .. }))
            .count()
    };
    assert!(blends(&record) > 0, "the fixture must carry a blend face");
    let design = design_with(record);
    let i = match import(&design) {
        Ok(i) => i,
        Err(e) => panic!("the filleted design is refused: {e}"),
    };
    assert!(
        blends(&i.solid.record) > 0,
        "the imported record keeps its blend"
    );
    // The round takes material: the volume is the modeller's, below the box's.
    let got = i.solid.measure.volume;
    assert!(got < 6000.0, "volume {got} is not below the block's");
    assert!(
        (got - volume).abs() < 1e-6 * volume,
        "volume {got}, modeller {volume}"
    );
    // The kernel's box encloses the block, and can be a little too large
    // on a blend's net (as on trimmed cylinders, #939).
    let (lo, hi) = (i.solid.measure.min, i.solid.measure.max);
    let want = ([0.0; 3], [10.0, 20.0, 30.0]);
    for a in 0..3 {
        assert!(
            lo[a] <= want.0[a] + 1e-9 && lo[a] > want.0[a] - 1e-3,
            "min {lo:?}"
        );
        assert!(
            hi[a] >= want.1[a] - 1e-9 && hi[a] < want.1[a] + 1e-3,
            "max {hi:?}"
        );
    }
    // The machining mesh (what toolpaths read) takes the blend face too.
    let mesh = match machining_mesh(&i.solid, 0.005, &CancelToken::new()) {
        Ok(m) => m,
        Err(e) => panic!("the filleted body does not mesh: {e}"),
    };
    assert_eq!(mesh.report.open_sides, 0);
    assert!(
        mesh.report.deviation.largest() <= 0.005,
        "{:?}",
        mesh.report.deviation
    );
    assert!(
        (mesh.report.volume - volume).abs() < 1e-3 * volume,
        "{:?}",
        mesh.report
    );
}
