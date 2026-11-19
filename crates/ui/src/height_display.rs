//! Height display coordinates for an operation draft (#2416).
//! Provenance: our rigid-frame display law; no vendor material.

use camcraft_app::cl::TiltedFrame;
use camcraft_app::geometry::{Aabb, Point3};
use camcraft_app::{model, operation};

use crate::state::State;

pub(crate) fn frame(
    state: &State,
) -> Result<Option<TiltedFrame>, camcraft_app::machining2d::GenError> {
    operation::OpFrame::new(
        crate::interact::shown_setup(state),
        state.op_ui.params.tilt(),
    )
    .checked_frame()
}

/// HeightPlane.z retains the generation fold coordinate. Shift once,
/// then map the local rectangle into the setup frame.
pub(crate) fn point(state: &State, x: f64, y: f64, z: f64) -> Point3 {
    match frame(state).ok().flatten().filter(|f| !f.is_upright()) {
        Some(f) => f.to_world(Point3::new(x, y, z - f.origin.z)),
        None => Point3::new(x, y, z),
    }
}

/// Stock reach in the plane XY. Untilting keeps the shipped bounds.
pub(crate) fn bounds(state: &State) -> Option<Aabb> {
    let setup = crate::interact::shown_setup(state);
    crate::height_memo::bounds(&state.world, setup.as_deref(), state.op_ui.params.tilt(), || bounds_fresh(state))
}

fn bounds_fresh(state: &State) -> Option<Aabb> {
    let f = frame(state).ok().flatten();
    if f.is_none_or(|f| f.is_upright()) {
        return model::stock_world_aabb(&state.world)
            .or_else(|| model::solid_world_aabb(&state.world));
    }
    let setup = crate::interact::shown_setup(state);
    let opframe = operation::OpFrame::new(setup.clone(), state.op_ui.params.tilt());
    let stock = camcraft_app::setup::stock_entity_name(&state.world, setup.as_deref());
    model::find_by_name(&state.world, &stock)
        .and_then(|e| opframe.furniture_aabb_local(&state.world, &e))
        .or_else(|| opframe.solid_aabb_local(&state.world))
}

/// The chip states the same local distance as the Heights tab.
pub(crate) fn value(state: &State, z: f64) -> String {
    let (z, suffix) = match frame(state).ok().flatten().filter(|f| !f.is_upright()) {
        Some(f) => (z - f.origin.z, " along tool axis"),
        None => (z, ""),
    };
    format!("{}{suffix}", camcraft_app::units::len(z, state.units(), 2))
}
