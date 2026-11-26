//! #3449: `list_operation_types` names every field that a kind accepts -
//! the optional ones that the defaults do not write, and the words of an
//! enum field - so that an agent finds a feature from the tool alone.
//! Provenance: our own tests over the MCP listing of our params DTOs.

use super::Client;
use serde_json::{Value, json};

fn kinds(client: &mut Client) -> Vec<Value> {
    client.ok("list_operation_types", json!({}))["kinds"].as_array().cloned().unwrap_or_default()
}

fn field<'a>(kinds: &'a [Value], key: &str, name: &str) -> &'a Value {
    let kind = kinds.iter().find(|k| k["key"] == key).unwrap_or_else(|| panic!("kind {key} listed"));
    kind["fields"]
        .as_array()
        .and_then(|f| f.iter().find(|f| f["name"] == name))
        .unwrap_or_else(|| panic!("{key} lists {name}: {}", kind["fields"]))
}

fn values(f: &Value) -> Vec<&str> {
    f["values"].as_array().map(|v| v.iter().filter_map(Value::as_str).collect()).unwrap_or_default()
}

/// The Done clause: the trace reads each kind's params type, so a field
/// that a params struct gains is listed without a hand edit. A kind whose
/// params the trace cannot read says so with `fields_incomplete`.
#[test]
fn list_operation_types_reads_every_kind_completely() {
    let mut client = Client::new();
    let kinds = kinds(&mut client);
    assert!(kinds.len() > 40, "{}", kinds.len());
    for kind in &kinds {
        assert!(kind.get("fields_incomplete").is_none(), "{} cannot be traced: {kind}", kind["key"]);
        let fields = kind["fields"].as_array().expect("fields");
        let mut names: Vec<&str> = fields.iter().filter_map(|f| f["name"].as_str()).collect();
        assert_eq!(names.len(), fields.len(), "{kind}");
        names.dedup();
        assert_eq!(names.len(), fields.len(), "a field is listed twice: {kind}");
        for f in fields {
            assert!(f["type"].as_str().is_some_and(|t| !t.is_empty()), "{f}");
            if f["optional"] == json!(true) {
                assert!(f.get("default").is_some(), "{f}");
            }
        }
    }
}

#[test]
fn list_operation_types_names_the_optional_fields_and_enum_words() {
    let mut client = Client::new();
    let kinds = kinds(&mut client);

    // The 3+2 index and the rest stock of a pocket are optional.
    for name in ["tilt", "rest"] {
        assert_eq!(field(&kinds, "pocket", name)["optional"], json!(true), "pocket {name}");
    }
    let rest = field(&kinds, "pocket", "rest");
    assert!(rest["shape"].as_str().is_some_and(|s| s.contains("previous_tool_diameter")), "{rest}");
    assert!(values(field(&kinds, "pocket", "entry")).contains(&"helix"));

    // Wire Contour: the condition and the optional extras.
    for name in ["condition", "taper_deg", "tabs", "capture", "skims"] {
        field(&kinds, "wire_contour", name);
    }
    // A legacy key that the load reads but the file never writes is not offered.
    let wire = kinds.iter().find(|k| k["key"] == "wire_contour").expect("wire_contour");
    assert!(!wire["fields"].as_array().is_some_and(|f| f.iter().any(|f| f["name"] == "start")), "{wire}");
    let tabs = field(&kinds, "wire_contour", "tabs");
    assert!(tabs["shape"].as_str().is_some_and(|s| s.contains("cutoff_condition")), "{tabs}");

    // Plasma: feed and the pierce keys that an unset run posts without.
    for name in ["feed", "kerf", "pierce_delay", "pierce_height", "cut_height"] {
        assert_eq!(field(&kinds, "plasma_profile", name)["optional"], json!(true), "plasma {name}");
    }

    // An enum field lists its words.
    let cycle = values(field(&kinds, "drill", "cycle"));
    for word in ["auto", "spot", "peck", "tap", "back_bore", "fine_bore"] {
        assert!(cycle.contains(&word), "drill cycle {word}: {cycle:?}");
    }

    // The flattened kinds: the thread's pass keys and its own optional ones.
    for name in ["pass_schedule", "spring_passes", "pitch", "break_through"] {
        field(&kinds, "thread", name);
    }
    assert!(values(field(&kinds, "thread", "pass_schedule")).contains(&"constant_area"));
}
