//! #2583: the `.mch` part tree read whole (`MachineTree`) - the parent
//! chain, the moving side of each axis, the unit scaling, and the
//! refusals that name the part id and the key. Pinned on the shipped
//! machine files and on hand-written trees.
//! Provenance: our own test over our own machine-file grammar
//! (`assets/machines/AGENTS.md`).
//!
//! Gate map (M4.4): covers camcraft_post::machine (tree).

use camcraft_post::machine::{parse_mch, parse_mch_tree, AxisDirection, MachineTree, MovingSide, PartKind};

fn machines_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines")
}

fn shipped(name: &str) -> MachineTree {
    let text = std::fs::read_to_string(machines_dir().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
    parse_mch_tree(&text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The ids from the root down to `id`, the part itself last.
fn chain(t: &MachineTree, id: &str) -> Vec<String> {
    let i = t.index_of(id).unwrap_or_else(|| panic!("no part {id}"));
    t.chain(i).iter().map(|p| p.id.clone()).collect()
}

fn side(t: &MachineTree, id: &str) -> Option<MovingSide> {
    t.part(id).unwrap_or_else(|| panic!("no part {id}")).moving_side
}

fn tree(units: &str, parts: &str) -> String {
    format!(r#"{{ "kinematics": {{ "default": {{ "units": {{ "length": "{units}" }}, "parts": {parts} }} }} }}"#)
}

fn refusal(parts: &str) -> String {
    parse_mch_tree(&tree("mm", parts)).expect_err("the tree must refuse")
}

#[test]
fn every_shipped_machine_file_reads_as_a_tree() {
    let mut read = 0;
    for entry in std::fs::read_dir(machines_dir()).expect("assets/machines") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("mch") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("machine text");
        let t = parse_mch_tree(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let m = parse_mch(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        // The tree and the flat list state the same driven axes.
        let axes = t.parts.iter().filter(|p| matches!(p.kind, PartKind::Linear | PartKind::Rotary)).count();
        assert_eq!(axes, m.axes.len(), "{}", path.display());
        // A parent is always listed ahead of its child.
        for (i, p) in t.parts.iter().enumerate() {
            assert!(p.parent.is_none_or(|parent| parent < i), "{}: {}", path.display(), p.id);
        }
        read += 1;
    }
    assert!(read >= 40, "only {read} machine files found");
}

#[test]
fn the_mirror_form_default_machine_states_no_part_tree() {
    // `3axis-vmc.mch` is written in the mirror form (`kinematics.static`):
    // it states no `kinematics.default.parts`, so the tree is empty.
    let t = shipped("3axis-vmc.mch");
    assert!(t.parts.is_empty());
    assert!(t.head().is_none());
}

#[test]
fn the_three_axis_vmc_keeps_its_parent_chain() {
    let t = shipped("vmc.mch");
    assert_eq!(chain(&t, "sld"), ["base", "col", "ram", "sld"]);
    assert_eq!(chain(&t, "head"), ["base", "head"]);
    let x = t.part("sld").expect("sld");
    assert_eq!((x.name.as_str(), x.kind), ("X", PartKind::Linear));
    assert_eq!(x.travel, Some((-600.0, 600.0)));
    assert_eq!(x.direction, AxisDirection::Standard);
    let base = t.index_of("base").expect("base");
    assert_eq!(t.parts[base].kind, PartKind::Table);
    assert_eq!(t.parts[base].parent, None);
    let kids: Vec<&str> = t.children(base).map(|p| p.id.as_str()).collect();
    assert_eq!(kids, ["col", "head"]);
    // The head hangs beside the axes: no axis is on the path to it.
    for id in ["col", "ram", "sld"] {
        assert_eq!(side(&t, id), Some(MovingSide::Table), "{id}");
    }
    assert_eq!(side(&t, "base"), None);
    assert_eq!(side(&t, "head"), None);
    let head = t.head().expect("head");
    let body = head.spindle_body.expect("spindle body");
    assert_eq!((body.diameter, body.length), (Some(80.0), Some(60.0)));
    let station = head.tool_station.expect("tool station");
    assert_eq!((station.max_tool_length, station.max_tool_diameter), (Some(300.0), Some(100.0)));
}

#[test]
fn the_tilt_head_puts_the_head_chain_on_the_tool_side() {
    let t = shipped("tilthead.mch");
    assert_eq!(chain(&t, "hed"), ["base", "col", "tla", "tlb", "hed"]);
    assert_eq!(chain(&t, "sld"), ["base", "crl", "sld"]);
    for id in ["col", "tla", "tlb"] {
        assert_eq!(side(&t, id), Some(MovingSide::Tool), "{id}");
    }
    for id in ["crl", "sld"] {
        assert_eq!(side(&t, id), Some(MovingSide::Table), "{id}");
    }
    let a = t.part("tla").expect("tla");
    assert_eq!((a.name.as_str(), a.kind), ("A", PartKind::Rotary));
    assert_eq!(a.axis, Some([1.0, 0.0, 0.0]));
    assert_eq!(a.travel, Some((-120.0, 120.0)));
    assert_eq!(t.head().map(|h| h.id.as_str()), Some("hed"));
}

#[test]
fn an_inch_file_scales_lengths_and_keeps_angles() {
    let t = shipped("tormach-770mx-microarc4.mch");
    assert_eq!(chain(&t, "sld"), ["base", "col", "ram", "sld"]);
    assert_eq!(t.part("sld").expect("sld").travel, Some((-14.0 * 25.4, 14.0 * 25.4)));
    assert_eq!(t.part("col").expect("col").travel, Some((-13.25 * 25.4, 13.25 * 25.4)));
    let a = t.part("arc4").expect("arc4");
    assert_eq!(a.travel, None, "an unstated travel stays unstated");
    assert_eq!(a.moving_side, Some(MovingSide::Table));
    // The stated numbers of the tree and of the flat reader agree.
    let text = std::fs::read_to_string(machines_dir().join("tormach-770mx-microarc4.mch")).expect("text");
    let m = parse_mch(&text).expect("machine");
    for axis in &m.axes {
        let part = t.parts.iter().find(|p| p.name == axis.name).expect("axis part");
        assert_eq!(part.travel, axis.travel, "{}", axis.name);
    }
}

#[test]
fn a_hand_written_inch_tree_scales_each_length_and_no_angle() {
    let parts = r#"[{ "id": "base", "type": "table", "parts": [
        { "id": "z", "name": "Z", "type": "linear", "travel": { "min": -1.0, "max": 2.0 }, "direction": -1, "parts": [
          { "id": "b", "name": "B", "type": "rotary", "axis": [0, 1, 0], "travel": { "min": -90, "max": 90 },
            "pivot": [1.0, 0.0, 2.0], "parts": [
            { "id": "c", "name": "C", "type": "rotary", "axis": [0, 0, 1], "offset": [0.0, 0.5, 0.0], "parts": [
              { "id": "h", "type": "head", "spindle": { "body_diameter": 4.0, "body_length": 0.0 },
                "tool_station": { "max_tool_length": 10.0 } } ] } ] } ] } ] }]"#;
    let t = parse_mch_tree(&tree("inch", parts)).expect("tree");
    let z = t.part("z").expect("z");
    assert_eq!(z.travel, Some((-25.4, 50.8)));
    assert_eq!(z.direction, AxisDirection::Inverted);
    let b = t.part("b").expect("b");
    assert_eq!(b.travel, Some((-90.0, 90.0)), "an angle is never unit-scaled");
    assert_eq!(b.pivot, Some([25.4, 0.0, 50.8]));
    assert_eq!(t.part("c").expect("c").offset, Some([0.0, 12.7, 0.0]));
    let h = t.head().expect("head");
    assert_eq!(h.name, "h", "an unnamed part takes its id");
    let body = h.spindle_body.expect("body");
    assert_eq!((body.diameter, body.length), (Some(4.0 * 25.4), None), "a zero length is unstated");
    let station = h.tool_station.expect("station");
    assert_eq!((station.max_tool_length, station.max_tool_diameter), (Some(254.0), None));
    for id in ["z", "b", "c"] {
        assert_eq!(side(&t, id), Some(MovingSide::Tool), "{id}");
    }
}

#[test]
fn a_tree_with_no_head_states_no_moving_side() {
    let t = parse_mch_tree(&tree("mm", r#"[{ "id": "x", "name": "X", "type": "linear" }]"#)).expect("tree");
    assert_eq!(side(&t, "x"), None);
    assert!(t.head().is_none());
}

#[test]
fn a_malformed_tree_refuses_by_part_id_and_key() {
    let cases = [
        (r#"{ "id": "base" }"#, "kinematics.default.parts"),
        (r#"[{ "id": "base", "type": "gantry" }]"#, "part base: type"),
        (r#"[{ "id": "base" }]"#, "part base: type"),
        (r#"[{ "name": "table", "type": "table" }]"#, "part table: id"),
        (r#"[{ "id": 7, "type": "table" }]"#, "id"),
        (r#"[{ "id": "base", "type": "table", "parts": {} }]"#, "part base: parts"),
        (r#"[{ "id": "base", "type": "table", "parts": [3] }]"#, "part base: parts[0]"),
        (
            r#"[{ "id": "a", "type": "table", "parts": [{ "id": "a", "type": "head" }] }]"#,
            "part a: id",
        ),
        (r#"[{ "id": "z", "type": "linear", "direction": 2 }]"#, "part z: direction"),
        (r#"[{ "id": "z", "type": "linear", "name": 4 }]"#, "part z: name"),
        (r#"[{ "id": "a", "type": "rotary", "axis": [1, 0] }]"#, "part a: axis"),
        (r#"[{ "id": "a", "type": "rotary", "axis": [0, 0, 0] }]"#, "part a: axis"),
        (r#"[{ "id": "a", "type": "rotary", "direction": -1 }]"#, "part a: direction"),
        (r#"[{ "id": "a", "type": "rotary", "pivot": "here" }]"#, "part a pivot"),
        (r#"[{ "id": "z", "type": "linear", "offset": [1, 2, 3] }]"#, "part z offset"),
        (r#"[{ "id": "h", "type": "head", "spindle": 5 }]"#, "part h: spindle"),
        (r#"[{ "id": "h", "type": "head", "tool_station": [] }]"#, "part h: tool_station"),
    ];
    for (parts, want) in cases {
        let e = refusal(parts);
        assert!(e.starts_with("machine file: "), "{parts}: {e}");
        assert!(e.contains(want), "{parts}: {e:?} does not name {want:?}");
    }
    assert!(parse_mch_tree("not json").is_err_and(|e| e.starts_with("machine file: ")));
    assert!(parse_mch_tree("[]").is_err_and(|e| e.contains("expected a machine object")));
}

fn head_length_overflow_refuses(object: &str, key: &str) {
    let parts = format!(r#"[{{ "id": "h", "type": "head", "{object}": {{ "{key}": 1e308 }} }}]"#);
    let error = parse_mch_tree(&tree("inch", &parts)).expect_err("converted length must be finite");
    assert!(error.starts_with("machine file: part h:"), "{error}");
    assert!(error.contains(key), "{error} does not name {key}");
    assert!(error.contains("finite"), "{error}");
}

#[test]
fn inch_body_diameter_overflow_refuses() {
    head_length_overflow_refuses("spindle", "body_diameter");
}

#[test]
fn inch_body_length_overflow_refuses() {
    head_length_overflow_refuses("spindle", "body_length");
}

#[test]
fn inch_max_tool_length_overflow_refuses() {
    head_length_overflow_refuses("tool_station", "max_tool_length");
}

#[test]
fn inch_max_tool_diameter_overflow_refuses() {
    head_length_overflow_refuses("tool_station", "max_tool_diameter");
}

#[test]
fn absent_and_nonpositive_head_lengths_stay_unstated() {
    for fields in [
        "",
        r#""body_diameter": null, "body_length": null, "max_tool_length": null, "max_tool_diameter": null"#,
        r#""body_diameter": 0, "body_length": 0, "max_tool_length": 0, "max_tool_diameter": 0"#,
        r#""body_diameter": -1e308, "body_length": -1e308, "max_tool_length": -1e308, "max_tool_diameter": -1e308"#,
    ] {
        let parts = format!(
            r#"[{{ "id": "h", "type": "head", "spindle": {{ {fields} }}, "tool_station": {{ {fields} }} }}]"#
        );
        let t = parse_mch_tree(&tree("inch", &parts)).expect("unstated lengths must parse");
        let h = t.head().expect("head");
        let body = h.spindle_body.expect("body");
        let station = h.tool_station.expect("station");
        assert_eq!((body.diameter, body.length), (None, None), "{fields}");
        assert_eq!(
            (station.max_tool_length, station.max_tool_diameter),
            (None, None),
            "{fields}"
        );
    }
}
