//! The generated ("loose") machine model (#2584): one closed block or
//! cylinder for each part that states a number to size it, posed along
//! the joint chain for a set of axis values. Pins the bodies and their
//! poses for a 3-axis VMC and for the tilt-head example
//! (`assets/machines/tilthead.mch`, mapped by hand: this crate has no
//! edge to the machine-file reader) at two axis positions each, the
//! "generated" mark, and the named note of a part with no number.
//!
//! Gate map (M4.4): covers camcraft_sim::machine_model.

use std::collections::HashMap;
use std::f64::consts::PI;

use camcraft_sim::geometry::Triangle;
use camcraft_sim::machine_model::{
    self, Body, BodyKind, BodySource, JointKind, MachineSpec, PartSpec, SpindleSpec, ToolStationSpec, SIDES,
};

const EPS: f64 = 1e-9;

fn linear(id: &str, parent: usize, axis: [f64; 3], travel: (f64, f64)) -> PartSpec {
    PartSpec { axis: Some(axis), travel: Some(travel), ..PartSpec::new(id, Some(parent), JointKind::Linear) }
}

fn rotary(id: &str, parent: usize, axis: [f64; 3]) -> PartSpec {
    PartSpec { axis: Some(axis), travel: Some((-120.0, 120.0)), ..PartSpec::new(id, Some(parent), JointKind::Rotary) }
}

fn head(id: &str, parent: usize, d: f64, len: f64, tool_len: f64, tool_d: f64) -> PartSpec {
    PartSpec {
        spindle: Some(SpindleSpec { diameter: Some(d), length: Some(len) }),
        tool_station: Some(ToolStationSpec { max_tool_length: Some(tool_len), max_tool_diameter: Some(tool_d) }),
        ..PartSpec::new(id, Some(parent), JointKind::Fixed)
    }
}

/// A 3-axis VMC: X and Y move the table (the body moves against the
/// program axis), Z moves the head.
fn vmc() -> MachineSpec {
    MachineSpec {
        parts: vec![
            PartSpec::new("base", None, JointKind::Fixed),
            linear("x", 0, [-1.0, 0.0, 0.0], (-400.0, 400.0)),
            linear("y", 1, [0.0, -1.0, 0.0], (-250.0, 250.0)),
            linear("z", 0, [0.0, 0.0, 1.0], (-300.0, 300.0)),
            head("head", 3, 100.0, 150.0, 250.0, 80.0),
        ],
    }
}

/// `tilthead.mch`: X and Y under the table, Z carries A, A carries B, B
/// carries the head.
fn tilt_head() -> MachineSpec {
    MachineSpec {
        parts: vec![
            PartSpec::new("base", None, JointKind::Fixed),
            linear("crl", 0, [-1.0, 0.0, 0.0], (-600.0, 600.0)),
            linear("sld", 1, [0.0, -1.0, 0.0], (-500.0, 500.0)),
            linear("col", 0, [0.0, 0.0, 1.0], (-500.0, 500.0)),
            rotary("tla", 3, [1.0, 0.0, 0.0]),
            rotary("tlb", 4, [0.0, 1.0, 0.0]),
            head("hed", 5, 90.0, 110.0, 300.0, 100.0),
        ],
    }
}

/// `(min, max)` corner of a body's triangles.
fn bbox(body: &Body) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for t in &body.triangles {
        for p in [t.a, t.b, t.c] {
            for (k, v) in [p.x, p.y, p.z].into_iter().enumerate() {
                lo[k] = lo[k].min(v);
                hi[k] = hi[k].max(v);
            }
        }
    }
    (lo, hi)
}

fn find<'a>(bodies: &'a [Body], id: &str, kind: BodyKind) -> &'a Body {
    bodies
        .iter()
        .find(|b| b.part_id == id && b.kind == kind)
        .unwrap_or_else(|| panic!("no {kind:?} body of part {id}"))
}

#[track_caller]
fn assert_box(bodies: &[Body], id: &str, lo: [f64; 3], hi: [f64; 3]) {
    let (got_lo, got_hi) = bbox(find(bodies, id, BodyKind::Block));
    for k in 0..3 {
        assert!((got_lo[k] - lo[k]).abs() < EPS && (got_hi[k] - hi[k]).abs() < EPS,
            "block {id}: got {got_lo:?}..{got_hi:?}, want {lo:?}..{hi:?}");
    }
}

