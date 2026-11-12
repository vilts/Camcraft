//! Issue #51: pin the TNC DIN/ISO cycle and preset interface against real
//! engine calls, including the expanded chains each cycle replaces or keeps.
//!
//! Source limits: TNC 640 Cycle Programming manual 892905-25 §§2.1, 3.3,
//! 3.8, 4.2, 4.3 and 10.4. These are regression assertions for our mapping,
//! not controller execution or ISO syntax verification.
//!
//! Gate map (#922): covers post::post (engine, cnc, probe_rec), post::machine,
//! app::posting.

use camcraft_app::cl::{CompSide, CycleKind, MoveKind};
use camcraft_app::post::{cnc::{self, CycleRec, MoveRec, Record}, engine, probe_rec::TouchRec};
use serde_json::{json, Value};
use std::path::Path;

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn machine(rigid: Option<bool>) -> Value {
    let mut m: Value = serde_json::from_str(&read("assets/machines/3axis-vmc.mch")).expect("machine json");
    if let Some(v) = rigid {
        m["kinematics"]["static"]["head"]["rigidTapping"] = json!(v);
    }
    m
}

fn post(stream: &str) -> engine::PostOutput {
    engine::run(&read("assets/posts/heidenhain.cps"), &machine(None), stream)
        .unwrap_or_else(|e| panic!("Heidenhain engine run: {e}"))
}

fn tap_output(kind: CycleKind, feed: f64, dwell: f64) -> engine::PostOutput {
    let mut rs = setup(3000.0);
    let mut n = 0;
    add_hole(&mut rs, &mut n, hole(kind, 10.0, 2.0, -6.0, 0.0, dwell, Some(feed)), None);
    finish(&mut rs);
    let serialized = stream(&rs);
    let parsed = cnc::parse(&serialized).expect("cycle fixture parses");
    let cycle = parsed.iter().find_map(|r| match r { Record::Cycle(c) => Some(c), _ => None }).unwrap();
    assert!((cycle.feed.unwrap() - feed).abs() < 0.000001, "fixture feed survives the public stream serializer");
    assert!((cycle.dwell - dwell).abs() < 0.000001, "fixture dwell survives millisecond serialization");
    post(&serialized)
}

fn param(name: &str, value: Value) -> Record {
    Record::Param { name: name.into(), value }
}

fn setup(rpm: f64) -> Vec<Record> {
    vec![
        param("OperationName", json!("Cycle regression")),
        param("SpindleSpeed", json!(rpm)),
        Record::Tool { number: 4, name: "TAP-M4".into() },
        Record::Section { begin: true },
    ]
}

fn mv(kind: MoveKind, x: f64, y: f64, z: f64, feed: Option<f64>) -> Record {
    Record::Move(MoveRec { kind, x, y, z, feed, center: None, ccw: false, plane: Default::default(), abc: None })
}

#[derive(Clone, Copy)]
struct Hole {
    kind: CycleKind,
    x: f64,
    y: f64,
    r: f64,
    entry: f64,
    bottom: f64,
    peck: f64,
    dwell: f64,
    feed: Option<f64>,
}

fn hole(kind: CycleKind, x: f64, r: f64, bottom: f64, peck: f64, dwell: f64, feed: Option<f64>) -> Hole {
    Hole { kind, x, y: 20.0, r, entry: 0.0, bottom, peck, dwell, feed }
}

/// Append a CYCLE and the generator-shaped approach/cutting/retract chain.
/// `chain_bottom` lets one test make the native ChainBottomZ guard disagree.
fn add_hole(rs: &mut Vec<Record>, move_no: &mut usize, h: Hole, chain_bottom: Option<f64>) {
    add_hole_with_chain_feed(rs, move_no, h, chain_bottom, None);
}

