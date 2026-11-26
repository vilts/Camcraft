//! `cam.insertText` (#2679, #2891): place a typed string as a profile of
//! single-line engraving strokes on the stock of a setup through the undoable
//! `InsertText`. The MCP tool `insert_text` calls the same verb.
//!
//! Provenance: our own verb over this repo's `InsertText` command.

use super::{arg_str, only_keys, Env};
use super::dxf::profile_view;

use crate::commands::{InsertText, TextOptions};

use std::collections::BTreeMap;

use serde_json::{json, Value};

/// `n` finite numbers under `key`, or `None` when absent.
fn opt_nums<const N: usize>(a: &BTreeMap<String, Value>, key: &str) -> Result<Option<[f64; N]>, String> {
    let v = match a.get(key) {
        None | Some(Value::Null) => return Ok(None),
        Some(v) => v,
    };
    let bad = || format!("\"{key}\" must be an array of {N} finite numbers, got {v}");
    let arr = v.as_array().filter(|x| x.len() == N).ok_or_else(bad)?;
    let mut out = [0.0; N];
    for (o, x) in out.iter_mut().zip(arr) {
        *o = x.as_f64().filter(|f| f.is_finite()).ok_or_else(bad)?;
    }
    Ok(Some(out))
}

/// `cam.insertText(text, options)` - place a typed string as a profile of
/// single-line engraving strokes on the stock of a setup (`setup`; the active
/// one by default). `height` is the cap height in mm (0.5 to 1000). `position`
/// is `[x, y]` in mm from the stock origin (default `[0, 0]`); `angle` is the
/// turn in degrees about the face normal (default 0). `letter_spacing` is the
/// extra gap between characters in mm (default 0). `name` names the profile
/// (`"Text <string>"` by default). One undo step.
pub(crate) fn v_insert_text(env: &mut Env, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("insertText requires a text string and options".to_owned());
    }
    let text = arg_str(&args, 0, "a text string")?.to_owned();
    if args.len() > 3 {
        return Err(format!("insertText takes a text string and options, got {} arguments", args.len()));
    }

    let (height_from_arg, given) = match args.get(1) {
        None | Some(Value::Null) => (None, Default::default()),
        Some(Value::Number(n)) => {
            let h = n.as_f64().ok_or_else(|| format!("\"height\" must be a number, got {n}"))?;
            let opts = match args.get(2) {
                None | Some(Value::Null) => Default::default(),
                Some(v) => only_keys(v, &["height", "position", "angle", "letter_spacing", "setup", "name", "text"])?,
            };
            (Some(h), opts)
        }
        Some(Value::Object(_)) => {
            if args.len() > 2 {
                return Err(format!("insertText takes a text string and one options object, got {} arguments", args.len()));
            }
            let opts = only_keys(&args[1], &["height", "position", "angle", "letter_spacing", "setup", "name", "text"])?;
            (None, opts)
        }
        Some(other) => {
            return Err(format!("the second argument must be an options object or a height number, got {other}"));
        }
    };

    let num = |key: &str| match given.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_f64().map(Some).ok_or_else(|| format!("\"{key}\" must be a number, got {v}")),
    };
    let text_opt = |key: &str| match given.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(other) => Err(format!("\"{key}\" must be a string, got {other}")),
    };

    let height = match height_from_arg {
        Some(h) => h,
        None => num("height")?.ok_or("\"height\" is required")?,
    };
    let [x, y] = opt_nums::<2>(&given, "position")?.unwrap_or_default();
    let angle_deg = num("angle")?.unwrap_or(0.0);
    let letter_spacing = num("letter_spacing")?.unwrap_or(0.0);
    let name = text_opt("name")?;
    let setup = text_opt("setup")?;

    let options = TextOptions {
        text,
        height,
        letter_spacing,
        x,
        y,
        angle_deg,
        name,
    };

    let cmd = InsertText::new(&env.world, setup.as_deref(), &options).map_err(|e| e.to_string())?;
    let name = cmd.name().to_owned();
    env.history.execute(&env.world, cmd);

    let mut out = profile_view(&env.world, &name).ok_or_else(|| format!("the text profile '{name}' did not land"))?;
    out["text"] = json!(options.text);
    out["height"] = json!(options.height);
    out["summary"] = json!(format!(
        "'{name}' placed on the stock top: \"{}\" at a cap height of {} mm, {} closed and {} open chains",
        options.text, options.height, out["chains"]["closed"], out["chains"]["open"],
    ));
    Ok(out)
}