/// A cylinder of radius `r` whose axis runs along world axis `along`
/// from `from` to `to`, through `centre` on the two other axes. The round
/// face is a circumscribed polygon: never inside `r`, at most the
/// tessellation allowance outside it.
#[track_caller]
fn assert_cylinder(body: &Body, along: usize, from: f64, to: f64, centre: [f64; 3], r: f64) {
    let (lo, hi) = bbox(body);
    let allow = r / (PI / SIDES as f64).cos() + EPS;
    for k in 0..3 {
        if k == along {
            assert!((lo[k] - from).abs() < EPS && (hi[k] - to).abs() < EPS,
                "cylinder of {}: axis {k} spans {}..{}, want {from}..{to}", body.part_id, lo[k], hi[k]);
        } else {
            for reach in [centre[k] - lo[k], hi[k] - centre[k]] {
                assert!(reach >= r - EPS && reach <= allow,
                    "cylinder of {}: axis {k} reaches {reach} from the centre, want {r}..{allow}", body.part_id);
            }
        }
    }
}

/// Each directed edge has its reverse exactly once, and the shell is
/// outward: the signed volume is the stated one.
#[track_caller]
fn assert_closed(body: &Body, volume: f64) {
    let key = |p: camcraft_sim::geometry::Point3| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()];
    let mut edges: HashMap<([u64; 3], [u64; 3]), i32> = HashMap::new();
    let mut six_v = 0.0;
    for &Triangle { a, b, c } in &body.triangles {
        for (p, q) in [(a, b), (b, c), (c, a)] {
            *edges.entry((key(p), key(q))).or_default() += 1;
        }
        six_v += a.dot(b.cross(c));
    }
    assert!(!edges.is_empty(), "body of {} has no triangle", body.part_id);
    for ((p, q), n) in &edges {
        assert_eq!(*n, 1, "body of {}: an edge is used {n} times one way", body.part_id);
        assert_eq!(edges.get(&(*q, *p)), Some(&1), "body of {}: an open edge", body.part_id);
    }
    let got = six_v / 6.0;
    assert!((got - volume).abs() <= 1e-6 * volume, "body of {}: volume {got}, want {volume}", body.part_id);
}

fn cylinder_volume(r: f64, len: f64) -> f64 {
    let n = SIDES as f64;
    n * r * r * (PI / n).tan() * len
}

#[test]
fn vmc_bodies_at_two_axis_positions() {
    let model = machine_model::generate(&vmc()).expect("the VMC generates");
    assert!(model.notes.is_empty(), "each VMC part states its numbers: {:?}", model.notes);
    let ids: Vec<(&str, BodyKind)> = model.bodies.iter().map(|b| (b.part_id.as_str(), b.kind)).collect();
    assert_eq!(ids, [
        ("base", BodyKind::Block), ("x", BodyKind::Block), ("y", BodyKind::Block), ("z", BodyKind::Block),
        ("head", BodyKind::Spindle), ("head", BodyKind::ToolEnvelope),
    ]);

    // Position 1: each axis at zero. The table stack hangs under z = 0,
    // the head stack stands over the spindle nose at z = 0.
    let home = model.pose(&[("x", 0.0), ("y", 0.0), ("z", 0.0)]).expect("home pose");
    assert_box(&home, "y", [-62.5, -250.0, -125.0], [62.5, 250.0, 0.0]);
    assert_box(&home, "x", [-400.0, -100.0, -325.0], [400.0, 100.0, -125.0]);
    assert_box(&home, "base", [-400.0, -250.0, -405.0], [400.0, 250.0, -325.0]);
    assert_box(&home, "z", [-75.0, -75.0, 150.0], [75.0, 75.0, 750.0]);
    assert_cylinder(find(&home, "head", BodyKind::Spindle), 2, 0.0, 150.0, [0.0; 3], 50.0);
    assert_cylinder(find(&home, "head", BodyKind::ToolEnvelope), 2, -250.0, 0.0, [0.0; 3], 40.0);

    // Position 2: X100 Y-50 Z-200. The table parts move against X and Y;
    // Y rides on X; the head rides on Z; the base stays.
    let cut = model.pose(&[("z", -200.0), ("x", 100.0), ("y", -50.0)]).expect("cut pose");
    assert_box(&cut, "x", [-500.0, -100.0, -325.0], [300.0, 100.0, -125.0]);
    assert_box(&cut, "y", [-162.5, -200.0, -125.0], [-37.5, 300.0, 0.0]);
    assert_box(&cut, "base", [-400.0, -250.0, -405.0], [400.0, 250.0, -325.0]);
    assert_box(&cut, "z", [-75.0, -75.0, -50.0], [75.0, 75.0, 550.0]);
    assert_cylinder(find(&cut, "head", BodyKind::Spindle), 2, -200.0, -50.0, [0.0; 3], 50.0);
    assert_cylinder(find(&cut, "head", BodyKind::ToolEnvelope), 2, -450.0, -200.0, [0.0; 3], 40.0);
}

