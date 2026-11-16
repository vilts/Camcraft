//! #864: a model imported coarser than the STEP importer promises says
//! so on its page, with the error it carries; a full-fidelity one does not.
//! Provenance: our own page law; no HSMWorks material.
//!
//! Gate map (M4.4): covers ui::egui_app::pages (model_page),
//! app::model_fidelity.

use super::*;

#[test]
fn a_reduced_fidelity_model_says_so_on_its_page_and_a_normal_one_does_not() {
    let mut h = shell_harness();
    {
        let st = &mut h.state_mut().state;
        let block = model::find_by_name(&st.world, "Block").expect("the seeded block");
        let facets = model::facets_of(&st.world, &block).expect("it has geometry");
        let coarse = camcraft_app::commands::ImportModel::from_mesh("Coarse", camcraft_app::geometry::Mesh::from_shared(facets))
            .with_fidelity(Some(0.012));
        st.history.execute(&st.world, coarse);
        st.ui_target = UiTarget::Model("Coarse".into());
    }
    h.run();
    assert!(
        h.query_by_label("Reduced fidelity: surfaces are approximated to within 0.012 mm").is_some(),
        "the flag and the achieved error are on the model page"
    );
    h.state_mut().state.ui_target = UiTarget::Model("Block".into());
    h.run();
    assert!(h.query_by_label("Model: Block").is_some(), "the block's page is open");
    assert!(
        h.query_by_label_contains("Reduced fidelity").is_none(),
        "a full-fidelity model carries no flag"
    );
    // Undo takes the coarse model - and its flag - away; redo brings both back.
    {
        let st = &mut h.state_mut().state;
        st.history.undo(&st.world);
        assert!(model::find_by_name(&st.world, "Coarse").is_none());
        st.history.redo(&st.world);
        let e = model::find_by_name(&st.world, "Coarse").expect("redone");
        assert_eq!(camcraft_app::model_fidelity::chord_of(&st.world, &e), Some(0.012));
    }
}
