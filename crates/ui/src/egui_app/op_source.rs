//! The operation page's Source list (#1863): the stock, the user's parts
//! (never a fixture, a work offset or a setup), and - for a kind that
//! cuts a profile (`sketch_model::admits`: 2D milling, plasma, wire EDM) -
//! the DXF profiles placed on the operation's setup. A setup with a stock
//! and a profile and no model so offers the profile as the source; the
//! app's profile law still refuses a wrong pairing by name.
//!
//! Provenance: our own list over this repo's #1862 virtual model
//! reference; no vendor material consulted.

use flecs_ecs::prelude::World;

use camcraft_app::operation::OpParams;
use camcraft_app::{model, sketch, sketch_model};

/// "Stock", then the parts, then the setup's profiles when the kind takes one.
pub(crate) fn source_items(world: &World, setup: Option<&str>, params: &OpParams) -> Vec<String> {
    let mut items = vec!["Stock".to_owned()];
    items.extend(
        model::model_entities(world).into_iter().filter(|(_, e)| !model::is_fixture(world, e)).map(|(n, _)| n),
    );
    if sketch_model::admits(params) {
        for s in sketch::placed_in(world, setup) {
            if !items.contains(&s) {
                items.push(s);
            }
        }
    }
    items
}
