//! The MCP raised-rotary tools (#821): read the stack a setup's rotary
//! stands on (its subplates and the plates under them) and the
//! `nose_to_axis` the index-swing check judges with, and state (or clear)
//! the setup's own measured figure - `SetSetupNoseToAxis`, the very
//! undoable command the setup's Machine page queues, one History step.
//!
//! Provenance: our own tool surface over `rotary_stack` and the setup
//! command.

use serde_json::{Map, Value, json};

use super::setup_tools::{cmd_err, obj, setup_or_active};
use super::tools::{Tool, req_str};
use crate::commands::SetSetupNoseToAxis;
use crate::rotary_stack::{self, Rise};
use crate::script::{Env, fnum};

pub(super) const TOOLS: &[Tool] = &[
    Tool { name: "get_rotary_stack", description: "Read how a setup's rotary is raised (#821): the rotary subplates in the setup and every plate under each (top down, with thicknesses), the setup's stated nose_to_axis if any, and the machine's work-turning rotaries' nose_to_axis - the figures the index-swing check judges with (the stated figure wins; else the machine's less the tallest stack). `refused` names why the check cannot judge (an imported plate with no stated thickness under a subplate, a stated figure that is no length).", schema: s_get, run: get_rotary_stack },
    Tool { name: "set_setup_nose_to_axis", description: "State a setup's own spindle-nose-to-rotary-centerline figure, mm, measured on the stack the rotary stands on (a subplate on a fixture plate) - it wins over the machine file's figure less the stack. null clears it. One undoable step; a figure that is not a length over zero (or over 5000 mm) is refused.", schema: s_set, run: set_setup_nose_to_axis },
];

fn rise_view(env: &Env, setup: Option<&str>) -> Value {
    let machine: Vec<Value> = crate::machine::of_setup(&env.world, setup)
        .1
        .map(|m| {
            m.axes
                .iter()
                .filter(|a| a.rotary && a.turns_part == Some(true))
                .map(|a| json!({ "rotary": a.name, "nose_to_axis": a.nose_to_axis.map(fnum) }))
                .collect()
        })
        .unwrap_or_default();
    let mut out = json!({
        "setup": setup,
        "stated_nose_to_axis": rotary_stack::stated(&env.world, setup).map(fnum),
        "machine_rotaries": machine,
        "stack": null, "refused": null,
    });
    match rotary_stack::rise(&env.world, setup) {
        Rise::Table | Rise::Stated(_) => {}
        Rise::Stack(s) => {
            out["stack"] = json!({
                "thickness": fnum(s.thickness),
                "layers": s.layers.iter().map(|(n, t)| json!({ "fixture": n, "thickness": fnum(*t) })).collect::<Vec<_>>(),
            });
        }
        Rise::Refused(why) => out["refused"] = json!(why),
    }
    out
}

fn get_rotary_stack(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let setup = setup_or_active(env, a)?;
    Ok(rise_view(env, setup.as_deref()))
}

fn set_setup_nose_to_axis(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let setup = req_str(a, "setup")?.to_owned();
    let next = match a.get("nose_to_axis") {
        None => return Err("\"nose_to_axis\" is required: a number of mm, or null to clear".into()),
        Some(Value::Null) => None,
        Some(v) => Some(v.as_f64().ok_or("\"nose_to_axis\" must be a number of mm or null")?),
    };
    let cmd = SetSetupNoseToAxis::new(&env.world, &setup, next).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(rise_view(env, Some(&setup)))
}

fn s_get() -> Value {
    obj(json!({ "setup": { "type": "string", "description": "The setup; defaults to the active one." } }), &[])
}

fn s_set() -> Value {
    obj(
        json!({
            "setup": { "type": "string", "description": "The setup's name." },
            "nose_to_axis": { "type": ["number", "null"], "description": "Spindle nose to the rotary's centerline, mm, as measured on this setup's stack; null clears it." }
        }),
        &["setup", "nose_to_axis"],
    )
}
