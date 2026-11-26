//! Provenance: our MCP surface over the existing thread infeed command and project DTO.

use serde_json::{Map, Value, json};

use super::edit::{find_op, merge};
use super::setup_tools::obj;
use super::tools::{Tool, req_str};
use crate::commands::EditOperationParams;
use crate::model;
use crate::operation::OpParams;
use crate::project::ThreadInfeedDto;
use crate::script::Env;
use crate::turning::ThreadInfeed;

pub(super) const TOOLS: &[Tool] = &[
    Tool {
        name: "get_thread_infeed",
        description: "Read a Threading operation's infeed plan, including default values, spring passes and repeat_each_depth.",
        schema: s_get,
        run: get,
    },
    Tool {
        name: "set_thread_infeed",
        description: "Patch a Threading operation's infeed plan through EditOperationParams. Unspecified fields keep their values. One undo step. Refuses unknown fields and invalid plans by name.",
        schema: s_set,
        run: set,
    },
];

fn thread_params(env: &Env, args: &Map<String, Value>) -> Result<(String, OpParams), String> {
    let key = find_op(env, req_str(args, "name")?)?;
    let entity = model::find_by_name(&env.world, &key)
        .ok_or_else(|| format!("no operation named '{key}'"))?;
    let operation = model::operation_of(&env.world, &entity)
        .ok_or_else(|| format!("'{key}' is not an operation"))?;
    if !matches!(operation.params, OpParams::Threading { .. }) {
        return Err(format!("'{key}' is not a Threading operation"));
    }
    Ok((key, operation.params))
}

// Project serialization omits default flags. An MCP read must also show
// those fields so callers can read and restore the complete default plan.
fn plan_value(plan: ThreadInfeed) -> Value {
    let dto = ThreadInfeedDto::from(plan);
    json!({
        "method": dto.method, "flank_angle": dto.flank_angle,
        "schedule": dto.schedule, "min_depth": dto.min_depth,
        "finish_allowance": dto.finish_allowance, "spring_passes": dto.spring_passes,
        "repeat_each_depth": dto.repeat_each_depth,
        "dips": { "interval": dto.dips.interval, "height": dto.dips.height, "length": dto.dips.length },
    })
}

fn get(env: &mut Env, args: &Map<String, Value>) -> Result<Value, String> {
    let (key, params) = thread_params(env, args)?;
    let OpParams::Threading { infeed_plan, .. } = params else {
        return Err(format!("'{key}' is not a Threading operation"));
    };
    Ok(json!({ "name": key, "infeed_plan": plan_value(infeed_plan) }))
}

fn check_fields(
    patch: &Map<String, Value>,
    fields: &Map<String, Value>,
    prefix: &str,
) -> Result<(), String> {
    for (field, value) in patch {
        let path = format!("{prefix}.{field}");
        let current = fields
            .get(field)
            .ok_or_else(|| format!("unknown field {path}"))?;
        if let (Some(patch), Some(fields)) = (value.as_object(), current.as_object()) {
            check_fields(patch, fields, &path)?;
        }
    }
    Ok(())
}

fn set(env: &mut Env, args: &Map<String, Value>) -> Result<Value, String> {
    let (key, mut params) = thread_params(env, args)?;
    let patch = args
        .get("infeed_plan")
        .and_then(Value::as_object)
        .ok_or("infeed_plan must be an object")?;
    let OpParams::Threading { infeed_plan, .. } = &mut params else {
        return Err(format!("'{key}' is not a Threading operation"));
    };
    let mut next = plan_value(*infeed_plan);
    let fields = next.as_object().ok_or("infeed_plan could not be read")?;
    check_fields(patch, fields, "infeed_plan")?;
    for (field, value) in patch {
        let target = next
            .get_mut(field)
            .ok_or_else(|| format!("unknown field infeed_plan.{field}"))?;
        merge(target, value);
        // Decode each field separately so malformed values name their field.
        let dto: ThreadInfeedDto = serde_json::from_value(next.clone())
            .map_err(|error| format!("bad infeed_plan.{field}: {error}"))?;
        *infeed_plan = dto.into();
    }
    let result = plan_value(*infeed_plan);
    let changed_fields = patch
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    let command = EditOperationParams::new(&env.world, &key, params)
        .map_err(|error| format!("bad infeed_plan ({changed_fields}): {error}"))?;
    env.history.execute(&env.world, command);
    Ok(json!({ "name": key, "infeed_plan": result }))
}

fn s_get() -> Value {
    obj(
        json!({ "name": { "type": "string", "description": "Threading operation key or display label." } }),
        &["name"],
    )
}

fn s_set() -> Value {
    obj(
        json!({
            "name": { "type": "string", "description": "Threading operation key or display label." },
            "infeed_plan": {
                "type": "object", "additionalProperties": false,
                "properties": {
                    "method": { "type": "string", "enum": ["radial", "flank", "alternating"] },
                    "flank_angle": { "type": "number", "description": "Flank angle in degrees." },
                    "schedule": { "type": "string", "enum": ["constant_depth", "constant_area"] },
                    "min_depth": { "type": "number", "description": "Minimum radial depth in mm." },
                    "finish_allowance": { "type": "number", "description": "Radial finishing allowance in mm." },
                    "spring_passes": { "type": "integer", "minimum": 0 },
                    "repeat_each_depth": { "type": "boolean" },
                    "dips": { "type": "object", "additionalProperties": false, "properties": {
                        "interval": { "type": "number" }, "height": { "type": "number" }, "length": { "type": "number" }
                    } }
                }
            }
        }),
        &["name", "infeed_plan"],
    )
}