fn add_hole_with_chain_feed(
    rs: &mut Vec<Record>, move_no: &mut usize, h: Hole,
    chain_bottom: Option<f64>, chain_feed: Option<f64>,
) {
    rs.push(Record::Cycle(CycleRec {
        kind: h.kind, x: h.x, y: h.y, r_z: h.r, bottom_z: h.bottom,
        entry_z: h.entry, peck: h.peck, dwell: h.dwell,
        first_move: *move_no, feed: h.feed,
    }));
    let bottom = chain_bottom.unwrap_or(h.bottom);
    let cut_feed = chain_feed.or(h.feed);
    let mut chain = vec![mv(MoveKind::Rapid, h.x, h.y, h.r, None)];
    match h.kind {
        CycleKind::Tap | CycleKind::FloatingTap => {
            chain.push(mv(MoveKind::Feed, h.x, h.y, bottom, cut_feed));
            chain.push(mv(MoveKind::Feed, h.x, h.y, h.r, cut_feed));
        }
        CycleKind::Peck | CycleKind::Deep => {
            let mut z = (h.r - h.peck).max(bottom);
            loop {
                chain.push(mv(MoveKind::Plunge, h.x, h.y, z, cut_feed));
                if z <= bottom { break; }
                let up = if h.kind == CycleKind::Peck { (z + 1.0).min(h.r) } else { h.r };
                chain.push(mv(MoveKind::Rapid, h.x, h.y, up, None));
                z = (z - h.peck).max(bottom);
            }
            chain.push(mv(MoveKind::Rapid, h.x, h.y, h.r, None));
        }
        CycleKind::Bore => {
            chain.push(mv(MoveKind::Feed, h.x, h.y, bottom, cut_feed));
            chain.push(mv(MoveKind::Rapid, h.x, h.y, h.r, None));
        }
        CycleKind::BackBore => {
            chain.push(mv(MoveKind::Feed, h.x, h.y, bottom, cut_feed));
            chain.push(mv(MoveKind::Feed, h.x, h.y, h.r, cut_feed));
        }
        _ => {
            chain.push(mv(MoveKind::Plunge, h.x, h.y, bottom, cut_feed));
            chain.push(mv(MoveKind::Rapid, h.x, h.y, h.r, None));
        }
    }
    chain.push(mv(MoveKind::Rapid, h.x, h.y, 10.0, None)); // explicit safe hop
    *move_no += chain.len();
    rs.extend(chain);
}

fn finish(rs: &mut Vec<Record>) { rs.push(Record::Section { begin: false }); }
fn stream(rs: &[Record]) -> String { cnc::serialize(rs) }

fn bodies(program: &str) -> Vec<String> {
    program.lines().filter_map(|line| {
        let body = line.strip_prefix('N')?.split_once(' ')?.1;
        if body.starts_with('%') { return None; }
        Some(body.strip_suffix('*')?.to_string())
    }).collect()
}

fn definition<'a>(bs: &'a [String], g: &str) -> &'a str {
    bs.iter().find(|b| b.starts_with(g)).map(String::as_str)
        .unwrap_or_else(|| panic!("no {g} definition in {bs:?}"))
}

fn numeric_qs(body: &str) -> Vec<(u16, String)> {
    body.split_whitespace().filter_map(|w| {
        let n = w.strip_prefix('Q')?;
        let (n, v) = n.split_once('=')?;
        if v == "FMAX" { return None; }
        Some((n.parse().expect("numeric Q address"), v.to_string()))
    }).collect()
}

fn assert_qs(body: &str, expected: &[(u16, &str)]) {
    let want: Vec<_> = expected.iter().map(|(n, v)| (*n, (*v).to_string())).collect();
    assert_eq!(numeric_qs(body), want, "Q inventory in {body}");
}

fn calls(bs: &[String]) -> usize {
    bs.iter().flat_map(|b| b.split_whitespace()).filter(|w| *w == "M99").count()
}

#[test]
fn g200_uses_surface_relative_q_values_and_keeps_the_explicit_hop() {
    let mut rs = vec![
        param("OperationName", json!("Cycle regression")),
        param("SpindleSpeed", json!(3000.0)),
        param("OriginX", json!(100.0)), param("OriginY", json!(200.0)),
        param("OriginZ", json!(50.0)), param("WorkOffset", json!("G55")),
        Record::Tool { number: 4, name: "TAP-M4".into() },
        Record::Section { begin: true },
    ];
    let mut n = 0;
    add_hole(&mut rs, &mut n, Hole { kind: CycleKind::Drill, x: 110.0, y: 220.0, r: 7.0, entry: 5.0, bottom: -8.0, peck: 0.0, dwell: 0.25, feed: Some(120.0) }, None);
    finish(&mut rs);
    let out = post(&stream(&rs));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let bs = bodies(&out.program);
    let g = definition(&bs, "G200");
    assert_qs(g, &[(200,"+2"),(201,"-13"),(206,"+120"),(202,"+13"),(210,"+0"),(203,"-45"),(204,"+2"),(211,"+0.25"),(395,"+0")]);
    assert!(bs.iter().any(|b| b == "G247 Q339=2"), "G55 is house-mapped to preset row 2: {bs:?}");
    assert!(bs.iter().any(|b| b.contains("G40 G90 X+10 Y+20 Z-43 M99")), "absolute positioning shifts by origin: {bs:?}");
    assert!(bs.iter().any(|b| b == "G00 X+10 Y+20 Z-40"), "the explicit high rapid hop remains: {bs:?}");
    assert_eq!(calls(&bs), 1);
    assert!(!out.program.contains("G80") && !out.program.contains("M89"));
    assert!(!out.program.contains("G01") && !out.program.contains("FUNCTION DWELL"), "native cycle replaces chain cuts/dwell");
}

