//! Provenance: clean-room contract for the original edge-work catalog fixture.
//! Gate map: covers app::project, app::operation, app::posting, app::script.

use camcraft_app::{model, operation::OpType, project, setup, world};
use std::{fs, path::Path};

#[test]
fn edge_work_catalog_posts_all_four_operation_kinds() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/catalog/2d/edge-work");
    assert!(dir.is_dir(), "catalog case 2d/edge-work is missing");
    let w = world::new_world();
    let notes = project::load_noted(&w, &dir.join("edge-work.camproj")).expect("load edge-work");
    assert!(notes.is_empty(), "unexpected load notes: {notes:?}");
    let operations = setup::operation_order(&w);
    let kinds: Vec<_> = operations
        .iter()
        .map(|(_, entity)| {
            model::operation_of(&w, entity)
                .expect("catalog operation")
                .params
                .kind()
        })
        .collect();
    assert_eq!(
        kinds,
        [
            OpType::Chamfer2D,
            OpType::Deburr,
            OpType::Engrave,
            OpType::Trace
        ]
    );
    let plate = model::find_by_name(&w, "edge-work").expect("embedded plate");
    assert!(camcraft_app::model_source::of(&w, &plate).is_none());
    assert!(model::facets_of(&w, &plate).is_some_and(|facets| !facets.is_empty()));
    let program = fs::read_to_string(dir.join("edge-work.nc")).expect("read edge-work NC");
    let sections: Vec<_> = program.split("(OPERATION: ").skip(1).collect();
    assert_eq!(
        sections.len(),
        4,
        "each operation must reach the posted program"
    );
    for (section, name) in sections
        .iter()
        .zip(["Chamfer", "Deburr", "Engrave", "Trace"])
    {
        assert!(section.starts_with(name), "missing {name} section");
        assert!(
            section
                .lines()
                .any(|line| line.starts_with("G01 ") && line.contains('X')),
            "{name} must contain cutting motion"
        );
    }
}
