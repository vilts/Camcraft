//! Provenance: #1453's explicit tap hand, using our own starter catalog.
//! Gate map: covers tools::catalog_asset, tools::thread_hand.

use camcraft_tools::catalog_asset::{CATALOG_JSON, parse_catalog};
use camcraft_tools::thread_hand::ThreadHand;
use serde_json::{Value, json};

fn catalog_with(hand: Value) -> String {
    let mut value: Value = serde_json::from_str(CATALOG_JSON).expect("catalog JSON");
    let row = value["tools"].as_array_mut().expect("tools").iter_mut()
        .find(|r| r["name"] == "TAP-M6").expect("tap row");
    row["definition"]["thread_hand"] = hand;
    value.to_string()
}

#[test]
fn catalog_preserves_left_tap_and_defaults_legacy_tools_to_right() {
    let legacy = parse_catalog(CATALOG_JSON).expect("legacy catalog");
    assert!(legacy.iter().all(|(_, t)| t.thread_hand == ThreadHand::Right));
    let left = parse_catalog(&catalog_with(json!("left"))).expect("left tap catalog");
    let tap = left.iter().find(|(n, _)| n == "TAP-M6").expect("tap");
    assert_eq!(tap.1.thread_hand, ThreadHand::Left);
    assert!(tap.1.is_sane(), "left-handed tools are representable");
}

#[test]
fn catalog_refuses_invalid_hands_by_tool_and_field_name() {
    for bad in [json!("backwards"), json!(7), Value::Null, json!({"left": true})] {
        let err = parse_catalog(&catalog_with(bad.clone())).expect_err("bad hand");
        assert!(err.contains("TAP-M6") && err.contains("thread hand"), "{bad}: {err}");
    }
}
