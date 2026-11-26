//! The machine definition - our `.mch` role.
//!
//! Provenance: a JSON file carrying capability flags and a kinematics
//! tree (head → axes → table), a layout specced from Autodesk's
//! published post-processor documentation (behavior only, per the
//! Clean-Room Protocol). The grammar and every key name are ours.

use super::machine_rotary::is_rotary_letter;
use serde_json::{json, Value};

/// Parse the file text (syntax only; absent fields default at read
/// time, so a minimal file is legal).
pub fn from_json(text: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str(text)
}

/// The static-grammar mirror of a checker-parsed machine (M3.3 A).
/// [`crate::machine::Machine`] is what a vendor `.mch` parses to -
/// limits the checker judges; the script's `machine` global and the
/// sim's traverse timer walk `kinematics.static` instead. This says
/// the same numbers in that grammar, stating only what the file
/// stated: an unstated limit is absent, never a zero that lies (the
/// posts' `> 0` guards keep silence faithful). Coolants arrived with
/// M3.3 stage C: a machine that names its options says so under
/// `head.toolStation.coolants`, and one that names nothing mirrors
/// nothing - the post holds its coolant word silent, exactly as it
/// did when the bridge carried none. Stage D brought the rotaries
/// across - as geometry, kept out of `axes` where the traverse and
/// the rate ceilings live: their direction and degree travel join
/// the script under `kinematics.static.rotaries`. Stage G added a
/// `speeds` object to those entries - deg/min under the rotary's own
/// roof, where the sim's timeline reads the pace an angle travels
/// at - and the unit law stands exactly as stage C set it: a rotary
/// rate crosses into no mm/min field, no ceiling sum, no script
/// property. Issue #33 let the post's rotary inventory carry those
/// rates too, under each rotary's own `speeds` (deg/min, named as
/// such): a C-axis post caps its composite feed by them, and still
/// never adds them to an mm/min number.
#[must_use]
pub fn mirror(m: &crate::machine::Machine) -> Value {
    let mut axes: Vec<Value> = Vec::new();
    let rotaries = super::machine_rotary::mirror_rotaries(m);
    for a in &m.axes {
        if a.rotary {
            continue;
        }
        let direction = match a.name.as_str() {
            "X" => [1.0, 0.0, 0.0],
            "Y" => [0.0, 1.0, 0.0],
            "Z" => [0.0, 0.0, 1.0],
            _ => continue,
        };
        let mut speeds = serde_json::Map::new();
        if let Some(r) = a.rapid {
            speeds.insert("rapid".to_owned(), json!(r));
        }
        if let Some(f) = a.feed {
            speeds.insert("normal".to_owned(), json!(f));
        }
        let mut axis = json!({ "letter": a.name, "direction": direction, "speeds": Value::Object(speeds) });
        if let (Some((lo, hi)), Some(o)) = (a.travel, axis.as_object_mut()) {
            o.insert("travel".to_owned(), json!([lo, hi])); // #1882
        }
        axes.push(axis);
    }
    // Rates the script reads as ceilings: the fastest stated axis -
    // and stated axes are LINEAR only. A rotary's deg/min is not a
    // number any prismatic ceiling may absorb (stage C's law, kept).
    let rapid = m
        .axes
        .iter()
        .filter(|a| !a.rotary)
        .filter_map(|a| a.rapid)
        .fold(0.0_f64, f64::max);
    let feed = m
        .axes
        .iter()
        .filter(|a| !a.rotary)
        .filter_map(|a| a.feed)
        .fold(0.0_f64, f64::max);
    let mut head = serde_json::Map::new();
    if let Some(s) = m.spindle_max {
        head.insert("spindle".to_owned(), json!({"max": s}));
    }
    if let Some(s) = m.spindle_min {
        head.entry("spindle").or_insert_with(|| json!({}))["min"] = json!(s);
    }
    if !m.spindle_ranges.is_empty() {
        // #170: the hand-changed speed ranges, [[low, high], ...] rpm.
        let rs: Vec<Value> = m.spindle_ranges.iter().map(|(lo, hi)| json!([lo, hi])).collect();
        head.insert("spindleRanges".to_owned(), Value::Array(rs));
    }
    if let Some(b) = m.rigid_tapping {
        head.insert("rigidTapping".to_owned(), json!(b));
    }
    if let Some(n) = m.spindles {
        // The handoff inventory (M5.3 C): how many heads stated a
        // spindle. Absent when the file stated none - never a 1
        // that guesses for a machine that said nothing.
        head.insert("spindleCount".to_owned(), json!(n));
    }
    if !m.coolants.is_empty() {
        // Only what the file stated: a named-without-code entry rides
        // as a bare name, and script_props reads it as codeless - the
        // post keeps silent about a coolant it cannot address.
        let cs: Vec<Value> = m
            .coolants
            .iter()
            .map(|c| {
                if c.m_code.is_empty() {
                    json!({ "name": c.name })
                } else {
                    json!({ "name": c.name, "mCode": c.m_code })
                }
            })
            .collect();
        head.insert("toolStation".to_owned(), json!({ "coolants": cs }));
    }
    if let Some(lt) = &m.live_tool {
        // Issue #33: the builder's C-axis / live-spindle words.
        head.insert(
            "liveTool".to_owned(),
            json!({
                "cAxisOn": lt.c_axis_on,
                "cAxisOff": lt.c_axis_off,
                "spindleOn": lt.spindle_on,
                "spindleOff": lt.spindle_off,
            }),
        );
    }
    let mut out = json!({
        "name": m.model,
        "kinematics": { "static": { "axes": axes, "rotaries": rotaries, "head": Value::Object(head) } },
        "rapidRate": rapid,
        "feedRateMax": feed,
        // #131: the unit the machine is programmed in (numbers stay mm).
        "units": m.units.as_deref().unwrap_or("mm"),
    });
    // #374: the inverted linear axes, which the engine mirrors the stream
    // in (`post::invert`). Absent for a standard machine, so every
    // standard mirror is byte-identical to the one before #374.
    // #495: the safe Z retract, machine coordinates (mm), only when the
    // file states it - an unstated machine's mirror is unchanged.
    if let Some(z) = m.safe_retract_z
        && let Some(o) = out.as_object_mut()
    {
        o.insert("safeRetractZ".to_owned(), json!(z));
    }
    // #671: a declared plasma table says so, for the plasma post's
    // machine check; every other mirror is unchanged.
    if crate::machine::kind(Some(m)) == crate::machine::Kind::Plasma
        && let Some(o) = out.as_object_mut()
    {
        o.insert("kind".to_owned(), json!("plasma"));
    }
    // #1882: a vendor-tree waterjet keeps its kind through the CLI mirror.
    if crate::machine::kind(Some(m)) == crate::machine::Kind::Waterjet
        && let Some(o) = out.as_object_mut()
    {
        o.insert("kind".to_owned(), json!("waterjet"));
    }
    // #533: a wire or sinker EDM says its kind and its `edm` words (the
    // control's codes, D5, and whether it threads by itself) for the
    // EDM posts' machine check; every other mirror is unchanged. #542:
    // a sinker's `edm` also carries what the sinker post reads - its
    // orbit words, pickup and condition table (lengths already mm) - so
    // a wire mirror keeps main's two keys, plus its condition table when
    // the file states one (#3214).
    let k = crate::machine::kind(Some(m));
    if matches!(k, crate::machine::Kind::WireEdm | crate::machine::Kind::SinkerEdm)
        && let Some(o) = out.as_object_mut()
    {
        o.insert("kind".to_owned(), json!(k.word()));
        let awt = m.edm.as_ref().and_then(|e| e.awt);
        let mut edm = json!({ "codes": m.edm_codes, "awt": awt });
        if k == crate::machine::Kind::SinkerEdm
            && let (Some(e), Some(eo)) = (m.edm.as_ref(), edm.as_object_mut())
        {
            eo.insert("orbits".to_owned(), json!(e.orbits));
            eo.insert("pickup".to_owned(), json!(e.pickup));
            eo.insert("conditions".to_owned(), serde_json::to_value(&e.conditions).unwrap_or_else(|_| json!([])));
        } else if let (Some(e), Some(eo)) = (m.edm.as_ref(), edm.as_object_mut())
            && !e.conditions.is_empty()
        {
            // #3214: a wire's stated condition table (gaps already mm), so
            // the Sodick post can state the gap the kerf used and flag an
            // EXAMPLE row. Only when stated: a wire file with no table
            // mirrors exactly as before.
            eo.insert("conditions".to_owned(), serde_json::to_value(&e.conditions).unwrap_or_else(|_| json!([])));
        }
        // #1499: a wire's stated tank and flushing, as their words - only
        // when stated, so a mirror that states neither is main's.
        if let (Some(e), Some(eo)) = (m.edm.as_ref(), edm.as_object_mut()) {
            if let Some(t) = e.tank {
                eo.insert("tank".to_owned(), json!(t.word()));
            }
            if let Some(f) = e.flushing {
                eo.insert("flushing".to_owned(), json!(f.word()));
            }
            // #2686: the skim passes' flushing, only when stated.
            if let Some(f) = e.skim_flushing {
                eo.insert("skim_flushing".to_owned(), json!(f.word()));
            }
        }
        o.insert("edm".to_owned(), edm);
    }
    let inverted = m.inverted_axes();
    if !inverted.is_empty()
        && let Some(o) = out.as_object_mut()
    {
        o.insert("invertedAxes".to_owned(), json!(inverted));
    }
    if m.coordinate_rotation == Some(true)
        && let Some(o) = out.as_object_mut()
    {
        o.insert("coordinateRotation".to_owned(), json!(true));
    }
    if let Some(o) = out.as_object_mut() {
        super::machine_rotary::mirror_controller(m, o);
        super::machine_installation::mirror(m, o);
    }
    out
}

