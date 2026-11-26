//! #2976 at the app: a STEP model whose edges lie off their surfaces by
//! more than the chord, in a file that states a tolerance, lands flagged
//! reduced fidelity at twice that tolerance (never over 0.02 mm), and the
//! note in plain words rides the `ImportModel` command. A tracked fixture
//! with tight edges lands at full fidelity.
//! Fixtures (our own, testdata/models/edge_gap/*.step, generated): a box
//! whose top is a B-spline tilted by the gap while its top edges stay flat.
//!
//! Gate map (#2976): covers app::commands (import_model: notes and
//! coarse_mm), app::model_fidelity (chord_of).
//!
//! Provenance: our own pins over our own fixtures; no vendor material.

use camcraft_app::commands::{History, ImportModel};
use camcraft_app::geometry::import::step;
use camcraft_app::world::new_world;
use camcraft_app::{model, model_fidelity};

fn path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name)
}

#[test]
fn a_model_with_edges_off_its_surfaces_lands_flagged_with_its_note() {
    let cmd = ImportModel::new(&path("edge_gap/edge_gap_018_stated_010.step")).expect("imports at reduced fidelity");
    let chord = cmd.coarse_mm.expect("the looser chord is the reduced-fidelity flag");
    assert!((chord - 0.02).abs() <= 1e-12, "{chord}");
    assert!(
        cmd.notes.iter().any(|n| n
            == "This file's edges sit up to 0.018 mm off their surfaces. The file states a tolerance of 0.010 mm. \
                The model is used at reduced accuracy (0.020 mm). \
                Gouge checks against this model use a wider true-surface bound than the normal 0.010 mm. \
                Each posted program states this bound and names this model in its header. \
                An operation that they cannot check is refused at the post."),
        "{:?}",
        cmd.notes
    );
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, cmd.renamed("gap"));
    let e = model::find_by_name(&world, "gap").expect("landed");
    assert_eq!(model_fidelity::chord_of(&world, &e), Some(chord));
}

#[test]
fn a_gap_over_the_cap_is_refused_and_tight_fixtures_stay_full_fidelity() {
    let e = ImportModel::new(&path("edge_gap/edge_gap_021_stated_010.step")).map(|c| c.coarse_mm).expect_err("refused");
    assert!(e.to_string().contains("over the cap of 0.02 mm; the file states a tolerance of 0.01 mm"), "{e}");
    for file in ["fig8.step", "fillet_corner.step", "bar_cross_holes.step", "twisted_flank.step"] {
        let cmd = ImportModel::new(&path(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
        assert_eq!(cmd.coarse_mm, None, "{file} keeps full fidelity");
        assert!(!cmd.notes.iter().any(|n| n.contains("reduced accuracy")), "{file}: {:?}", cmd.notes);
    }
}

#[test]
fn the_note_says_what_the_fidelity_law_does_at_the_chord() {
    // #2996: file-tolerance widening states the true-surface bound
    // in the program header; past the limit, nothing is left.
    assert!(step::loosened_consequence(0.02).starts_with("Gouge checks against this model use a wider true-surface bound"));
    assert!(step::loosened_consequence(0.01524).starts_with("Gouge checks against this model use a wider true-surface bound"));
    assert!(step::loosened_consequence(0.021).starts_with("A program cannot be posted while this model is in the document."));
    assert!(step::loosened_consequence(0.012).starts_with("Gouge checks against this model use a wider true-surface bound"));
    // Up to DEV_TOL (excess 0.004 mm): the checks are tightened, not refused.
    assert_eq!(
        step::loosened_consequence(0.009),
        "Gouge checks against this model are stricter, and an operation that they cannot check is refused at the post."
    );
}