#[test]
fn peck_and_deep_have_distinct_documented_native_cycles_and_q_inventory() {
    let mut rs = setup(3000.0);
    let mut n = 0;
    add_hole(&mut rs, &mut n, hole(CycleKind::Peck, 10.0, 0.0, -8.0, 4.0, 0.2, Some(150.0)), None);
    add_hole(&mut rs, &mut n, hole(CycleKind::Deep, 30.0, 0.0, -8.0, 4.0, 0.0, Some(90.0)), None);
    finish(&mut rs);
    let out = post(&stream(&rs));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let bs = bodies(&out.program);
    assert_qs(definition(&bs, "G205"), &[(200,"+0"),(201,"-8"),(206,"+150"),(202,"+4"),(203,"+0"),(204,"+0"),(212,"+0"),(205,"+4"),(258,"+0"),(259,"+0"),(257,"+4"),(256,"+1"),(211,"+0.2"),(379,"+0"),(253,"+150"),(395,"+0")]);
    assert!(definition(&bs, "G205").contains("Q208=FMAX"));
    assert_qs(definition(&bs, "G200"), &[(200,"+0"),(201,"-8"),(206,"+90"),(202,"+4"),(210,"+0"),(203,"+0"),(204,"+0"),(211,"+0"),(395,"+0")]);
    assert_eq!(calls(&bs), 2);
}

#[test]
fn cycle_signatures_redefine_on_feed_dwell_surface_and_tapping_mode_changes() {
    let mut rs = setup(3000.0);
    let mut n = 0;
    let make = |x, kind, r, bottom, dwell, feed| hole(kind, x, r, bottom, 0.0, dwell, Some(feed));
    for h in [
        make(10.0, CycleKind::Drill, 2.0, -6.0, 0.25, 120.0),
        make(20.0, CycleKind::Drill, 2.0, -6.0, 0.25, 120.0), // same signature
        make(30.0, CycleKind::Drill, 2.0, -6.0, 0.25, 150.0),
        make(40.0, CycleKind::Drill, 2.0, -6.0, 0.5, 150.0),
        Hole { kind: CycleKind::Drill, x: 50.0, y: 20.0, r: 3.0, entry: 1.0, bottom: -7.0, peck: 0.0, dwell: 0.5, feed: Some(150.0) },
        make(60.0, CycleKind::Tap, 2.0, -6.0, 0.0, 300.0),
        make(70.0, CycleKind::FloatingTap, 2.0, -6.0, 0.5, 300.0),
    ] { add_hole(&mut rs, &mut n, h, None); }
    finish(&mut rs);
    let out = post(&stream(&rs));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let bs = bodies(&out.program);
    assert_eq!(bs.iter().filter(|b| b.starts_with("G200 ")).count(), 4);
    assert_eq!(bs.iter().filter(|b| b.starts_with("G207 ")).count(), 1);
    assert_eq!(bs.iter().filter(|b| b.starts_with("G206 ")).count(), 1);
    assert_eq!(calls(&bs), 7, "one M99 per hole even when a definition is reused");
    assert!(!out.program.contains("G01") && !out.program.contains("FUNCTION DWELL"));
    assert!(!out.program.contains("G80") && !out.program.contains("M89"));
}

