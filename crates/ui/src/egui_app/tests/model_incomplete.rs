//! #1610 review: an incomplete model names its hole and post refusal on
//! its page and the accessible import status, without coarse-model advice.
//! Provenance: our own box fixture with one wall omitted.
//!
//! Since #917 the solidcore reader refuses a STEP shell with a face missing
//! at read ("the outer shell is not closed"), so no STEP text reaches an
//! incomplete model at the pinned solidcore commit. The model lands here
//! through the import's own landing step (`land_import_for_test`) with the
//! hole line that `camcraft_geometry::step` writes for a left-out face; the
//! kernel's partial mesh is pinned in `crates/kernel/tests/machining_mesh_open.rs`.
//!
//! Gate map (#1610): covers ui::egui_app::pages (model_page),
//! ui::state (finish_import), ui::egui_app::panes (status_bar).

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{cube, Mesh, Point3};
use egui_kittest::kittest::NodeT;

use super::*;

/// The hole line of a solid whose sixth face the mesher left out.
const HOLE: &str = "closed shell of solid #16 (6 faces) meshes open: face 6 of 6 got no mesh from the kernel (a test reason)";
const SAID: &str = "Incomplete: closed shell of solid #16 (6 faces) meshes open";

#[test]
fn incomplete_model_page_and_import_status_name_the_hole_and_post_refusal() {
    let _guard = snapshot_guard();
    let name = format!("incomplete-ui-{}", std::process::id());
    // The box with its top wall left out: the two facets at z = 1.
    let walls: Vec<_> = cube(Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 1.0, 1.0))
        .facets
        .iter()
        .copied()
        .filter(|t| !(t.a.z == 1.0 && t.b.z == 1.0 && t.c.z == 1.0))
        .collect();
    assert_eq!(walls.len(), 10, "one wall of two facets is left out");
    let mut cmd = ImportModel::from_mesh(&name, Mesh::from_facets(walls))
        .with_fidelity(Some(camcraft_app::model_fidelity::UNKNOWN_MM));
    cmd.hole = Some(HOLE.to_string());
    let mut h = shell_harness();
    {
        let st = &mut h.state_mut().state;
        let file = std::env::temp_dir().join(format!("{name}.step"));
        st.land_import_for_test(file, cmd);
        st.settle();
        st.ui_target = UiTarget::Model(name.clone());
    }
    h.run_steps(3);
    assert!(
        h.query_by_label(&format!("Model: {name}")).is_some(),
        "the model page is open"
    );
    let summary = h
        .query_all_by_label_contains(SAID)
        .find(|node| {
            node.accesskit_node()
                .value()
                .is_some_and(|label| label.starts_with("Incomplete:"))
        })
        .expect("the model page names the missing wall");
    let label = summary.accesskit_node().value().expect("page summary text");
    assert!(
        label.contains("Its operations are refused at the post"),
        "{label}"
    );
    assert!(
        label.contains("re-export the model, or import an STL"),
        "{label}"
    );
    for advice in [
        "This model is approximated more coarsely",
        "export the file again with a tighter tolerance",
        "Raise the STEP facet budget",
        "Reduced fidelity",
    ] {
        assert!(
            h.query_by_label_contains(advice).is_none(),
            "incomplete model must not show {advice}"
        );
    }
    let imported = format!("imported {name}");
    // #1953: the status strip's node is the one that shows the status
    // line itself (an alert line gets a "! " marker), found by its text,
    // not by where the strip sits.
    let line = h.state().state.status.clone();
    let status_node = h
        .query_all_by_label_contains(&imported)
        .find(|node| {
            node.accesskit_node()
                .value()
                .is_some_and(|v| v.trim_start_matches("! ") == line)
        })
        .expect("the real import status is accessible in the status strip");
    let status = status_node.accesskit_node().value().expect("status text");
    for said in [
        SAID,
        "Its operations are refused at the post",
        "re-export the model, or import an STL",
    ] {
        assert!(
            status.contains(said),
            "import status omits {said:?}: {status}"
        );
    }
    assert!(!status.contains("Reduced fidelity"), "{status}");
}
