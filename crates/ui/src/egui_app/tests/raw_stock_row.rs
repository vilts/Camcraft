//! #1094: the stock page's "raw stock only" checkbox for a from-model
//! stock - shown with its plain label, a click queues the one undoable
//! command, undo walks it back. Structural (AccessKit labels), no pixels.
//! Provenance: our own page law.

use super::*;
use camcraft_app::commands::{ImportModel, SetStock};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_source::{raw_stock_only_of, RAW_STOCK_ONLY};

#[test]
fn the_stock_page_ticks_raw_stock_only_through_the_command_and_undo() {
    let mut harness = shell_harness();
    {
        let st = &mut harness.state_mut().state;
        let casting = cube(Point3::new(-30.0, -30.0, -10.0), Point3::new(30.0, 30.0, 20.0));
        st.history.execute(&st.world, ImportModel::from_mesh("casting", casting));
        let shape = StockShape::FromModel { source: "casting".into() };
        let cmd = SetStock::new(&st.world, shape, Location::default()).expect("stock");
        st.history.execute(&st.world, cmd);
        st.doc_version += 1;
        st.stock_ui.shape = 2;
        st.stock_ui.source = "casting".into();
        st.ui_target = UiTarget::Stock;
    }
    harness.run();
    assert_eq!(raw_stock_only_of(&harness.state().state.world, None), Some(false), "off by default");
    harness.get_by_label(RAW_STOCK_ONLY).click();
    harness.run();
    assert_eq!(raw_stock_only_of(&harness.state().state.world, None), Some(true), "the tick lands");
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    assert_eq!(raw_stock_only_of(&harness.state().state.world, None), Some(false), "undo walks it back");
}
