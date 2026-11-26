//! #3991: `edit_operation` sets the tangent arc lead radius and
//! `get_job` reads it back; a pair the arc lead is not cut with is
//! refused at the command door, and so is a radius not over the kerf
//! offset; a model's profile refuses the arc lead
//! by name at generation; undo restores the straight lead.

use super::*;

fn lead(c: &mut Client) -> Value {
    c.ok("get_job", json!({}))["operations"][0]["params"]["wire_contour"]["lead_arc_radius"].clone()
}

#[test]
fn edit_operation_sets_the_arc_lead_radius_and_refuses_what_it_is_not_cut_with() {
    let mut c = wire_job("arc-lead");
    c.ok("add_operation", wire_args("C110"));
    assert_eq!(lead(&mut c), Value::Null, "a new wire cut has the straight lead");
    c.ok("edit_operation", json!({ "name": "W", "params": { "lead_arc_radius": 0.75 } }));
    assert_eq!(lead(&mut c), json!(0.75));

    // Beside a taper, the arc lead is refused at the door, and nothing changes.
    let why = c.refused("edit_operation", json!({ "name": "W", "params": { "taper_deg": 2.0 } }));
    assert!(why.contains("tangent arc lead") && why.contains("taper"), "{why}");
    assert_eq!(lead(&mut c), json!(0.75));
    // A radius not over C110's kerf offset (0.165 mm) is refused at the
    // same door, and nothing changes (no undo step either: see below).
    let why = c.refused("edit_operation", json!({ "name": "W", "params": { "lead_arc_radius": 0.1 } }));
    assert!(why.contains("must be larger than the kerf offset (0.1650 mm)"), "{why}");
    assert_eq!(lead(&mut c), json!(0.75));
    // A model's section is not a selected DXF chain: refused by name.
    let (err, body) = c.call("generate", json!({ "name": "W" }));
    let text = body.to_string();
    assert!(err && text.contains("not a selected DXF chain"), "{err}: {text}");

    // A null radius is the straight lead again; undo restores the radius,
    // and a second undo the straight lead.
    c.ok("edit_operation", json!({ "name": "W", "params": { "lead_arc_radius": null } }));
    assert_eq!(lead(&mut c), Value::Null);
    c.ok("undo", json!({}));
    assert_eq!(lead(&mut c), json!(0.75));
    c.ok("undo", json!({}));
    assert_eq!(lead(&mut c), Value::Null);
}
