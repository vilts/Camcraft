//! `cam.importDxf` and the profile view (#1862): place a DXF drawing on
//! a setup's stock through the undoable `ImportDxf`, and say what was
//! read - units, layers, chains and skipped entities. The MCP tools
//! `import_dxf`, `move_dxf_profile` and `get_job` read the same view.
//!
//! Provenance: our own verb over this repo's `ImportDxf` command.

use super::{arg_str, fnum, only_keys, Env};

use crate::commands::{DxfOptions, ImportDxf, ImportSvg};
use crate::sketch;
use crate::units::Unit;
use camcraft_geometry::dxf;

use flecs_ecs::prelude::World;
use serde_json::{json, Value};
use std::path::PathBuf;

/// The unit word of a DXF unit: `mm`, `in`, else its lower-case name.
fn unit_word(unit: dxf::Unit) -> String {
    match unit {
        dxf::Unit::Millimeters => "mm".into(),
        dxf::Unit::Inches => "in".into(),
        other => format!("{other:?}").to_lowercase(),
    }
}

/// One placed profile as data; `None` when the name wears none.
pub(crate) fn profile_view(world: &World, name: &str) -> Option<Value> {
    let (entity, profile) = sketch::find(world, name)?;
    let data = &profile.data;
    let units = data.units();
    let chains = data.chains();
    let skipped = data.skipped_total();
    let (x, y, angle) = crate::model::transform_of(world, &entity).and_then(sketch::face_placement).unwrap_or_default();
    let pose = sketch::world_pose(world, &entity);
    let stated = match units.stated {
        dxf::Units::Known(u) => json!(unit_word(u)),
        dxf::Units::MissingOrUnitless => Value::Null,
        dxf::Units::Unsupported(code) => json!(format!("$INSUNITS {code}")),
    };
    Some(json!({
        "profile": name,
        "setup": profile.setup,
        "units": {
            "stated": stated,
            "used": unit_word(units.applied),
            "from": if units.overridden() { "caller" } else { "file" },
            "mm_per_unit": fnum(units.applied.millimeters_per_unit()),
        },
        "layers": data.layers(),
        "primitives": data.primitives().len(),
        "join_tol": fnum(data.join_tolerance()),
        "chains": { "closed": chains.closed.len(), "open": chains.open.len(), "rejected": chains.rejected.len() },
        "skipped": data.summary().skipped,
        "skipped_total": skipped,
        "bounds": data.bounds().map(|(lo, hi)| json!({ "min": [fnum(lo.x), fnum(lo.y)], "max": [fnum(hi.x), fnum(hi.y)] })),
        // On the top face of the setup's stock, from the stock origin.
        "placement": { "x": fnum(x), "y": fnum(y), "angle": fnum(angle) },
        // Where that is in the setup's frame; null while the setup has no stock.
        "origin": pose.map(|p| vec![fnum(p.pos.x), fnum(p.pos.y), fnum(p.pos.z)]),
        "rotation": pose.map(|p| p.rot.iter().map(|v| fnum(*v)).collect::<Vec<_>>()),
    }))
}

/// `cam.importDxf(path, options?)` - place a DXF drawing as a profile on
/// the stock of a setup (`setup`; the active one by default). `units`
/// is `"mm"` or `"in"`: it wins over the file's `$INSUNITS`, and a file
/// that states no unit refuses by name without it. `join_tol` is the
/// chaining tolerance in mm; `name` names the profile (the file's stem by
/// default). One undo step. The reply states the units used and the
/// skipped entities.
pub(crate) fn v_import_dxf(env: &mut Env, args: Vec<Value>) -> Result<Value, String> {
    let (path, setup, options) = import_args(&args, "importDxf", "a DXF path")?;
    let cmd = ImportDxf::new(&env.world, setup.as_deref(), &path, &options).map_err(|e| e.to_string())?;
    let name = cmd.name().to_owned();
    env.history.execute(&env.world, cmd);
    placed(env, &name, "DXF")
}

/// `cam.importSvg(path, options?)` (#1880) - place an SVG drawing as a
/// profile, with `importDxf`'s options. `units` is the unit of one drawing
/// unit of a file that states no physical size; such a file refuses by name
/// without it, and a file that states its size refuses it. One undo step.
pub(crate) fn v_import_svg(env: &mut Env, args: Vec<Value>) -> Result<Value, String> {
    let (path, setup, options) = import_args(&args, "importSvg", "an SVG path")?;
    let cmd = ImportSvg::new(&env.world, setup.as_deref(), &path, &options).map_err(|e| e.to_string())?;
    let name = cmd.name().to_owned();
    env.history.execute(&env.world, cmd);
    placed(env, &name, "SVG")
}

/// The path, the setup and the options of an import verb.
fn import_args(args: &[Value], verb: &str, what: &str) -> Result<(PathBuf, Option<String>, DxfOptions), String> {
    let path = PathBuf::from(arg_str(args, 0, what)?);
    if args.len() > 2 {
        return Err(format!("{verb} takes a path and one options object, got {} arguments", args.len()));
    }
    let given = match args.get(1) {
        None | Some(Value::Null) => Default::default(),
        Some(v) => only_keys(v, &["setup", "units", "join_tol", "name"])?,
    };
    let text = |key: &str| match given.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(other) => Err(format!("\"{key}\" must be a string, got {other}")),
    };
    let units = match text("units")?.as_deref() {
        None => None,
        Some("mm") => Some(Unit::Millimeters),
        Some("in") => Some(Unit::Inches),
        Some(other) => return Err(format!("\"units\" must be \"mm\" or \"in\", not \"{other}\"")),
    };
    let join_tolerance = match given.get("join_tol") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_f64().ok_or_else(|| format!("\"join_tol\" must be a number in mm, got {v}"))?),
    };
    Ok((path, text("setup")?, DxfOptions { units, join_tolerance, name: text("name")? }))
}

/// The view of a profile that just landed, with its one-line summary.
fn placed(env: &Env, name: &str, kind: &str) -> Result<Value, String> {
    let mut out = profile_view(&env.world, name).ok_or_else(|| format!("the {kind} profile '{name}' did not land"))?;
    let used = out["units"]["used"].as_str().unwrap_or_default().to_owned();
    out["summary"] = json!(format!(
        "'{name}' placed on the stock top: {} primitives read in {used}, {} closed and {} open chains, {} rejected, {} entities skipped",
        out["primitives"], out["chains"]["closed"], out["chains"]["open"], out["chains"]["rejected"], out["skipped_total"],
    ));
    Ok(out)
}
