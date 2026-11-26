//! Provenance: original catalog checks for the roughing and finishing workflow in #2860.
//! Gate map: covers app::project, app::setup, app::world, app::model, app::operation.

use std::{fs, path::Path};

use camcraft_app::operation::{Height, OpParams};
use camcraft_app::{model, project, setup, world};

#[test]
fn adaptive_pocket_case_roughs_above_the_finished_floor_then_finishes() {
    let case = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/catalog/2d/adaptive-pocket");
    let nc = fs::read_to_string(case.join("adaptive-pocket.nc"))
        .expect("#2860 requires the adaptive-pocket catalog program");
    let w = world::new_world();
    let notes = project::load_noted(&w, &case.join("adaptive-pocket.camproj"))
        .expect("the adaptive-pocket project must open");
    assert!(notes.is_empty(), "project load notes: {notes:?}");
    let ops = setup::operation_order(&w);
    let names: Vec<_> = ops.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["Adaptive rough", "Pocket finish"]);
    let rough = model::operation_of(&w, &ops[0].1).expect("Adaptive operation");
    let finish = model::operation_of(&w, &ops[1].1).expect("Pocket operation");
    match rough.params {
        OpParams::Adaptive2D { stock_to_leave, bottom_z, .. } => {
            assert_eq!(stock_to_leave, 0.5, "roughing leaves 0.5 mm on the walls");
            assert_eq!(bottom_z, Height::fixed(6.5), "roughing leaves 0.5 mm on the floor");
        }
        _ => panic!("roughing must use 2D Adaptive"),
    }
    match finish.params {
        OpParams::Pocket { stock_to_leave, bottom_z, .. } => {
            assert_eq!(stock_to_leave, 0.0, "finishing removes the wall allowance");
            assert_eq!(bottom_z, Height::fixed(6.0), "finishing reaches the model floor");
        }
        _ => panic!("finishing must use 2D Pocket"),
    }
    let part = model::find_by_name(&w, "adaptive-pocket").expect("embedded pocket model");
    assert!(camcraft_app::model_source::of(&w, &part).is_none(), "the model must be portable");

    let rough_header = "(OPERATION: Adaptive rough - ";
    let finish_header = "(OPERATION: Pocket finish - ";
    let rough_start = nc.find(rough_header).expect("Adaptive rough must be posted");
    let finish_start = nc.find(finish_header).expect("Pocket finish must be posted");
    assert!(rough_start < finish_start, "roughing must precede finishing");
    let floor = |blocks: &str| {
        blocks.lines().filter(|line| {
            line.starts_with("G01 ") || line.starts_with("G02 ") || line.starts_with("G03 ")
        }).flat_map(|line| line.split_whitespace())
            .filter_map(|word| word.strip_prefix('Z')?.parse::<f64>().ok())
            .fold(f64::INFINITY, f64::min)
    };
    assert_eq!(floor(&nc[rough_start..finish_start]), 6.5, "roughing leaves 0.5 mm on the floor");
    assert_eq!(floor(&nc[finish_start..]), 6.0, "finishing reaches the model floor");
}
