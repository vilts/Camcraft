//! The viewport floor (#2172): the height at which the ground grid and the
//! bottom of a mill's work envelope are drawn. It is the lowest point of
//! the bodies that stand in the active setup: the models, the setup's own
//! fixtures and its stock. With no fixture the model and the stock stand
//! on the floor; a fixture under the stock puts the floor at the fixture's
//! base, so the model and the stock stand on the fixture; removing the
//! fixture brings the floor back up to them.
//!
//! The law: display only. The floor moves the reference furniture to the
//! bodies, never the bodies to a reference, so no stored position, WCS,
//! toolpath, collision check, simulation or posted line can change. It
//! reads the world at each frame and so follows undo and redo by
//! construction. A lathe keeps its spindle-axis view (#1993): there the
//! floor stays at world Z=0.
//! Provenance: our own view-state design (issue #2172); no vendor material.

use camcraft_app::geometry::transform_point;
use camcraft_app::model;
use flecs_ecs::prelude::Entity;

use crate::state::State;
use crate::view_frame::ViewFrame;

/// Past this height (mm) the floor is not trusted: far beyond any machine,
/// and the grid would draw nothing useful there (Prime Directive: a
/// corrupt mesh never sends the furniture to infinity).
const FLOOR_MAX_ABS: f64 = 1.0e7;

/// The world Z of the viewport floor: the lowest finite point of the
/// drawn bodies of the active setup, or 0 when there is none (an empty
/// document, a lathe, or only non-finite geometry).
#[must_use]
pub(crate) fn floor_z(state: &State) -> f64 {
    if ViewFrame::of(state) == ViewFrame::Lathe {
        return 0.0;
    }
    let mut low = f64::INFINITY;
    // A body hidden by its browser eye still stands where it stands: the
    // floor does not jump when an eye closes. A fixture of a setup that
    // is not shown is not drawn at all, so it does not count.
    for (name, entity) in model::model_entities(&state.world) {
        if !state.fixture_hidden_by_setup(&name) {
            lower(state, &entity, &mut low);
        }
    }
    let stock = camcraft_app::setup::stock_entity_name(
        &state.world,
        camcraft_app::setup::active(&state.world).as_deref(),
    );
    if let Some(entity) = model::find_by_name(&state.world, &stock)
        && model::is_stock(&state.world, &entity)
    {
        lower(state, &entity, &mut low);
    }
    if low.is_finite() && low.abs() <= FLOOR_MAX_ABS { low } else { 0.0 }
}

/// Lower `low` to the lowest finite Z of `entity` where the viewport draws
/// it (the active setup's pose, #326: the same transform as the draw).
fn lower(state: &State, entity: &Entity, low: &mut f64) {
    let Some(tris) = model::facets_of(&state.world, entity) else {
        return;
    };
    let loc = camcraft_app::setup_align::view_pose(&state.world, entity).unwrap_or_default();
    for t in tris.iter() {
        for p in [t.a, t.b, t.c] {
            let z = transform_point(p, loc.pos, loc.rot).z;
            if z.is_finite() && z < *low {
                *low = z;
            }
        }
    }
}
