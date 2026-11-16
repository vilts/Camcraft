//! The `wire-iso.cps` script suite (#533, the per-script-suite law of
//! #926): the post binary on every recorded edge-case stream
//! (`recorded/streams/*.cnc`), its program or its named refusal compared
//! with the golden in `recorded/golden/wire-iso/`. The recorded streams
//! are cutting streams posted machine-less (or on a mill), so the wire
//! post refuses each by name - it posts only wire EDM work on a machine
//! declared `wire_edm`; its wire program is pinned by app's `wire_post`.
//!
//! Regenerate after an intentional change, then read the diff:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-post --test script_wire_iso`
//!
//! Gate map (M4.4): covers assets/posts/wire-iso.cps, camcraft_post::post::engine,
//! camcraft_post::post::cnc, the post binary.

#[macro_use]
mod recorded;

post_suite!("wire-iso.cps");

// #1164 review (PR #1218): the angle-word refusals of the script, on
// hand-built streams over the synthetic fixture (crates/app/tests/edm,
// its `uv` swapped for an invented angle template). The app refuses
// most of these first; the script is the guard for any other stream.

use camcraft_post::cl::{ArcPlane, MoveKind};
use camcraft_post::post::cnc::{self, MoveRec, Record};
use camcraft_post::post::engine;
use serde_json::{json, Value};

fn read(rel: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// The fixture, U/V (`angle` false) or angle-word with `template`.
fn machine(angle: Option<&str>) -> Value {
    let mut m: Value = serde_json::from_str(&read("crates/app/tests/edm/wire-iso-fixture.mch")).expect("fixture json");
    if let Some(t) = angle {
        let codes = m["edm"]["codes"].as_object_mut().expect("codes");
        codes.remove("uv");
        codes.insert("taper_angle".into(), json!(t));
    }
    m
}

fn mv(kind: MoveKind, x: f64, y: f64) -> Record {
    let feed = (kind == MoveKind::Feed).then_some(2.0);
    Record::Move(MoveRec { kind, x, y, z: 0.0, feed, center: None, ccw: false, plane: ArcPlane::Xy, abc: None })
}

/// One wire section with the `params` before it and one angle-worded
/// feed (`WireAngle` 3) in it.
fn stream(params: &[(&str, Value)]) -> String {
    let mut recs = vec![
        Record::Param { name: "OperationName".into(), value: json!("Punch1") },
        Record::Param { name: "EdmCondition".into(), value: json!("C110") },
        Record::Param { name: "WireSide".into(), value: json!("punch") },
    ];
    recs.extend(params.iter().map(|(n, v)| Record::Param { name: (*n).into(), value: v.clone() }));
    recs.push(Record::Section { begin: true });
    recs.push(mv(MoveKind::Rapid, -30.0, 0.0));
    recs.push(Record::Param { name: "WireAngle".into(), value: json!(3.0) });
    recs.push(mv(MoveKind::Feed, -20.0, 0.0));
    recs.push(Record::Section { begin: false });
    cnc::serialize(&recs)
}

fn warnings(m: &Value, params: &[(&str, Value)]) -> Vec<String> {
    engine::run(&read("assets/posts/wire-iso.cps"), m, &stream(params)).expect("the wire post runs").warnings
}

fn says(w: &[String], what: &str) -> bool {
    w.iter().any(|l| l.contains(what))
}

fn angle(extra: &[(&'static str, Value)]) -> Vec<(&'static str, Value)> {
    let mut p = vec![("WireTaperDialect", json!("angle")), ("WireCorner", json!("mitre")), ("WirePlane", json!(12.0))];
    p.extend(extra.iter().cloned());
    p
}

#[test]
fn an_angle_word_section_posts_and_each_missing_piece_is_refused_by_name() {
    let good = machine(Some("FX-ANGLE A{angle}"));
    let w = warnings(&good, &angle(&[("WireTaper", json!(3.0))]));
    assert!(w.is_empty(), "the control section posts: {w:?}");
    let one = "an angle-word taper needs one stated angle";
    assert!(says(&warnings(&good, &angle(&[])), one), "a zero taper");
    assert!(says(&warnings(&good, &angle(&[("WireTaper", json!(3.0)), ("WireFourAxis", json!("arc length"))])), one), "a 4-axis cut");
    let mut no_code = machine(Some("FX-ANGLE A{angle}"));
    no_code["edm"]["codes"].as_object_mut().expect("codes").remove("taper_angle");
    let w = warnings(&no_code, &angle(&[("WireTaper", json!(3.0))]));
    assert!(says(&w, "is tapered by angle word, and machine") && says(&w, "states no edm.codes.taper_angle"), "{w:?}");
    let w = warnings(&machine(Some("FX-ANGLE A")), &angle(&[("WireTaper", json!(3.0))]));
    assert!(says(&w, "edm.codes.taper_angle states no {angle} place"), "{w:?}");
    let w = warnings(&machine(None), &[("WireTaper", json!(3.0))]);
    assert!(says(&w, "an angle word in a section that states no angle-word taper"), "{w:?}");
}