#[test]
fn rigid_and_floating_taps_keep_their_separate_pitch_contracts_and_refusals() {
    let mut rs = setup(3000.0);
    let mut n = 0;
    add_hole(&mut rs, &mut n, hole(CycleKind::Tap, 10.0, 2.0, -6.0, 0.0, 0.0, Some(300.0)), None);
    add_hole(&mut rs, &mut n, hole(CycleKind::FloatingTap, 20.0, 2.0, -6.0, 0.0, 0.25, Some(300.0)), None);
    finish(&mut rs);
    let out = engine::run(&read("assets/posts/heidenhain.cps"), &machine(None), &stream(&rs)).unwrap();
    assert!(out.warnings.is_empty(), "valid rigid and floating intent: {:?}", out.warnings);
    let bs = bodies(&out.program);
    assert_qs(definition(&bs, "G207"), &[(200,"+2"),(201,"-6"),(239,"+0.1"),(203,"+0"),(204,"+2")]);
    assert_qs(definition(&bs, "G206"), &[(200,"+2"),(201,"-6"),(206,"+300"),(211,"+0.25"),(203,"+0"),(204,"+2")]);
    assert_eq!(calls(&bs), 2);

    let mut rigid = setup(3000.0);
    let mut m = 0;
    add_hole(&mut rigid, &mut m, hole(CycleKind::Tap, 10.0, 2.0, -6.0, 0.0, 0.0, Some(300.0)), None);
    finish(&mut rigid);
    let rigid_machine = machine(Some(false));
    assert_eq!(
        camcraft_app::post::machine::script_props(&rigid_machine)["rigidTapping"],
        json!(false),
        "the fixture must set the capability key mapped into the engine's machine global",
    );
    let out = engine::run(&read("assets/posts/heidenhain.cps"), &rigid_machine, &stream(&rigid)).unwrap();
    assert!(out.warnings.iter().any(|w| w.contains("rigid tapping is unavailable")), "{:?}", out.warnings);
    assert!(!out.program.contains("G206") && !out.program.contains("G207"), "no floating fallback or unsupported rigid cycle");
    assert!(!out.program.contains("G01"), "a refused tap does not fall through to feed moves");

    let mut floating = setup(3000.0);
    let mut f = 0;
    add_hole(&mut floating, &mut f, hole(CycleKind::FloatingTap, 10.0, 2.0, -6.0, 0.0, 0.0, Some(300.0)), None);
    finish(&mut floating);
    let out = engine::run(&read("assets/posts/heidenhain.cps"), &machine(Some(false)), &stream(&floating)).unwrap();
    assert!(out.warnings.is_empty(), "explicit floating holder does not depend on rigid-tapping capability: {:?}", out.warnings);
    assert!(out.program.contains("G206"), "explicit floating intent maps to G206");
    assert!(!out.program.contains("G207"));
}

#[test]
fn floating_tap_uses_q206_and_enforces_its_feed_and_dwell_resolution() {
    let floating = tap_output(CycleKind::FloatingTap, 370.368, 0.25);
    assert!(floating.warnings.is_empty(), "{:?}", floating.warnings);
    let bs = bodies(&floating.program);
    let g206 = definition(&bs, "G206");
    assert_qs(g206, &[(200, "+2"), (201, "-6"), (206, "+370.368"), (211, "+0.25"), (203, "+0"), (204, "+2")]);
    assert!(!g206.contains("Q239="), "floating pitch is derived from feed and rpm, not Q239");

    let rigid = tap_output(CycleKind::Tap, 370.368, 0.0);
    assert!(rigid.warnings.iter().any(|w| w.contains("cannot be represented at the control's pitch resolution")), "{:?}", rigid.warnings);
    assert!(!rigid.program.contains("G206") && !rigid.program.contains("G207"));

    let dwell_at_limit = tap_output(CycleKind::FloatingTap, 370.368, 0.5);
    assert!(dwell_at_limit.warnings.is_empty(), "0.5 seconds is accepted: {:?}", dwell_at_limit.warnings);
    assert!(definition(&bodies(&dwell_at_limit.program), "G206").contains("Q211=+0.5"));

    let dwell_over_limit = tap_output(CycleKind::FloatingTap, 370.368, 0.501);
    assert!(dwell_over_limit.warnings.iter().any(|w| w.contains("must be at most 0.5 seconds")), "{:?}", dwell_over_limit.warnings);
    assert!(!dwell_over_limit.program.contains("G206") && !dwell_over_limit.program.contains("G207"));

    // Only a hand-authored stream can exceed the serializer's 0.001 feed
    // resolution. Establish that parsed premise before testing the refusal.
    let mut rs = setup(3000.0);
    let mut n = 0;
    add_hole(&mut rs, &mut n, hole(CycleKind::FloatingTap, 10.0, 2.0, -6.0, 0.0, 0.25, Some(370.368)), None);
    finish(&mut rs);
    let raw = stream(&rs).replacen("F 370368", "F 370368.1", 1);
    let parsed = cnc::parse(&raw).expect("precise hand-authored stream parses");
    let feed = parsed.iter().find_map(|r| match r { Record::Cycle(c) => c.feed, _ => None }).unwrap();
    assert!((feed - 370.3681).abs() < 0.0000001, "the raw stream keeps the distinguishing feed");
    let feed_over_resolution = post(&raw);
    assert!(feed_over_resolution.warnings.iter().any(|w| w.contains("floating tapping feed cannot be represented at the control's feed resolution")), "{:?}", feed_over_resolution.warnings);
    assert!(!feed_over_resolution.program.contains("G206") && !feed_over_resolution.program.contains("G207"));
}

