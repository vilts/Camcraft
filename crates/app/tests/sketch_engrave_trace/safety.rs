//! Blocking verification and input refusals for #1869.
//! Provenance: original fixtures over CamCraft's stock and mark laws.

use super::{DXF, KINDS, OP, PROFILE, TOOL, TOP, add_op, drawing_job, generated, job, layer, open_pick, params};
use flecs_ecs::prelude::World;

use camcraft_app::commands::{CreateOperation, GenerateOperation, History, ImportModel, TagFixture};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::verify::Warning;
use camcraft_app::{model, posting, sketch_model};

fn refused_mark(world: &World, history: &mut History) -> String {
    let generation = GenerateOperation::new(world, OP);
    let reason = match generation {
        Ok(cmd) => {
            history.execute(world, cmd);
            let entity = model::find_by_name(world, OP).expect("the refused operation");
            let cache = model::toolpath_of(world, &entity).expect("warned path");
            assert!(
                cache.warnings.iter().any(|w| w.blocks_post()),
                "unsafe path had no blocking warning"
            );
            cache.path.notes.join("\n")
        }
        Err(e) => e.to_string(),
    };
    let err = posting::post_setup_unwritten(world).expect_err("unsafe marks must never post");
    assert!(err.contains(OP), "{err}");
    format!("{reason}\n{err}")
}

#[test]
fn sketch_marks_deeper_than_the_cutter_radius_or_through_stock_do_not_post() {
    for kind in KINDS {
        for (height, depth, expected) in [(TOP, 2.2, "mark"), (0.4, 0.5, "through")] {
            let (world, mut history) = job(height);
            let src = SrcRef::Selection(vec![open_pick(&world)]);
            add_op(&world, &mut history, src, params(kind, true, depth, 0.25));
            let why = refused_mark(&world, &mut history);
            assert!(why.to_lowercase().contains(expected), "{kind:?}: {why}");
        }
    }
}

#[test]
fn a_fixture_in_a_sketch_mark_is_a_blocking_collision() {
    for kind in KINDS {
        let (world, mut history) = job(TOP);
        let clamp = cube(Point3::new(5.0, 14.0, 5.8), Point3::new(8.0, 16.0, 12.0));
        history.execute(&world, ImportModel::from_mesh("clamp", clamp));
        history.execute(&world, TagFixture::new(&world, "clamp", None).expect("tag clamp"));
        assert!(!sketch_model::has_part(&world));
        let src = SrcRef::Selection(vec![open_pick(&world)]);
        add_op(&world, &mut history, src, params(kind, true, 0.5, 0.5));
        let mark = generated(&world, &mut history);
        assert!(
            mark.warnings
                .iter()
                .any(|w| matches!(w, Warning::FixtureHit { .. }) && w.blocks_post()),
            "{:?}",
            mark.warnings
        );
        let err = posting::post_setup_unwritten(&world).expect_err("fixture blocks posting");
        assert!(err.contains(OP) && err.contains("fixture"), "{err}");
    }
}

#[test]
fn stale_selection_and_invalid_stepdown_do_not_generate_a_sketch_mark() {
    for kind in KINDS {
        let (world, mut history) = job(TOP);
        let src = SrcRef::Selection(vec![layer("missing-layer")]);
        let create = CreateOperation::new(&world, OP, TOOL, src, params(kind, true, 0.5, 0.5));
        let why = match create {
            Ok(cmd) => {
                history.execute(&world, cmd);
                GenerateOperation::new(&world, OP)
                    .err()
                    .expect("missing layer must refuse")
                    .to_string()
            }
            Err(e) => e.to_string(),
        };
        assert!(why.contains("missing-layer") && why.contains(PROFILE), "{why}");

        let src = SrcRef::Entity(PROFILE.into());
        let why = CreateOperation::new(&world, "Bad depth", TOOL, src, params(kind, true, 0.5, 0.0))
            .err()
            .expect("zero stepdown must refuse when the command is built")
            .to_string();
        assert!(
            why.contains("operation parameters are not usable") && why.contains(kind.info().label),
            "{why}"
        );
        assert!(model::find_by_name(&world, "Bad depth").is_none());
    }
}

#[test]
fn a_lateral_rapid_below_the_stock_top_does_not_post() {
    for kind in KINDS {
        let (world, mut history) = job(TOP);
        let mut cut = params(kind, true, 0.5, 0.5);
        match &mut cut {
            OpParams::Engrave { clearance_z, top_z, .. } | OpParams::Trace { clearance_z, top_z, .. } => {
                *clearance_z = Height::fixed(5.95);
                *top_z = Height::fixed(5.9);
            }
            _ => unreachable!(),
        }
        let src = SrcRef::Selection(vec![layer("open"), layer("closed")]);
        add_op(&world, &mut history, src, cut);
        let why = refused_mark(&world, &mut history).to_lowercase();
        assert!(
            why.contains("stock") && (why.contains("clearance") || why.contains("rapid") || why.contains("traverse")),
            "{why}"
        );
    }
}

#[test]
fn crossing_selected_strokes_keep_the_crossing_check() {
    // The lines intersect in their interiors and are on separate layers,
    // so the importer can build a chain for each without joining them.
    let drawing = DXF.replace(
        "0\nENDSEC\n0\nEOF",
        "0\nLINE\n8\ncrossing\n10\n6\n20\n7\n11\n6\n21\n11\n0\nENDSEC\n0\nEOF",
    );
    for kind in KINDS {
        let (world, mut history) = drawing_job(TOP, &drawing);
        let src = SrcRef::Selection(vec![layer("open"), layer("crossing")]);
        add_op(&world, &mut history, src, params(kind, true, 0.5, 0.5));
        let mark = generated(&world, &mut history);
        assert!(
            mark.warnings
                .iter()
                .any(|w| matches!(w, Warning::Crossing { .. }) && w.blocks_post()),
            "{:?}",
            mark.warnings
        );
        let err = posting::post_setup_unwritten(&world).expect_err("crossings block posting");
        assert!(err.contains(OP) && err.contains("cross"), "{err}");
    }
}