#[test]
fn a_linear_head_keeps_its_spindle_tool_envelope_and_travel_block() {
    for travel in [Some((-50.0, 50.0)), None] {
        let mut spec = vmc();
        spec.parts[4].joint = JointKind::Linear;
        spec.parts[4].axis = Some([0.0, 0.0, 1.0]);
        spec.parts[4].travel = travel;
        let model = machine_model::generate(&spec).expect("the moving head generates");
        assert_eq!(
            model.bodies.iter().filter(|b| b.part_id == "head").count(),
            if travel.is_some() { 3 } else { 2 }
        );
        for value in [0.0, 40.0] {
            let posed = model
                .pose(&[("x", 0.0), ("y", 0.0), ("z", -200.0), ("head", value)])
                .expect("linear head pose");
            assert_cylinder(
                find(&posed, "head", BodyKind::Spindle),
                2,
                -200.0 + value,
                -50.0 + value,
                [0.0; 3],
                50.0,
            );
            assert_cylinder(
                find(&posed, "head", BodyKind::ToolEnvelope),
                2,
                -450.0 + value,
                -200.0 + value,
                [0.0; 3],
                40.0,
            );
            if travel.is_some() {
                assert_box(
                    &posed,
                    "head",
                    [-12.5, -12.5, -50.0 + value],
                    [12.5, 12.5, 50.0 + value],
                );
            }
        }
        if travel.is_some() {
            assert!(model.notes.is_empty(), "{:?}", model.notes);
        }
    }
}

#[test]
fn a_rotary_head_keeps_and_turns_its_spindle_and_tool_envelope() {
    let mut spec = vmc();
    spec.parts[4].joint = JointKind::Rotary;
    spec.parts[4].axis = Some([0.0, 1.0, 0.0]);
    let model = machine_model::generate(&spec).expect("the rotary head generates");
    assert_eq!(
        model.bodies.iter().filter(|b| b.part_id == "head").count(),
        2
    );
    assert!(model.notes.is_empty(), "{:?}", model.notes);
    let home = model
        .pose(&[("x", 0.0), ("y", 0.0), ("z", -200.0), ("head", 0.0)])
        .expect("home pose");
    assert_cylinder(
        find(&home, "head", BodyKind::Spindle),
        2,
        -200.0,
        -50.0,
        [0.0; 3],
        50.0,
    );
    assert_cylinder(
        find(&home, "head", BodyKind::ToolEnvelope),
        2,
        -450.0,
        -200.0,
        [0.0; 3],
        40.0,
    );
    let turned = model
        .pose(&[("x", 0.0), ("y", 0.0), ("z", -200.0), ("head", 90.0)])
        .expect("turned pose");
    assert_cylinder(
        find(&turned, "head", BodyKind::Spindle),
        0,
        0.0,
        150.0,
        [0.0, 0.0, -200.0],
        50.0,
    );
    assert_cylinder(
        find(&turned, "head", BodyKind::ToolEnvelope),
        0,
        -250.0,
        0.0,
        [0.0, 0.0, -200.0],
        40.0,
    );
}

#[test]
fn tilt_head_bodies_at_two_axis_positions() {
    let model = machine_model::generate(&tilt_head()).expect("the tilt head generates");
    // A rotary part states an angle, no length: no body, and a named note.
    assert_eq!(model.notes.len(), 2, "{:?}", model.notes);
    for id in ["tla", "tlb"] {
        assert!(model.notes.iter().any(|n| n.contains(&format!("part {id}")) && n.contains("no body")),
            "no note names {id}: {:?}", model.notes);
        assert!(model.bodies.iter().all(|b| b.part_id != id), "{id} got a guessed body");
    }

    let zero = [("crl", 0.0), ("sld", 0.0), ("col", 0.0), ("tla", 0.0), ("tlb", 0.0)];
    let home = model.pose(&zero).expect("home pose");
    assert_box(&home, "sld", [-125.0, -500.0, -250.0], [125.0, 500.0, 0.0]);
    assert_box(&home, "crl", [-600.0, -150.0, -550.0], [600.0, 150.0, -250.0]);
    assert_box(&home, "base", [-600.0, -500.0, -670.0], [600.0, 500.0, -550.0]);
    assert_box(&home, "col", [-125.0, -125.0, 110.0], [125.0, 125.0, 1110.0]);
    assert_cylinder(find(&home, "hed", BodyKind::Spindle), 2, 0.0, 110.0, [0.0; 3], 45.0);
    assert_cylinder(find(&home, "hed", BodyKind::ToolEnvelope), 2, -300.0, 0.0, [0.0; 3], 50.0);

    // X50 Z-100 A90 B90. B turns the spindle from +Z to +X; A about X
    // then leaves it there. (A applied first would lay it along -Y.)
    let tilted = model
        .pose(&[("crl", 50.0), ("sld", 0.0), ("col", -100.0), ("tla", 90.0), ("tlb", 90.0)])
        .expect("tilted pose");
    let centre = [0.0, 0.0, -100.0];
    assert_cylinder(find(&tilted, "hed", BodyKind::Spindle), 0, 0.0, 110.0, centre, 45.0);
    assert_cylinder(find(&tilted, "hed", BodyKind::ToolEnvelope), 0, -300.0, 0.0, centre, 50.0);
    assert_box(&tilted, "col", [-125.0, -125.0, 10.0], [125.0, 125.0, 1010.0]);
    assert_box(&tilted, "crl", [-650.0, -150.0, -550.0], [550.0, 150.0, -250.0]);
    assert_box(&tilted, "sld", [-175.0, -500.0, -250.0], [75.0, 500.0, 0.0]);

    // A alone: +90 degrees about +X lays the spindle along -Y.
    let a90 = model
        .pose(&[("crl", 0.0), ("sld", 0.0), ("col", 0.0), ("tla", 90.0), ("tlb", 0.0)])
        .expect("A90 pose");
    assert_cylinder(find(&a90, "hed", BodyKind::Spindle), 1, -110.0, 0.0, [0.0; 3], 45.0);
}

