//! Provenance: shared corner-relief fixtures over the app command and project APIs.

use camcraft_app::commands::{AddTool, History, ImportModel, SetStock};
use camcraft_app::geometry::Mesh;
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::operation::{Height, HeightReference, OpParams};
use camcraft_app::project;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;
use serde_json::Value;

pub const TOOL: &str = "EM-0600-4FL";

pub fn setup_world(mesh: Option<Mesh>, dx: f64, dy: f64, dz: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    if let Some(m) = mesh {
        h.execute(&world, ImportModel::from_mesh("part", m));
    }
    h.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx, dy, dz }, Default::default()).expect("stock"),
    );
    let t = catalog()
        .into_iter()
        .find(|(n, _)| *n == TOOL)
        .map(|(_, t)| t)
        .expect("EM-0600-4FL in catalog");
    h.execute(&world, AddTool::new(&world, TOOL, t).expect("tool"));
    (world, h)
}

pub fn patch_pocket(base: &OpParams, patch: Value) -> Result<OpParams, String> {
    let Value::Object(mut tagged) = project::params_to_json(base) else {
        return Err("expected params object".into());
    };
    let Some((key, Value::Object(fields))) = tagged.iter_mut().next().map(|(k, v)| (k.clone(), v)) else {
        return Err("expected tagged variant".into());
    };
    let Value::Object(patch_obj) = patch else {
        return Err("patch must be object".into());
    };
    for (k, v) in patch_obj {
        fields.insert(k, v);
    }
    let next = project::params_from_json(&Value::Object(tagged)).map_err(|e| format!("params_from_json: {e}"))?;
    let back = project::params_to_json(&next);
    let key_fields = back.get(&key).and_then(Value::as_object);
    if !key_fields.is_some_and(|m| m.contains_key("corner_relief")) {
        return Err(format!("params round-trip dropped corner_relief on {key}: {back}"));
    }
    Ok(next)
}

pub fn base_pocket(strategy: PocketStrategy, top: Height, bottom: Height, stepdown: f64) -> OpParams {
    let mut p = camcraft_app::operation::seed_params(camcraft_app::operation::OpType::Pocket.ordinal());
    if let OpParams::Pocket {
        stepover,
        stock_to_leave,
        top_z,
        bottom_z,
        stepdown: sd,
        strategy: st,
        clearance_z,
        entry,
        ..
    } = &mut p
    {
        *stepover = 3.0;
        *stock_to_leave = 0.0;
        *top_z = top;
        *bottom_z = bottom;
        *sd = stepdown;
        *st = strategy;
        *clearance_z = Height::of(HeightReference::TopOfStock, 5.0);
        *entry = camcraft_app::machining3d::Entry3D::Plunge;
    }
    p
}
