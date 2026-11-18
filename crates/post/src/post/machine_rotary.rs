//! Rotary chain, trunnion geometry, and controller RTCP mirroring
//! (#1962) shared by `post::machine::mirror` and `script_props`.
//!
//! Provenance: our own mirror layout and script contract specced from
//! `assets/machines/AGENTS.md` and `assets/posts/AGENTS.md`.

use crate::machine::{can_tilt, parse_mch, Machine, RotarySignConvention};
use serde_json::{json, Map, Value};

fn get<'a>(v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cur = v;
    for k in path {
        cur = cur.get(*k)?;
    }
    Some(cur)
}

/// Mirror the rotary chain in parent-child order (`m.rotary_axes()`).
pub(super) fn mirror_rotaries(m: &Machine) -> Vec<Value> {
    let mut rotaries = Vec::new();
    for a in m.rotary_axes() {
        let mut r = Map::new();
        r.insert("letter".to_owned(), json!(a.name));
        r.insert("type".to_owned(), json!("rotary"));
        if let Some(d) = a.axis {
            r.insert("direction".to_owned(), json!([d[0], d[1], d[2]]));
        }
        if let Some((lo, hi)) = a.travel {
            r.insert("travel".to_owned(), json!([lo, hi]));
        }
        let mut speeds = Map::new();
        if let Some(f) = a.feed {
            speeds.insert("normal".to_owned(), json!(f));
        }
        if let Some(rap) = a.rapid {
            speeds.insert("rapid".to_owned(), json!(rap));
        }
        if !speeds.is_empty() {
            r.insert("speeds".to_owned(), Value::Object(speeds));
        }
        if let Some(p) = &a.parent {
            r.insert("parent".to_owned(), json!(p));
        }
        if let Some(p) = a.pivot {
            r.insert("pivot".to_owned(), json!([p[0], p[1], p[2]]));
        }
        if let Some(o) = a.offset {
            r.insert("offset".to_owned(), json!([o[0], o[1], o[2]]));
        }
        if let Some(z) = a.zero_offset {
            r.insert("zeroOffset".to_owned(), json!(z));
        }
        if let Some(b) = a.rollover {
            r.insert("rollover".to_owned(), json!(b));
        }
        if let Some(sc) = a.sign_convention.or(m.sign_convention) {
            r.insert("signConvention".to_owned(), json!(sc.word()));
        }
        if let Some(tp) = a.turns_part {
            r.insert("turnsPart".to_owned(), json!(tp));
        }
        rotaries.push(Value::Object(r));
    }
    rotaries
}

/// Mirror `controller.rtcp` and machine-level `sign_convention` when stated.
pub(super) fn mirror_controller(m: &Machine, out: &mut Map<String, Value>) {
    if let Some(b) = m.controller_rtcp {
        out.insert("controller".to_owned(), json!({ "rtcp": b }));
        out.insert("rtcp".to_owned(), json!(b && can_tilt(m)));
    }
    if let Some(sc) = m.sign_convention {
        out.insert("signConvention".to_owned(), json!(sc.word()));
    }
}

fn is_rotary_letter(letter: Option<&str>) -> bool {
    matches!(letter, Some("A") | Some("B") | Some("C"))
}

fn sign_word(v: Option<&Value>) -> Option<&'static str> {
    v.and_then(Value::as_str)
        .and_then(RotarySignConvention::from_word)
        .map(RotarySignConvention::word)
}

/// Build the script's `rotaries`, `rtcp`, and `signConvention` properties
/// from either the static mirror or a raw `.mch` tree.
pub(super) fn props_rotaries_and_rtcp(m: &Value) -> (Vec<Value>, bool, &'static str) {
    let mut parsed_raw: Option<Machine> = None;
    let machine_sign = sign_word(m.get("signConvention"))
        .or_else(|| sign_word(get(m, &["kinematics", "default", "sign_convention"])))
        .or_else(|| sign_word(get(m, &["controller", "sign_convention"])))
        .unwrap_or(RotarySignConvention::ToolRh.word());

    let mut rotaries: Vec<Value> = Vec::new();
    let static_rots = get(m, &["kinematics", "static", "rotaries"])
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| {
            if get(m, &["kinematics", "static", "axes"]).is_none()
                && get(m, &["kinematics", "default", "parts"]).is_some()
                && let Ok(parsed) = parse_mch(&m.to_string())
            {
                let rots = mirror_rotaries(&parsed);
                parsed_raw = Some(parsed);
                return Some(rots);
            }
            None
        });

    let machine_sign = parsed_raw
        .as_ref()
        .and_then(|p| p.sign_convention)
        .map_or(machine_sign, RotarySignConvention::word);

    if let Some(rs) = static_rots {
        for r in rs {
            let conv = sign_word(r.get("signConvention").or_else(|| r.get("sign_convention")))
                .unwrap_or(machine_sign);
            rotaries.push(json!({
                "name": get(&r, &["letter"]).and_then(Value::as_str).unwrap_or(""),
                "direction": get(&r, &["direction"]).cloned().unwrap_or(Value::Null),
                "travel": get(&r, &["travel"]).cloned().unwrap_or(Value::Null),
                "speeds": get(&r, &["speeds"]).filter(|s| s.is_object()).cloned().unwrap_or(Value::Null),
                "parent": get(&r, &["parent"]).cloned().unwrap_or(Value::Null),
                "pivot": get(&r, &["pivot"]).cloned().unwrap_or(Value::Null),
                "offset": get(&r, &["offset"]).cloned().unwrap_or(Value::Null),
                "zeroOffset": r.get("zeroOffset").or_else(|| r.get("zero_offset")).cloned().unwrap_or(Value::Null),
                "rollover": get(&r, &["rollover"]).cloned().unwrap_or(Value::Null),
                "signConvention": conv,
                "turnsPart": r.get("turnsPart").or_else(|| r.get("turns_part")).cloned().unwrap_or(Value::Null),
            }));
        }
    }
    if rotaries.is_empty()
        && let Some(axes) = get(m, &["kinematics", "static", "axes"]).and_then(Value::as_array)
    {
        for a in axes {
            let letter = a.get("letter").and_then(Value::as_str).unwrap_or("");
            if is_rotary_letter(Some(letter)) {
                rotaries.push(json!({
                    "name": letter,
                    "direction": a.get("axis").cloned().unwrap_or(Value::Null),
                    "travel": Value::Null,
                    "signConvention": machine_sign,
                }));
            }
        }
    }
    let has_tilt = rotaries.iter().any(|r| {
        r.get("direction")
            .and_then(Value::as_array)
            .filter(|d| d.len() == 3)
            .and_then(|d| {
                let [x, y, z] = [d[0].as_f64()?, d[1].as_f64()?, d[2].as_f64()?];
                Some(z * z < 0.99 * (x * x + y * y + z * z))
            })
            .unwrap_or(false)
    });
    let explicit_rtcp = parsed_raw
        .as_ref()
        .and_then(|p| p.controller_rtcp)
        .or_else(|| get(m, &["controller", "rtcp"]).and_then(Value::as_bool))
        .or_else(|| get(m, &["rtcp"]).and_then(Value::as_bool));
    let rtcp = explicit_rtcp != Some(false) && has_tilt;
    (rotaries, rtcp, machine_sign)
}