#[test]
fn invalid_tapping_inputs_and_legacy_tap_without_spindle_are_named_refusals() {
    let legacy = post(&read("crates/app/tests/golden/tap_plate.cl"));
    assert!(legacy.warnings.iter().any(|w| w.contains("tapping needs a positive integral spindle speed")), "{:?}", legacy.warnings);
    assert!(!legacy.program.contains("G206") && !legacy.program.contains("G207"));

    for (rpm, feed, dwell, reason) in [
        (3000.0, -300.0, 0.0, "positive cutting feed"),
        (-3000.0, 300.0, 0.0, "positive integral spindle speed"),
        (3000.0, 300.0, 0.5, "G207 rigid tapping has no dwell"),
    ] {
        let mut rs = setup(rpm);
        let mut n = 0;
        add_hole(&mut rs, &mut n, hole(CycleKind::Tap, 10.0, 2.0, -6.0, 0.0, dwell, Some(feed)), None);
        finish(&mut rs);
        let out = post(&stream(&rs));
        assert!(out.warnings.iter().any(|w| w.contains(reason)), "{reason}: {:?}", out.warnings);
        assert!(!out.program.contains("G206") && !out.program.contains("G207"));
    }
}

#[test]
fn legacy_missing_feed_uses_chain_feed_and_chain_bottom_mismatch_is_refused() {
    let mut rs = setup(3000.0);
    let mut n = 0;
    add_hole_with_chain_feed(&mut rs, &mut n, hole(CycleKind::Drill, 10.0, 0.0, -6.0, 0.0, 0.0, None), Some(-6.0), Some(120.0));
    finish(&mut rs);
    let out = post(&stream(&rs));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(definition(&bodies(&out.program), "G200").contains("Q206=+120"), "first expanded cut supplies the legacy feed");

    let mut bad = setup(3000.0);
    let mut m = 0;
    add_hole(&mut bad, &mut m, hole(CycleKind::Drill, 10.0, 0.0, -6.0, 0.0, 0.0, Some(120.0)), Some(-5.0));
    finish(&mut bad);
    let out = post(&stream(&bad));
    assert!(out.warnings.iter().any(|w| w.contains("disagrees with the expanded chain bottom")), "{:?}", out.warnings);
}

#[test]
fn nonzero_entry_peck_and_deep_and_all_borings_keep_the_expanded_chain() {
    let mut rs = setup(3000.0);
    let mut n = 0;
    add_hole(&mut rs, &mut n, hole(CycleKind::Peck, 10.0, 2.0, -8.0, 5.0, 0.0, Some(100.0)), None);
    add_hole(&mut rs, &mut n, hole(CycleKind::Deep, 30.0, 2.0, -8.0, 5.0, 0.0, Some(100.0)), None);
    add_hole(&mut rs, &mut n, hole(CycleKind::Bore, 50.0, 2.0, -8.0, 0.0, 0.25, Some(100.0)), None);
    add_hole(&mut rs, &mut n, hole(CycleKind::BackBore, 70.0, 2.0, -8.0, 0.0, 0.25, Some(100.0)), None);
    finish(&mut rs);
    let out = post(&stream(&rs));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(!out.program.contains("G200") && !out.program.contains("G205") && !out.program.contains("M99"));
    let bs = bodies(&out.program);
    let cuts: Vec<_> = bs.iter().filter(|b| b.starts_with("G01 ")).collect();
    assert_eq!(cuts.len(), 7, "both nonzero-R peck chains and both boring chains stay explicit: {cuts:?}");
    assert!(cuts.iter().any(|b| b.contains("Z-3 F100")) && cuts.iter().any(|b| b.contains("Z-8 F100")), "the from-R infeed chain is preserved: {cuts:?}");
    assert_eq!(bs.iter().filter(|b| b.starts_with("FUNCTION DWELL TIME")).count(), 2);
}

