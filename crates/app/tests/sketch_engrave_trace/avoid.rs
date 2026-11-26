//! #1869 with #3349: "Do not touch" judges sketch strokes for Trace and Engrave.
//! Provenance: original guard block beside the placed open stroke.

use super::{KINDS, OP, TOP, add_op, generated, job, open_pick, params};

use camcraft_app::commands::{GenerateOperation, ImportModel, SetOperationDoNotTouch};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::operation::{DoNotTouch, FeatureSel, GeoSel, SrcRef};
use camcraft_app::{model, posting};

const GUARD: &str = "guard";

#[test]
fn do_not_touch_refuses_a_sketch_stroke_that_passes_too_near_a_protected_face() {
    for kind in KINDS {
        let (world, mut history) = job(TOP);
        // The open stroke's line runs at x=7 from y=10 to y=18. The 4 mm
        // cutter's edge stays at x=9, 0.5 mm clear of the guard at x=9.5.
        let block = cube(Point3::new(9.5, 12.0, 0.0), Point3::new(12.0, 16.0, TOP));
        history.execute(&world, ImportModel::from_mesh(GUARD, block));
        let src = SrcRef::Selection(vec![open_pick(&world)]);
        add_op(&world, &mut history, src, params(kind, true, 0.5, 0.5));

        // Without protection the stroke cuts and nothing blocks.
        let free = generated(&world, &mut history);
        assert!(free.warnings.iter().all(|w| !w.blocks_post()), "{kind:?}: {:?}", free.warnings);
        assert!(
            free.path.moves.iter().any(|m| (m.from.x - 7.0).abs() < 1e-6
                && (m.to.x - 7.0).abs() < 1e-6
                && m.from.y.min(m.to.y) <= 12.0
                && m.from.y.max(m.to.y) >= 16.0
                && m.to.z < TOP),
            "{kind:?}: the stroke must pass the guard"
        );

        // A 1 mm clearance asks for 3 mm from the tool axis; the guard
        // is 2.5 mm away. Generation refuses by name and nothing posts.
        let entity = model::find_by_name(&world, GUARD).expect("guard model");
        let features = model::features_of(&world, &entity).expect("guard faces");
        let picks = features
            .faces
            .iter()
            .map(|f| GeoSel { entity: GUARD.into(), feature: FeatureSel::Face(f.id) })
            .collect();
        let dnt = DoNotTouch { clearance: 1.0, picks };
        history.execute(&world, SetOperationDoNotTouch::new(&world, OP, dnt).expect("protect the guard"));
        let why = GenerateOperation::new(&world, OP)
            .err()
            .expect("a protected guard beside a sketch stroke must refuse")
            .to_string();
        assert!(why.contains("Do not touch") && why.contains(GUARD), "{kind:?}: {why}");
        let err = posting::post_setup_unwritten(&world).expect_err("a protected guard blocks posting");
        assert!(err.contains(OP), "{kind:?}: {err}");
    }
}