#[test]
fn a_rotary_turns_about_its_pivot_and_a_part_sits_at_its_offset() {
    let mut spec = tilt_head();
    spec.parts[4].pivot = Some([0.0, 0.0, 200.0]);
    spec.parts[3].offset = [10.0, 20.0, 30.0];
    let model = machine_model::generate(&spec).expect("generates");
    let posed = model
        .pose(&[("crl", 0.0), ("sld", 0.0), ("col", 0.0), ("tla", 180.0), ("tlb", 0.0)])
        .expect("pose");
    // z -> 400 - z about the pivot line, then the column's offset.
    assert_cylinder(find(&posed, "hed", BodyKind::Spindle), 2, 320.0, 430.0, [10.0, 20.0, 0.0], 45.0);
    assert_box(&posed, "col", [-115.0, -105.0, 140.0], [135.0, 145.0, 1140.0]);
}

#[test]
fn each_body_is_closed_and_marked_generated() {
    let model = machine_model::generate(&vmc()).expect("the VMC generates");
    let posed = model.pose(&[("x", 100.0), ("y", -50.0), ("z", -200.0)]).expect("pose");
    for bodies in [&model.bodies, &posed] {
        assert_eq!(bodies.len(), 6);
        for b in bodies.iter() {
            assert_eq!(b.source, BodySource::Generated);
            assert_eq!(b.source.judged_on(), "judged on a generated model");
        }
        assert_closed(find(bodies, "base", BodyKind::Block), 800.0 * 500.0 * 80.0);
        assert_closed(find(bodies, "x", BodyKind::Block), 800.0 * 200.0 * 200.0);
        assert_closed(find(bodies, "y", BodyKind::Block), 500.0 * 125.0 * 125.0);
        assert_closed(find(bodies, "z", BodyKind::Block), 600.0 * 150.0 * 150.0);
        assert_closed(find(bodies, "head", BodyKind::Spindle), cylinder_volume(50.0, 150.0));
        assert_closed(find(bodies, "head", BodyKind::ToolEnvelope), cylinder_volume(40.0, 250.0));
    }
}

#[test]
fn a_part_with_no_number_gets_no_body_and_a_named_note() {
    let mut spec = vmc();
    spec.parts[1].travel = None; // X states no travel
    spec.parts[4].spindle = Some(SpindleSpec { diameter: Some(100.0), length: None });
    spec.parts[4].tool_station = None;
    let model = machine_model::generate(&spec).expect("unstated numbers are not an error");
    let kinds: Vec<(&str, BodyKind)> = model.bodies.iter().map(|b| (b.part_id.as_str(), b.kind)).collect();
    // No X travel: no X block, and no table block (its X size is unknown).
    assert_eq!(kinds, [("y", BodyKind::Block), ("z", BodyKind::Block)]);
    let note = |id: &str, word: &str| {
        assert!(model.notes.iter().any(|n| n.starts_with(&format!("machine model: part {id}:"))
            && n.contains(word) && n.contains("no body")), "no note for {id} / {word}: {:?}", model.notes);
    };
    note("base", "travel");
    note("x", "travel");
    note("head", "spindle length");
    note("head", "tool station");
    assert_eq!(model.notes.len(), 4, "{:?}", model.notes);
    // The unsized X part still moves what it carries.
    let posed = model.pose(&[("x", 100.0), ("y", 0.0), ("z", 0.0)]).expect("pose");
    assert_box(&posed, "y", [-162.5, -250.0, -125.0], [-37.5, 250.0, 0.0]);
    // With no spindle body under it, the Z block starts at the nose plane.
    assert_box(&posed, "z", [-75.0, -75.0, 0.0], [75.0, 75.0, 600.0]);
}