#[test]
fn preset_rows_transitions_and_missing_work_offset_keep_the_active_preset() {
    let rows: Vec<_> = (54..=59).map(|n| Some(format!("G{n}"))).collect();
    let mut rs = Vec::new();
    for row in &rows {
        if let Some(word) = row { rs.push(param("WorkOffset", json!(word))); }
        rs.push(Record::Section { begin: true });
        rs.push(mv(MoveKind::Rapid, 0.0, 0.0, 5.0, None));
        rs.push(Record::Section { begin: false });
    }
    let out = post(&stream(&rs));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let qs: Vec<_> = bodies(&out.program).iter().filter_map(|b| b.strip_prefix("G247 Q339=").map(str::to_string)).collect();
    assert_eq!(qs, ["1", "2", "3", "4", "5", "6"]);

    let mut rs = Vec::new();
    for offset in [Some("G55"), Some("G54"), Some("G54")] {
        if let Some(word) = offset { rs.push(param("WorkOffset", json!(word))); }
        rs.push(Record::Section { begin: true });
        rs.push(mv(MoveKind::Rapid, 0.0, 0.0, 5.0, None));
        rs.push(Record::Section { begin: false });
    }
    let out = post(&stream(&rs));
    let qs: Vec<_> = bodies(&out.program).iter().filter_map(|b| b.strip_prefix("G247 Q339=").map(str::to_string)).collect();
    assert_eq!(qs, ["2", "1"], "G55→G54 changes row; repeating G54 is modal");

    let mut rs = Vec::new();
    for offset in [Some("G55"), None] {
        if let Some(word) = offset { rs.push(param("WorkOffset", json!(word))); }
        rs.push(Record::Section { begin: true });
        rs.push(mv(MoveKind::Rapid, 0.0, 0.0, 5.0, None));
        rs.push(Record::Section { begin: false });
    }
    let out = post(&stream(&rs));
    let qs: Vec<_> = bodies(&out.program).iter().filter_map(|b| b.strip_prefix("G247 Q339=").map(str::to_string)).collect();
    assert_eq!(qs, ["2"], "omitting WorkOffset leaves the active preset alone");
}

#[test]
fn floating_tapping_unsupported_by_another_post_is_still_a_counted_refusal() {
    let mut rs = setup(3000.0);
    let mut n = 0;
    add_hole(&mut rs, &mut n, hole(CycleKind::FloatingTap, 10.0, 2.0, -6.0, 0.0, 0.0, Some(300.0)), None);
    finish(&mut rs);
    let fanuc = read("assets/posts/fanuc.cps");
    let out = engine::run(&fanuc, &machine(None), &stream(&rs)).expect("engine runs other post");
    assert!(out.warnings.iter().any(|w| w.contains("floating-holder tapping is not supported by this post")), "unsupported mode stays visible: {:?}", out.warnings);
}

#[test]
fn new_cycle_interface_does_not_weaken_probe_tilt_or_compensation_refusals() {
    let mut rs = setup(3000.0);
    rs.push(Record::Move(MoveRec { kind: MoveKind::Feed, x: 1.0, y: 0.0, z: -1.0, feed: Some(100.0), center: None, ccw: false, plane: Default::default(), abc: Some([10.0, 0.0, 0.0]) }));
    rs.push(Record::Comp { side: Some(CompSide::Left), radius_mm: 1.0, wear: false });
    rs.push(Record::Touch(TouchRec { target: [1.0, 0.0, -2.0], expect: [1.0, 0.0, -1.0], feed: 10.0, measure: Some(1) }));
    finish(&mut rs);
    let out = post(&stream(&rs));
    assert!(out.warnings.iter().any(|w| w.contains("tilted move - Heidenhain post has no RTCP output")), "tilt remains refused: {:?}", out.warnings);
    assert!(out.warnings.iter().any(|w| w.contains("control cutter compensation (G41/G42) is not supported")), "comp remains refused: {:?}", out.warnings);
    assert!(out.warnings.iter().any(|w| w.contains("does not define onProbeMove")), "probe move remains refused: {:?}", out.warnings);
}
