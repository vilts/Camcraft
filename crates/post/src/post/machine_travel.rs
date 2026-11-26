//! The linear axes' stated travel (#1882) as the script's
//! `machine.travel`: `{ X: [min, max] | null, Y: ..., Z: ... }` in mm,
//! beside the rotary travel that `machine.rotaries[i].travel` carries.
//!
//! Provenance: our own script contract (`assets/posts/AGENTS.md`). The
//! numbers are the machine file's stated strokes, never a guess: an
//! axis that states none is `null`.

use crate::machine::parse_mch;
use serde_json::{json, Map, Value};

const LETTERS: [&str; 3] = ["X", "Y", "Z"];

fn pair(v: Option<&Value>) -> Value {
    let Some(a) = v.and_then(Value::as_array) else { return Value::Null };
    match (a.first().and_then(Value::as_f64), a.get(1).and_then(Value::as_f64)) {
        (Some(lo), Some(hi)) if lo.is_finite() && hi.is_finite() && lo <= hi => json!([lo, hi]),
        _ => Value::Null,
    }
}

/// The linear travel through either door: the mirror's static axes
/// (`kinematics.static.axes[i].travel`), or a raw vendor tree read by
/// the same parser the app mounts with.
#[must_use]
pub fn linear(m: &Value) -> Value {
    let mut out = Map::new();
    for l in LETTERS {
        out.insert(l.to_owned(), Value::Null);
    }
    if let Some(axes) = m.pointer("/kinematics/static/axes").and_then(Value::as_array) {
        for a in axes {
            if let Some(l) = a.get("letter").and_then(Value::as_str).filter(|l| LETTERS.contains(l)) {
                out.insert(l.to_owned(), pair(a.get("travel")));
            }
        }
    } else if m.pointer("/kinematics/default/parts").is_some()
        && let Ok(parsed) = parse_mch(&m.to_string())
    {
        for a in parsed.axes.iter().filter(|a| !a.rotary && LETTERS.contains(&a.name.as_str())) {
            if let Some((lo, hi)) = a.travel {
                out.insert(a.name.clone(), pair(Some(&json!([lo, hi]))));
            }
        }
    }
    Value::Object(out)
}
