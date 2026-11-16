//! The `.mch` reader: JSON in, Machine out, with the file-form grammar
//! documented in `assets/machines/AGENTS.md`. Refusals name their
//! reason (Prime Directive); unstated numbers read `None`.
//!
//! Provenance: our own parser, specced from the observable shape of a
//! machine file and RS-274/ISO 6983 axis-lettering convention.

use super::def::{AxisLimits, Coolant, LiveToolCodes, Machine};
use super::direction::AxisDirection;

/// The number a machine field reads as, with the unstated-limit law:
/// absent, null, non-numeric, or exactly zero reads `None`; a
/// negative or NaN reads `Some` only for travel bounds (where a
/// negative stroke limit is the point), and `None` for rates (where
/// it is a broken file, checked at the point of use).
fn rate(v: Option<&serde_json::Value>) -> Option<f64> {
    let x = v?.as_f64()?;
    if x.is_finite() && x > 0.0 {
        Some(x)
    } else {
        None
    }
}

/// Length-unit factor: a file declaring inches multiplies every
/// distance - and every distance-per-minute - by 25.4 on the way in,
/// so the whole checker stays mm-only.
fn mm_factor(units: Option<&serde_json::Value>) -> Option<f64> {
    match units?.get("length")?.as_str()? {
        "inch" | "inches" => Some(25.4),
        _ => Some(1.0),
    }
}

/// Walk a part list collecting every `"type": "linear"` and (since
/// M3.3 stage C) `"type": "rotary"` part by name, depth-first (the
/// standard shape nests Y inside X inside Z - the chain is the
/// kinematics, but the limit table is flat; a rotary trunnion hangs
/// off the table the same way). The flag says which kind of motion
/// the part states, not a letter the reader has to stereotype.
fn collect_axes(
    parts: Option<&serde_json::Value>,
    out: &mut Vec<(String, serde_json::Value, bool)>,
) {
    let Some(arr) = parts.and_then(|p| p.as_array()) else { return };
    for p in arr {
        let ty = p.get("type").and_then(|t| t.as_str());
        let rotary = ty == Some("rotary");
        let name = p
            .get("name")
            .and_then(|n| n.as_str())
            .or_else(|| p.get("id").and_then(|n| n.as_str()))
            .unwrap_or("")
            .to_owned();
        if (ty == Some("linear") || rotary) && !name.is_empty() {
            out.push((name, p.clone(), rotary));
        }
        collect_axes(p.get("parts"), out);
    }
}

/// The coolants a tool station states, in either shape the grammar
/// takes: bare name strings (a named option, no code - posts
/// nothing), or `{name, mCode}` objects. The code may itself be a
/// string ("M8") or a bare number (8), normalized to the word form.
/// `tool_station.live_tool` (issue #33): the four builder words, all
/// non-empty strings, or None (a partial set states nothing usable).
fn parse_live_tool(v: Option<&serde_json::Value>) -> Option<LiveToolCodes> {
    let v = v?;
    let word = |k: &str| {
        v.get(k)
            .and_then(|w| w.as_str())
            .map(str::trim)
            .filter(|w| !w.is_empty())
            .map(str::to_owned)
    };
    Some(LiveToolCodes {
        c_axis_on: word("c_axis_on")?,
        c_axis_off: word("c_axis_off")?,
        spindle_on: word("spindle_on")?,
        spindle_off: word("spindle_off")?,
    })
}

fn parse_coolants(v: Option<&serde_json::Value>) -> Vec<Coolant> {
    let Some(arr) = v.and_then(|a| a.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|e| {
            if let Some(name) = e.as_str() {
                return Some(Coolant {
                    name: name.to_owned(),
                    m_code: String::new(),
                });
            }
            let name = e.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let code = match e.get("mCode").or_else(|| e.get("mcode")) {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(serde_json::Value::Number(n)) => format!("M{n}"),
                _ => String::new(),
            };
            (!name.is_empty()).then_some(Coolant {
                name: name.to_owned(),
                m_code: code,
            })
        })
        .collect()
}

