//! #2015: the Stock page states the stock's workpiece material - a
//! Material group under Shape, and a pick is one undoable
//! `SetStockMaterial` through the shell's queue.
//! Provenance: our own page law.

use super::*;

use camcraft_app::stock::StockShape;
use camcraft_app::tools::WorkMaterial;

#[test]
fn the_stock_page_states_the_stock_material_through_one_undo_step() {
    let mut harness = shell_harness();
    {
        let st = &mut harness.state_mut().state;
        if camcraft_app::setup::stock_entity(&st.world, None).is_none() {
            let cmd = camcraft_app::commands::SetStock::new(
                &st.world,
                StockShape::Box { dx: 80.0, dy: 80.0, dz: 40.0 },
                Default::default(),
            )
            .expect("stock");
            st.history.execute(&st.world, cmd);
        }
        st.ui_target = UiTarget::Stock;
    }
    harness.run();
    let top = |h: &Harness<'static, ShellUi>, l: &str| h.get_by_label(l).rect().top();
    let (shape, material, size) = (top(&harness, "Shape"), top(&harness, "Material"), top(&harness, "Size"));
    assert!(shape < material && material < size, "Material sits between Shape and Size: {shape} {material} {size}");
    assert!(harness.query_by_label("Workpiece material").is_some(), "the picker's row");
    let st = &mut harness.state_mut().state;
    let active = camcraft_app::setup::active(&st.world);
    st.pending.push_back(CommandKind::SetStockMaterial(Some(WorkMaterial::Brass)));
    st.drain_actions();
    assert_eq!(camcraft_app::stock_material::of_setup(&st.world, active.as_deref()), Some(WorkMaterial::Brass));
    assert!(st.history.undo(&st.world));
    assert_eq!(camcraft_app::stock_material::of_setup(&st.world, active.as_deref()), None);
}
