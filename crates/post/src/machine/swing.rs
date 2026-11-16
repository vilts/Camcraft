//! The rotary facts the index-swing clause reads (#804): whether a
//! rotary part turns the work or the spindle, and the stated height of
//! the spindle nose above its centerline at the safe-retract Z, and (#1107)
//! the centerline's height above the face the rotary is bolted to.
//!
//! Provenance: our own reading of our own `.mch` grammar
//! (`assets/machines/AGENTS.md`); a 4th-axis table turning the work
//! about X under a retracted spindle is general machine-tool practice
//! (Tormach TD10749 / 4th-axis comparison sheet 02/24, behaviour only).

/// Does any part at or below `part` state `type: "head"`? A rotary that
/// carries the head tilts the tool; one that does not turns the work.
fn carries_head(part: &serde_json::Value) -> bool {
    part.get("type").and_then(|t| t.as_str()) == Some("head")
        || part
            .get("parts")
            .and_then(|p| p.as_array())
            .is_some_and(|arr| arr.iter().any(carries_head))
}

/// Whether a rotary part turns the work (#804): it is a rotary and no
/// head hangs below it.
pub(super) fn turns_part(part: &serde_json::Value, rotary: bool) -> bool {
    rotary && !carries_head(part)
}

/// A part's `"nose_to_axis"` (#804), in mm (`f` is the file's length
/// factor) - see [`work_length`].
pub(super) fn nose_to_axis(name: &str, part: &serde_json::Value, rotary: bool, f: f64) -> Result<Option<f64>, String> {
    work_length("nose_to_axis", name, part, rotary, f)
}

/// A work-turning rotary's length figure `key` (`"nose_to_axis"`, #804;
/// `"axis_to_table"`, #1107), in mm (`f` is the file's length factor).
/// Absent or null is unstated; a finite positive number on a
/// work-turning rotary states it; anything else, or the key on any other
/// part, refuses naming the axis - a garbled clearance must never read
/// as "unstated" or as a number nobody wrote.
pub(super) fn work_length(
    key: &str,
    name: &str,
    part: &serde_json::Value,
    rotary: bool,
    f: f64,
) -> Result<Option<f64>, String> {
    let v = match part.get(key) {
        None | Some(serde_json::Value::Null) => return Ok(None),
        Some(v) => v,
    };
    if !turns_part(part, rotary) {
        return Err(format!(
            "machine file: axis {name} states {key} - only a rotary that turns the work states one"
        ));
    }
    match v.as_f64() {
        Some(d) if d.is_finite() && d > 0.0 => Ok(Some(d * f)),
        _ => Err(format!("machine file: axis {name} {key} must be a positive number, not {v}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_rotary_with_the_head_below_it_turns_the_tool() {
        let head_a = json!({ "type": "rotary", "parts": [{ "type": "rotary", "parts": [{ "type": "head" }] }] });
        assert!(!turns_part(&head_a, true));
        let table_a = json!({ "type": "rotary" });
        assert!(turns_part(&table_a, true));
        assert!(!turns_part(&table_a, false), "a linear part never turns the work");
    }

    #[test]
    fn nose_to_axis_reads_scaled_or_refuses_by_name() {
        let a = json!({ "type": "rotary", "nose_to_axis": 10.0 });
        assert_eq!(nose_to_axis("A", &a, true, 25.4), Ok(Some(254.0)));
        assert_eq!(nose_to_axis("A", &json!({ "type": "rotary" }), true, 1.0), Ok(None));
        let bad = json!({ "type": "rotary", "nose_to_axis": -1.0 });
        assert!(nose_to_axis("A", &bad, true, 1.0).is_err_and(|e| e.contains("axis A")));
        let on_z = json!({ "type": "linear", "nose_to_axis": 5.0 });
        assert!(nose_to_axis("Z", &on_z, false, 1.0).is_err_and(|e| e.contains("axis Z")));
        // #1107: the centre height reads by the same law, under its own key.
        let a = json!({ "type": "rotary", "axis_to_table": 3.15 });
        assert_eq!(work_length("axis_to_table", "A", &a, true, 25.4), Ok(Some(3.15 * 25.4)));
        let bad = json!({ "type": "rotary", "axis_to_table": "low" });
        assert!(work_length("axis_to_table", "A", &bad, true, 1.0).is_err_and(|e| e.contains("axis_to_table")));
        let on_z = json!({ "type": "linear", "axis_to_table": 5.0 });
        assert!(work_length("axis_to_table", "Z", &on_z, false, 1.0).is_err_and(|e| e.contains("axis Z")));
    }
}