/// How many `type: "head"` parts anywhere in the tree state a
/// spindle (M5.3 C). The limit numbers still read the first match
/// (`find_type` above) - this is the inventory for the handoff
/// check, so it counts them all.
fn count_spindles(parts: Option<&serde_json::Value>) -> usize {
    let Some(arr) = parts.and_then(|p| p.as_array()) else {
        return 0;
    };
    arr.iter()
        .map(|p| {
            usize::from(
                p.get("type").and_then(|t| t.as_str()) == Some("head")
                    && p.get("spindle").is_some_and(|s| !s.is_null()),
            ) + count_spindles(p.get("parts"))
        })
        .sum()
}

/// The spindle's speed ranges (#170): `[{ "min_speed", "max_speed" }]`,
/// rpm. An entry missing either bound, or with them out of order, is
/// dropped - a range the file cannot state whole names nothing.
fn parse_ranges(v: Option<&serde_json::Value>) -> Vec<(f64, f64)> {
    let Some(arr) = v.and_then(|a| a.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|r| {
            let lo = rate(r.get("min_speed"))?;
            let hi = rate(r.get("max_speed"))?;
            (lo < hi).then_some((lo, hi))
        })
        .collect()
}

/// First part anywhere in the tree with the given `type`, depth-first.
fn find_type<'a>(parts: &'a serde_json::Value, ty: &str) -> Option<&'a serde_json::Value> {
    let arr = parts.as_array()?;
    for p in arr {
        if p.get("type").and_then(|t| t.as_str()) == Some(ty) {
            return Some(p);
        }
        // #785: a leaf part (no `parts`, e.g. a 4th-axis rotary listed
        // before the head) is passed over, never the end of the search -
        // a `?` here lost the head, and every spindle and coolant figure
        // with it, for any file with a leaf ahead of its head.
        if let Some(hit) = p.get("parts").and_then(|c| find_type(c, ty)) {
            return Some(hit);
        }
    }
    None
}

/// Parse a `.mch` machine file. Malformed JSON refuses; a well-formed
/// file that describes nothing answers with an all-`None` Machine -
/// an empty machine checks nothing, which is the truth rather than a
/// silently stricter one. The holder library is the caller's (#828):
/// this door judges `tooling.default_holder` as a word only - the app's
/// reader passes its library through [`parse_mch_with`].
pub fn parse_mch(text: &str) -> Result<Machine, String> {
    parse_mch_with(text, None)
}

