//! Provenance: clean-room catalog contract over CamCraft's project API and posted lathe output (#2864).
//! Gate map: covers app::project, app::world, app::model, app::model_source.

use std::fs;
use std::path::Path;

use camcraft_app::{model, project, world};

fn word(block: &str, address: char) -> Option<f64> {
    block.split_whitespace().find_map(|w| w.strip_prefix(address))?.parse().ok()
}

fn section<'a>(program: &'a str, name: &str) -> Vec<&'a str> {
    let header = format!("(OPERATION: {name} - ");
    program.lines().skip_while(|line| !line.starts_with(&header)).skip(1)
        .take_while(|line| !line.starts_with("(OPERATION: ")).collect()
}

/// The catalog case shows radial groove cuts followed by release at the part floor.
#[test]
fn groove_part_off_case_posts_rough_finish_and_axis_release() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/catalog/turning/groove-part-off");
    let nc = fs::read_to_string(dir.join("groove-part-off.nc"))
        .expect("catalog case turning/groove-part-off must contain its posted program");
    let kinds: Vec<&str> = nc.lines().filter(|line| line.starts_with("(OPERATION: "))
        .filter_map(|line| line.split(" - ").nth(1)).collect();
    assert_eq!(kinds, ["GROOVE ROUGH", "GROOVE FINISH", "PARTING"]);
    assert!(nc.contains("G18"), "the lathe program selects the ZX plane");
    assert!(!nc.lines().any(|line| word(line, 'Y').is_some()), "the program has no Y motion");

    for (name, diameter) in [("Groove rough", 22.5), ("Groove finish", 22.0)] {
        let blocks = section(&nc, name);
        let cuts: Vec<(f64, f64)> = blocks.iter().filter(|line| line.starts_with("G01 "))
            .filter_map(|line| Some((word(line, 'X')?, word(line, 'Z')?))).collect();
        let minimum = cuts.iter().map(|&(x, _)| x).fold(f64::INFINITY, f64::min);
        assert!((minimum - diameter).abs() < 0.001,
            "{name} must reach diameter {diameter}: {cuts:?}");
        assert!(cuts.iter().any(|&(x, z)| x < 30.0 && (35.0..=41.0).contains(&z)),
            "{name} must cut inside the groove window: {cuts:?}");
        assert!(cuts.iter().filter(|&&(x, _)| x < 30.0)
            .all(|&(_, z)| (35.0..=41.0).contains(&z)),
            "{name} must keep its cutting plunges in the groove: {cuts:?}");
    }
    let parting = section(&nc, "Part off");
    assert!(parting.iter().any(|line| line.starts_with("G01 ")
        && word(line, 'X') == Some(0.0) && word(line, 'Z') == Some(12.0)),
        "the orientation 2 blade reaches the axis at low edge Z12, its kerf Z12..15 below the part (#3452): {parting:?}");
    assert!(parting.iter().filter(|line| line.starts_with("G01 "))
        .filter_map(|line| word(line, 'Z')).all(|z| z + 3.0 <= 15.0 + 1e-6),
        "no part-off feed puts the 3 mm blade above the part's end face Z15: {parting:?}");

    let w = world::new_world();
    let notes = project::load_noted(&w, &dir.join("groove-part-off.camproj"))
        .expect("the example project must open outside the case directory");
    assert!(notes.is_empty(), "the project must open without repair: {notes:?}");
    let entity = model::find_by_name(&w, "groove-part-off").expect("the project keeps the part model");
    assert!(camcraft_app::model_source::of(&w, &entity).is_none(), "the part mesh must be embedded");
    assert!(model::facets_of(&w, &entity).is_some_and(|facets| !facets.is_empty()));
}
