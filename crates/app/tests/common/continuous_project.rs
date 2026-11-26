//! Compare the guide project with its construction, from our saved-project schema.
//! Generated cache coordinates have six decimal places; libm rounding can
//! change the final digit across supported platforms. All other values are exact.

use serde_json::Value;

pub fn compare(actual: &str, expected: &str) -> Result<(), String> {
    let actual: Value = serde_json::from_str(actual).map_err(|e| e.to_string())?;
    let expected: Value = serde_json::from_str(expected).map_err(|e| e.to_string())?;
    compare_value(&actual, &expected, "")
}

fn cached_coordinate(path: &str) -> bool {
    let keys: Vec<_> = path.trim_start_matches('/').split('/').collect();
    matches!(keys.as_slice(), ["entities", entity, "operation", "cache", "moves", movement, "from" | "to" | "axis", component]
        if entity.parse::<usize>().is_ok() && movement.parse::<usize>().is_ok()
            && component.parse::<usize>().is_ok_and(|i| i < 3))
}

fn compare_value(actual: &Value, expected: &Value, path: &str) -> Result<(), String> {
    if actual == expected {
        return Ok(());
    }
    let mismatch = || format!("{path}: {actual} != {expected}");
    match (actual, expected) {
        (Value::Object(a), Value::Object(b)) if a.keys().eq(b.keys()) => {
            for (key, value) in a {
                compare_value(value, &b[key], &format!("{path}/{key}"))?;
            }
            Ok(())
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare_value(a, b, &format!("{path}/{i}"))?;
            }
            Ok(())
        }
        (Value::Number(a), Value::Number(b)) if cached_coordinate(path) => {
            let a = a.as_f64().ok_or_else(mismatch)?;
            let b = b.as_f64().ok_or_else(mismatch)?;
            // One unit in the saved cache's last decimal, plus arithmetic roundoff.
            if a.is_finite() && b.is_finite() && (a - b).abs() <= 1.000_000_01e-6 {
                Ok(())
            } else {
                Err(mismatch())
            }
        }
        _ => Err(mismatch()),
    }
}

#[test]
fn project_comparison_accepts_one_cached_coordinate_rounding_step() {
    let fixture = include_str!("../../../../examples/first-continuous-4axis-part.camproj");
    let mut changed: Value = serde_json::from_str(fixture).unwrap();
    let entities = changed["entities"].as_array_mut().unwrap();
    let operation = entities.iter_mut().find(|e| e.get("operation").is_some()).unwrap();
    let coordinate = &mut operation["operation"]["cache"]["moves"][0]["to"][0];
    *coordinate = Value::from(coordinate.as_f64().unwrap() + 0.000_001);
    assert!(compare(&changed.to_string(), fixture).is_ok());
}

#[test]
fn project_comparison_rejects_stale_setup_and_larger_cache_changes() {
    let fixture = include_str!("../../../../examples/first-continuous-4axis-part.camproj");
    let original: Value = serde_json::from_str(fixture).unwrap();
    let entities = original["entities"].as_array().unwrap();
    let op = entities.iter().position(|e| e.get("operation").is_some()).unwrap();
    let tool = entities.iter().position(|e| e.get("tool").is_some()).unwrap();
    for (path, replacement) in [
        ("/version".to_string(), Value::from(67)),
        (
            "/version".to_string(),
            Value::from(original["version"].as_u64().unwrap() - 1),
        ),
        ("/machine/machine/axes/3/travel".to_string(), serde_json::json!([-360.0, 360.0])),
        (format!("/entities/{tool}/tool/number"), Value::Null),
        (format!("/entities/{op}/operation/cache/moves/0/to/0"), Value::from(-31.749_998)),
        (format!("/entities/{op}/operation/cache/moves/0/kind"), Value::from("feed")),
        (format!("/entities/{op}/operation/cache/moves/0/to"), serde_json::json!([-31.75, 0.0])),
        (format!("/entities/{op}/operation/cache/moves/0/axis/0"), Value::from("0")),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(&path).unwrap() = replacement;
        assert!(compare(&changed.to_string(), fixture).is_err(), "accepted {path}");
    }
}