/// [`parse_mch`] with the caller's holder library (#828), so an unknown
/// default holder, or one off the stated mount, is refused by name.
pub fn parse_mch_with(text: &str, holders: Option<super::HolderLookup>) -> Result<Machine, String> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("machine file: {e}"))?;
    if !v.is_object() {
        return Err("machine file: expected a machine object".to_owned());
    }

    let general = v.pointer("/general");
    let mut m = Machine {
        model: text_of(general.and_then(|g| g.get("model"))),
        vendor: text_of(general.and_then(|g| g.get("vendor"))),
        ..Default::default()
    };

    // The declared kind (M3.3 C) and its per-kind door rules.
    let declared = super::kind::declared_kind(&v)?;
    m.kind = declared.as_ref().map(|(_, w)| w.clone());

    // #828: the spindle mount and the default holder, refused by name.
    let (mount, holder) = super::tooling::parse_tooling(&v, holders).map_err(|e| format!("machine file: {e}"))?;
    m.spindle_mount = mount;
    m.default_holder = holder;

    let kine = v.pointer("/kinematics/default/parts");
    let f = mm_factor(v.pointer("/kinematics/default/units")).unwrap_or(1.0);
    // #131: keep the stated unit for the posts (the numbers stay mm).
    let length = v.pointer("/kinematics/default/units/length").and_then(|u| u.as_str());
    if matches!(length, Some("inch" | "inches")) {
        m.units = Some("inch".to_owned());
    }

    // #530: the EDM section - only an EDM kind states one, and only
    // its own kind's keys; anything else refuses the file by name.
    m.edm = super::edm::parse_edm(v.get("edm"), f, declared.map(|(k, _)| k))?;
    // #533: the control's own words (D5), read beside the section.
    m.edm_codes = super::edm_codes::parse_codes(v.get("edm"))?;

    if let Some(parts) = kine {
        if let Some(head) = find_type(parts, "head") {
            let sp = head.get("spindle");
            m.spindle_min = rate(sp.and_then(|s| s.get("min_speed")));
            m.spindle_max = rate(sp.and_then(|s| s.get("max_speed")));
            m.spindle_power = rate(sp.and_then(|s| s.get("power")));
            m.spindle_ranges = parse_ranges(sp.and_then(|s| s.get("ranges")));
            // #786: a stated capability gates G84, so a word the reader
            // cannot read refuses the file - never a silent "not limited".
            m.rigid_tapping = match sp.and_then(|s| s.get("rigid_tapping")) {
                None | Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::Bool(b)) => Some(*b),
                Some(x) => return Err(format!("machine file: spindle.rigid_tapping must be true or false, not {x}")),
            };
            let ts = head.get("tool_station");
            m.max_tool_length = scale(rate(ts.and_then(|t| t.get("max_tool_length"))), f);
            m.max_tool_diameter = scale(rate(ts.and_then(|t| t.get("max_tool_diameter"))), f);
            m.spindle_d = scale(rate(sp.and_then(|s| s.get("body_diameter"))), f);
            m.spindle_len = scale(rate(sp.and_then(|s| s.get("body_length"))), f);
            m.coolants = parse_coolants(ts.and_then(|t| t.get("coolants")));
            m.live_tool = parse_live_tool(ts.and_then(|t| t.get("live_tool")));
        }
        if m.coolants.is_empty() {
            // Some files keep the option list at the tooling root
            // rather than on the station; either home reads.
            m.coolants = parse_coolants(v.pointer("/tooling/coolants"));
        }
        // The spindle inventory (M5.3 C): heads that state a spindle,
        // counted whole-tree; no head states one, no count is stated.
        let nsp = count_spindles(Some(parts));
        m.spindles = (nsp > 0).then_some(nsp);
        // The changer (stage C's capacity number): the tooling tree's
        // `number_of_tools`, a count the file states directly.
        m.tool_count = v
            .pointer("/tooling/number_of_tools")
            .and_then(|n| n.as_u64())
            .map(|n| n as usize)
            .filter(|n| *n > 0);

        let mut listed: Vec<(String, serde_json::Value, bool)> = Vec::new();
        collect_axes(Some(parts), &mut listed);

        // Controller speeds, keyed by axis name; the kinematics list
        // decides WHICH axes exist, the controller says how fast they
        // go, and a `"travel"` object (our extension) says how far.
        let ctrl = v.pointer("/controller/synced_configuration/parts");
        let mut axes: Vec<AxisLimits> = Vec::new();
        for (name, part, rotary) in &listed {
            let c = ctrl.and_then(|c| c.get(name.as_str()));
            // Travel: a length scales with the file's units, an
            // angle is an angle (the length factor would lie). Rates
            // the same law: mm/min scales, deg/min does not.
            let scale_v = if *rotary { 1.0 } else { f };
            let travel = part.get("travel").and_then(|t| {
                let lo = t.get("min")?.as_f64()?;
                let hi = t.get("max")?.as_f64()?;
                if lo.is_finite() && hi.is_finite() && hi > lo {
                    Some((lo * scale_v, hi * scale_v))
                } else {
                    None
                }
            });
            let resolution = scale(rate(part.get("resolution")), scale_v);
            // A rotary's stated rotation direction (stage D): three
            // finite numbers or a refusal that names the axis - a
            // garbled tilt vector must never quietly become "no
            // tilt", because the whole RTCP judgment rides on it.
            let mut axis = None;
            if let Some(a) = part.get("axis") {
                let d = a.as_array().filter(|v| v.len() == 3).map(|v| {
                    [
                        v[0].as_f64(),
                        v[1].as_f64(),
                        v[2].as_f64(),
                    ]
                });
                match d {
                    Some([Some(x), Some(y), Some(z)]) => {
                        if *rotary && x == 0.0 && y == 0.0 && z == 0.0 {
                            return Err(format!(
                                "machine file: rotary axis {name} states the zero direction"
                            ));
                        }
                        axis = Some([x, y, z]);
                    }
                    _ => {
                        return Err(format!(
                            "machine file: axis {name} direction must be three finite numbers"
                        ));
                    }
                }
            }
            let direction = axis_direction(name, part, *rotary)?;
            let nose_to_axis = super::swing::nose_to_axis(name, part, *rotary, f)?;
            let axis_to_table = super::swing::work_length("axis_to_table", name, part, *rotary, f)?;
            if let Some(z) = safe_retract(name, part, *rotary, f)? {
                m.safe_retract_z = Some(z);
            }
            axes.push(AxisLimits {
                name: name.clone(),
                rotary: *rotary,
                feed: scale(rate(c.and_then(|c| c.get("max_normal_speed"))), scale_v),
                rapid: scale(rate(c.and_then(|c| c.get("max_rapid_speed"))), scale_v),
                travel,
                resolution,
                axis,
                direction,
                turns_part: rotary.then(|| super::swing::turns_part(part, true)),
                nose_to_axis,
                axis_to_table,
            });
        }
        axes.sort_by_key(|a| ctrl_coord(ctrl, &a.name));
        m.axes = axes;
    }
    Ok(m)
}