/// Walk a path of object keys; `None` on any missing or mismatched hop.
fn get<'a>(v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cur = v;
    for k in path {
        cur = cur.get(*k)?;
    }
    Some(cur)
}

fn f64_or(v: Option<&Value>, default: f64) -> f64 {
    v.and_then(|x| x.as_f64()).unwrap_or(default)
}

/// Return the properties of the script's `machine` global. Direct callers
/// receive `{"error": reason}` if the installation facts fail validation.
/// The post engine returns that error before it runs any script hooks.
pub fn script_props(m: &Value) -> Value {
    match super::machine_installation::checked_input(m) {
        Ok(input) => flatten(&input),
        Err(reason) => json!({ "error": reason }),
    }
}

/// Project the machine prepared by the engine into the JavaScript host's properties.
pub(super) fn flatten(m: &Value) -> Value {
    // An axes-array form contributes the max rapid across axes (the
    // post uses rapidRate as a ceiling, so the max is the permissive
    // bound).
    let axes_rapid = get(m, &["kinematics", "static", "axes"])
        .and_then(Value::as_array)
        .map(|axes| {
            axes.iter()
                // A raw file may list its rotaries among the axes;
                // their deg/min is not a prismatic ceiling (stage C's
                // unit law, kept through the raw door too).
                .filter(|a| !is_rotary_letter(a.get("letter").and_then(Value::as_str)))
                .map(|a| f64_or(get(a, &["speeds", "rapid"]), 0.0))
                .fold(0.0_f64, f64::max)
        })
        .unwrap_or(0.0);
    let rapid = f64_or(get(m, &["rapidRate"]), 0.0).max(axes_rapid);
    let feed = f64_or(get(m, &["feedRateMax"]), 0.0);
    let spindle = f64_or(
        get(m, &["kinematics", "static", "head", "spindle", "max"]).or(get(m, &["spindleMax"])),
        0.0,
    );
    let coolants: Vec<Value> = get(m, &["kinematics", "static", "head", "toolStation", "coolants"])
        .or(get(m, &["coolants"]))
        .and_then(Value::as_array)
        .map(|c| {
            c.iter()
                .map(|e| {
                    e.as_str().map(Value::from).unwrap_or_else(|| {
                        json!({
                            "name": get(e, &["name"]).and_then(Value::as_str).unwrap_or(""),
                            "mCode": get(e, &["mCode"]).and_then(Value::as_str).unwrap_or(""),
                        })
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let (rotaries, rtcp, sign_convention) = super::machine_rotary::props_rotaries_and_rtcp(m);
    // The spindle inventory (M5.3 C): the mirror's stated count, the
    // serde mirror's field, or - through the raw door - the same
    // whole-tree head count `parse_mch` keeps. Unstated stays
    // absent: a dialect script asks `machine.spindles > 1` and gets
    // truth about what the file states, never a number invented for
    // a machine that said nothing.
    let spindles = get(m, &["kinematics", "static", "head", "spindleCount"])
        .or_else(|| m.get("spindles"))
        .and_then(Value::as_u64)
        .or_else(|| {
            let n = spindle_heads(get(m, &["kinematics", "default", "parts"]));
            (n > 0).then_some(n as u64)
        });
    let mut props = json!({
        "name": get(m, &["name"]).and_then(Value::as_str).unwrap_or(""),
        "capabilities": get(m, &["capabilities"]).cloned().unwrap_or_else(|| json!({})),
        "rapidRate": f64::max(rapid, axes_rapid),
        "feedRateMax": feed,
        "spindleMax": spindle,
        "spindleMin": f64_or(m.pointer("/kinematics/static/head/spindle/min").or(m.get("spindleMin")), 0.0),
        "coolants": coolants,
        "rotaries": rotaries,
        // #1882: the linear axes' stated strokes, mm ([min, max] or null).
        "travel": super::machine_travel::linear(m),
        "rtcp": rtcp,
        "signConvention": sign_convention,
        "spindles": spindles.map_or(Value::Null, |n| json!(n)),
        "units": if units_inch(m) { "inch" } else { "mm" },
        // #170: the speed ranges a hand belt/gear change picks between
        // ([] when unstated), and whether a rigid tap is possible (true
        // unless the machine says it is not - the unstated-limit law).
        "spindleRanges": get(m, &["kinematics", "static", "head", "spindleRanges"])
            .or(get(m, &["spindleRanges"]))
            .filter(|v| v.is_array())
            .cloned()
            .unwrap_or_else(|| json!([])),
        // Issue #33: the builder's C-axis live-tool words, or null.
        "liveTool": get(m, &["kinematics", "static", "head", "liveTool"])
            .or(get(m, &["liveTool"]))
            .filter(|v| v.is_object())
            .cloned()
            .unwrap_or(Value::Null),
        "rigidTapping": get(m, &["kinematics", "static", "head", "rigidTapping"])
            .or(get(m, &["rigidTapping"]))
            .and_then(Value::as_bool)
            .unwrap_or(true),
        // #2303: true only when the file STATES rigid tapping (a spindle
        // encoder); `rigidTapping` above reads unstated as "not limited",
        // which a post that needs the encoder cannot take as a yes.
        "rigidTappingStated": get(m, &["kinematics", "static", "head", "rigidTapping"])
            .or(get(m, &["rigidTapping"]))
            .and_then(Value::as_bool)
            == Some(true),
        // #495: the machine-coordinate Z a post retracts to (mm), or
        // null when the machine does not state one; `model` is the
        // machine's model name through either door (the mirror's
        // `name`, or a raw file's `general.model`), for the posts that
        // judge an unstated retract by whose machine it is.
        "safeRetractZ": safe_retract_z(m).map_or(Value::Null, |z| json!(z)),
        // #671: the declared kind's canonical word (`plasma`, `lathe`,
        // ...) through either door, "" when the file declares none.
        "kind": get(m, &["kind"])
            .or_else(|| get(m, &["general", "kind"]))
            .and_then(Value::as_str)
            .and_then(crate::machine::Kind::from_word)
            .map_or("", crate::machine::Kind::word),
        "model": get(m, &["name"])
            .and_then(Value::as_str)
            .filter(|n| !n.is_empty())
            .or_else(|| get(m, &["general", "model"]).and_then(Value::as_str))
            .unwrap_or(""),
        // #375: the inverted linear axes, for the header comment a post
        // that translates them writes ([] for every standard machine). A
        // statement the reader refuses was refused before the script ran.
        // #533: the EDM section's codes and AWT flag through either
        // door (the mirror's or a raw file's `edm`), null when unstated.
        // #542: a sinker's orbits, pickup and conditions ride the same
        // object; the sinker post reads only their words and ids (no
        // length), so the raw door needs no unit scaling here.
        "edm": get(m, &["edm"]).filter(|e| e.is_object()).cloned().unwrap_or(Value::Null),
        "invertedAxes": super::invert::stated(m).map(|(_, l)| l).unwrap_or_default(),
    });
    super::machine_installation::script_props(m, &mut props);
    props
}

/// The stated safe Z retract (#495), mm: the mirror's `safeRetractZ`,
/// or - through the raw door - the file form's Z part, read by the
/// same parser the app mounts with (so units scale the same way). A
/// raw file the parser refuses states no retract.
fn safe_retract_z(m: &Value) -> Option<f64> {
    if let Some(z) = get(m, &["safeRetractZ"]).and_then(Value::as_f64) {
        return z.is_finite().then_some(z);
    }
    if get(m, &["kinematics", "default", "parts"]).is_some() {
        return crate::machine::parse_mch(&m.to_string()).ok()?.safe_retract_z;
    }
    None
}

/// Whether the machine is programmed in inches (#131): the mirror's
/// `units` word, or - through the raw door - the `.mch` file's own
/// `kinematics.default.units.length`. Anything else, or nothing, is mm.
fn units_inch(m: &Value) -> bool {
    let word = get(m, &["units"])
        .and_then(Value::as_str)
        .or_else(|| get(m, &["kinematics", "default", "units", "length"]).and_then(Value::as_str));
    matches!(word, Some("inch" | "inches"))
}

/// Count `type: "head"` parts stating a spindle, depth-first - the
/// raw door for the M5.3 C inventory, the walk `parse_mch`'s
/// `count_spindles` makes over the same tree.
fn spindle_heads(parts: Option<&Value>) -> usize {
    let Some(arr) = parts.and_then(Value::as_array) else {
        return 0;
    };
    arr.iter()
        .map(|p| {
            usize::from(
                p.get("type").and_then(Value::as_str) == Some("head")
                    && p.get("spindle").is_some_and(|s| !s.is_null()),
            ) + spindle_heads(p.get("parts"))
        })
        .sum()
}
