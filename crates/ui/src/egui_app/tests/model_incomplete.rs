//! #1610 review: an incomplete model names its hole and post refusal on
//! its page and the accessible import status, without coarse-model advice.
//! Provenance: our own box fixture with one wall omitted.
//!
//! Gate map (#1610): covers ui::egui_app::pages (model_page),
//! ui::state (finish_import), ui::egui_app::panes (status_bar).

use egui_kittest::kittest::NodeT;

use super::*;

#[test]
fn incomplete_model_page_and_import_status_name_the_hole_and_post_refusal() {
    let _guard = snapshot_guard();
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/box.step");
    let text = std::fs::read_to_string(fixture).expect("box fixture");
    let shell = "#17 = CLOSED_SHELL('', (#18, #25, #32, #39, #46, #53));";
    assert_eq!(text.matches(shell).count(), 1, "fixture layout changed");
    let open = text.replace(shell, "#17 = CLOSED_SHELL('', (#18, #25, #32, #39, #46));");
    let path = std::env::temp_dir().join(format!("incomplete-ui-{}.step", std::process::id()));
    std::fs::write(&path, open).expect("write incomplete shell");
    let name = format!("incomplete-ui-{}", std::process::id());
    let mut h = shell_harness();
    {
        let st = &mut h.state_mut().state;
        st.pending.push_back(CommandKind::ImportFile(path.clone()));
        st.settle();
        st.ui_target = UiTarget::Model(name.clone());
    }
    let _ = std::fs::remove_file(path);
    h.run_steps(3);
    assert!(
        h.query_by_label(&format!("Model: {name}")).is_some(),
        "the model page is open"
    );
    let summary = h
        .query_all_by_label_contains("Incomplete: closed shell #17 (5 faces) meshes open")
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
        "This model was too large",
        "Raise the STEP facet budget",
        "Reduced fidelity",
    ] {
        assert!(
            h.query_by_label_contains(advice).is_none(),
            "incomplete model must not show {advice}"
        );
    }
    let imported = format!("imported {name}");
    let status_node = h
        .query_all_by_label_contains(&imported)
        .find(|node| node.rect().min.y > 600.0)
        .expect("the real import status is accessible in the bottom strip");
    let status = status_node.accesskit_node().value().expect("status text");
    for said in [
        "Incomplete: closed shell #17 (5 faces) meshes open",
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