/// A part's `"direction"` (#373). Linear: absent or null is standard,
/// exactly `1` / `-1` states it, anything else refuses naming the axis -
/// a garbled sign must never read as "standard". Rotary: a numeric `-1`
/// refuses (a reversed rotary is not supported); any other value is the
/// rotary's own business (`axis` states its geometry) and is not read.
fn axis_direction(name: &str, part: &serde_json::Value, rotary: bool) -> Result<AxisDirection, String> {
    let v = match part.get("direction") {
        None | Some(serde_json::Value::Null) => return Ok(AxisDirection::Standard),
        Some(v) => v,
    };
    let sign = v.as_f64().and_then(AxisDirection::from_sign);
    if rotary {
        return match sign {
            Some(AxisDirection::Inverted) => Err(format!(
                "machine file: rotary axis {name} states direction -1; a reversed rotary is not supported"
            )),
            _ => Ok(AxisDirection::Standard),
        };
    }
    sign.ok_or_else(|| format!("machine file: axis {name} direction must be +1 or -1, not {v}"))
}

/// A part's `"safe_retract"` (#495): the machine-coordinate Z a post
/// retracts to (`G53 G0 Z..`), in the file's length unit. Absent or
/// null is unstated; a finite number on the linear Z states it (zero
/// included - it is a position, not a limit); anything else, or the
/// key on any other axis, refuses naming the axis, because a garbled
/// retract height must never read as "unstated" and fall back to a
/// guess about which way is up.
fn safe_retract(name: &str, part: &serde_json::Value, rotary: bool, f: f64) -> Result<Option<f64>, String> {
    let v = match part.get("safe_retract") {
        None | Some(serde_json::Value::Null) => return Ok(None),
        Some(v) => v,
    };
    if rotary || name != "Z" {
        return Err(format!(
            "machine file: axis {name} states safe_retract - only the linear Z axis states one"
        ));
    }
    match v.as_f64() {
        Some(z) if z.is_finite() => Ok(Some(z * f)),
        _ => Err(format!("machine file: axis Z safe_retract must be a number, not {v}")),
    }
}

fn text_of(v: Option<&serde_json::Value>) -> String {
    v.and_then(|x| x.as_str()).unwrap_or("unknown").to_owned()
}

fn scale(v: Option<f64>, f: f64) -> Option<f64> {
    v.map(|x| x * f)
}

fn ctrl_coord(ctrl: Option<&serde_json::Value>, name: &str) -> i64 {
    ctrl.and_then(|c| c.get(name))
        .and_then(|a| a.get("coordinate"))
        .and_then(|x| x.as_i64())
        .unwrap_or(i64::MAX)
}

