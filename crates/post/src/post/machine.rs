//! The machine definition - our `.mch` role.
//!
//! Provenance: a JSON file carrying capability flags and a kinematics
//! tree (head → axes → table), a layout specced from Autodesk's
//! published post-processor documentation (behavior only, per the
//! Clean-Room Protocol). The grammar and every key name are ours.

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
    let mut rotaries: Vec<Value> = Vec::new();
    for a in &m.axes {
        if a.rotary {
            // The stage D door: a rotary mirrors as GEOMETRY - its
            // letter, its stated direction, its travel in degrees.
            // M5.1 stage G added its speeds - in a `speeds` object of
            // the rotary's OWN, stating degrees per minute under keys
            // the sim's rotary law reads as degrees. The unit lie
            // stage C refused was a deg/min rate absorbed into an
            // mm/min ceiling field, and that refusal stands: the
            // rotary joins no `axes` array, and `rapidRate`/
            // `feedRateMax` stay linear-only sums below. The RTCP
            // question a post asks ("does this machine tilt?") is
            // still answered by the direction, not the rate; the
            // rate exists now because the timeline turns angles, and
            // a paced simulation has to know how fast.
            let mut r = serde_json::Map::new();
            r.insert("letter".to_owned(), json!(a.name));
            r.insert("type".to_owned(), json!("rotary"));
            if let Some(d) = a.axis {
                r.insert("direction".to_owned(), json!([d[0], d[1], d[2]]));
            }
            if let Some((lo, hi)) = a.travel {
                r.insert("travel".to_owned(), json!([lo, hi]));
            }
            let mut speeds = serde_json::Map::new();
            if let Some(f) = a.feed {
                speeds.insert("normal".to_owned(), json!(f));
            }
            if let Some(rap) = a.rapid {
                speeds.insert("rapid".to_owned(), json!(rap));
            }
            if !speeds.is_empty() {
                r.insert("speeds".to_owned(), Value::Object(speeds));
            }
            rotaries.push(Value::Object(r));
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
        axes.push(json!({
            "letter": a.name,
            "direction": direction,
            "speeds": Value::Object(speeds),
        }));
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
    // #533: a wire or sinker EDM says its kind and its `edm` words (the
    // control's codes, D5, and whether it threads by itself) for the
    // EDM posts' machine check; every other mirror is unchanged. #542:
    // a sinker's `edm` also carries what the sinker post reads - its
    // orbit words, pickup and condition table (lengths already mm) - so
    // a wire mirror keeps exactly main's two keys.
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
        }
        o.insert("edm".to_owned(), edm);
    }
    let inverted = m.inverted_axes();
    if !inverted.is_empty()
        && let Some(o) = out.as_object_mut()
    {
        o.insert("invertedAxes".to_owned(), json!(inverted));
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

/// The flattened object the script's `machine` global sees, per the
/// format doc: name, rate maxima, spindle bound, coolant names.
pub fn script_props(m: &Value) -> Value {
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
    // The rotary inventory (stage D): the mirror's geometry section,
    // or - through the raw door - the axes that carry an A/B/C
    // letter. RTCP is then a geometric question, not a naming one:
    // the machine can tilt only if some rotary states a direction
    // that is not the Z column. An unstated direction tilts nothing
    // a post may promise, and `machine.rtcp` says so.
    let mut rotaries: Vec<Value> = Vec::new();
    if let Some(rs) = get(m, &["kinematics", "static", "rotaries"]).and_then(Value::as_array) {
        for r in rs {
            // Issue #33: the rotary's own deg/min rates ride under its
            // own `speeds` key (null when unstated) so a C-axis post can
            // cap a composite feed by the C rate the file states. They
            // stay out of every mm/min field (stage C's unit law).
            rotaries.push(json!({
                "name": get(r, &["letter"]).and_then(Value::as_str).unwrap_or(""),
                "direction": get(r, &["direction"]).cloned().unwrap_or(Value::Null),
                "travel": get(r, &["travel"]).cloned().unwrap_or(Value::Null),
                "speeds": get(r, &["speeds"]).filter(|s| s.is_object()).cloned().unwrap_or(Value::Null),
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
                }));
            }
        }
    }
    let rtcp = rotaries.iter().any(|r| {
        r.get("direction")
            .and_then(Value::as_array)
            .filter(|d| d.len() == 3)
            .and_then(|d| {
                let [x, y, z] = [
                    d[0].as_f64()?,
                    d[1].as_f64()?,
                    d[2].as_f64()?,
                ];
                Some(z * z < 0.99 * (x * x + y * y + z * z))
            })
            .unwrap_or(false)
    });
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
    json!({
        "name": get(m, &["name"]).and_then(Value::as_str).unwrap_or(""),
        "capabilities": get(m, &["capabilities"]).cloned().unwrap_or_else(|| json!({})),
        "rapidRate": f64::max(rapid, axes_rapid),
        "feedRateMax": feed,
        "spindleMax": spindle,
        "coolants": coolants,
        "rotaries": rotaries,
        "rtcp": rtcp,
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
    })
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

/// The RS-274 rotary addresses: A turns about X, B about Y, C about Z.
/// Everything else in an axes list is linear by our grammar (a Q
/// slide is a slide - stage C's stated law, re-echoed at the raw
/// door, where letters are all a bare file says).
fn is_rotary_letter(letter: Option<&str>) -> bool {
    matches!(letter, Some("A") | Some("B") | Some("C"))
}
