// Provenance: original CamCraft Probe-before-Face catalog contract (#2874).
mod catalog_probing_probe_face {
    use super::*;
    use camcraft_app::{model, operation::OpType, project, setup, world};

    #[test]
    fn probes_before_first_cut() {
        let dir = repo("examples/catalog/probing/probe-face");
        assert!(dir.is_dir(), "catalog case probing/probe-face is missing");
        let w = world::new_world();
        let notes = project::load_noted(&w, &dir.join("probe-face.camproj"))
            .expect("load probe-face project");
        assert!(notes.is_empty(), "unexpected load notes: {notes:?}");
        let kinds: Vec<_> = setup::operation_order(&w)
            .iter()
            .map(|(_, entity)| model::operation_of(&w, entity).expect("operation").params.kind())
            .collect();
        assert_eq!(kinds, [OpType::Probe, OpType::Face]);
        let part = model::find_by_name(&w, "probe-face").expect("embedded block");
        assert!(camcraft_app::model_source::of(&w, &part).is_none());
        assert!(model::facets_of(&w, &part).is_some_and(|facets| !facets.is_empty()));

        let nc = fs::read_to_string(dir.join("probe-face.nc")).expect("read probe-face NC");
        let sections: Vec<_> = nc.split("(OPERATION: ").skip(1).collect();
        assert_eq!(sections.len(), 2, "both operations must reach the program");
        assert!(sections[0].starts_with("Probe1 - PROBE"));
        assert!(sections[1].starts_with("Face1 - FACE"));
        assert!(sections[0].lines().any(|line| line.starts_with("G31 ")),
            "the probe cycle must precede the first cut");
        assert!(!sections[0].lines().any(|line| line.starts_with("G01 ")),
            "no cutting motion before the probe cycle finishes");
        assert!(sections[1].lines().any(|line| line.starts_with("G01 ")),
            "the Face operation must cut");
    }

    #[test]
    fn regenerates() {
        run_case("probing", "probe-face", "regenerates", regenerate);
    }

    #[test]
    fn rebuilds() {
        run_case("probing", "probe-face", "rebuilds", rebuild);
    }
}
