//! The tangent arc lead on a selected DXF chain (#3991): a die circle, a
//! line-and-arc punch and an exact polygon entered and left on tangent
//! arcs, verified by the wire laws and the wire verdict, and posted through
//! the generic ISO, Sodick and G-EDM wire posts, each to its golden; a hole
//! too near is refused by name; a radius not over the kerf offset and a
//! pair the arc lead is not cut with are refused at the command door;
//! the straight lead stays the default.
//!
//! Regenerate the goldens after an intentional change, then read the diff:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test wire_dxf_selection arc_lead`

use super::arcs::*;
use super::*;

/// `doc`, then the tangent arc lead of `radius` stated by one edit.
fn led(dxf: &str, machine: &str, condition: &str, side: WireSide, hole: (f64, f64), sharp: bool, radius: f64) -> World {
    let world = doc(dxf, machine, condition, side, hole, sharp);
    let op = WireContourOp {
        condition: condition.into(),
        side,
        picks: start_at(to_world(&world, hole.0, hole.1)),
        sharp_corners: sharp,
        lead_arc_radius: Some(radius),
        ..wire_op()
    };
    let mut h = History::default();
    h.execute(&world, EditOperationParams::new(&world, "Cut", OpParams::WireContour(op)).expect("edit"));
    world
}

/// Generate and read the wire verdict: no finding, and two freed pieces,
/// both captured - the slug and the small waste piece that the lead-in and
/// the lead-out close between them.
fn generated_led(world: &World) -> camcraft_app::cl::Toolpath {
    let cmd = GenerateOperation::new(world, "Cut").unwrap_or_else(|e| panic!("generates: {e}"));
    let mut h = History::default();
    h.execute(world, cmd);
    let e = model::find_by_name(world, "Cut").expect("operation");
    let verdict = operation::wire_verify::wire_verdict(world, &e).expect("wire").expect("verifiable");
    assert!(verdict.findings.is_empty() && verdict.captured && verdict.slugs.len() == 2, "{verdict:?}");
    let path = model::toolpath_of(world, &e).expect("cache").path.clone();
    assert!(path.notes.iter().any(|n| n.contains("close a small waste piece")), "{:?}", path.notes);
    path
}

/// The first move is straight, the second an arc whose walk direction at
/// its end is that of the lap's first move at its start (within the
/// lattice), and the mirror at the end of the path.
fn assert_tangent_leads(path: &camcraft_app::cl::Toolpath) {
    let m = &path.moves;
    let n = m.len();
    assert!(m[0].arc.is_none() && m[1].arc.is_some(), "a straight, then the lead arc: {m:?}");
    assert!(m[n - 1].arc.is_none() && m[n - 2].arc.is_some(), "the lead-out arc, then a straight: {m:?}");
    let dir = |mv: &camcraft_app::cl::Move, end: bool| {
        let q = if end { mv.to } else { mv.from };
        let v = match mv.arc {
            None => (mv.to.x - mv.from.x, mv.to.y - mv.from.y),
            Some(a) => {
                let r = (q.x - a.center.x, q.y - a.center.y);
                if a.ccw { (-r.1, r.0) } else { (r.1, -r.0) }
            }
        };
        let l = v.0.hypot(v.1);
        (v.0 / l, v.1 / l)
    };
    for i in (0..2).chain(n - 3..n - 1) {
        let (a, b) = (dir(&m[i], true), dir(&m[i + 1], false));
        assert!((a.0 * b.1 - a.1 * b.0).abs() < 1e-5 && a.0 * b.0 + a.1 * b.1 > 0.0, "joint {i}: {a:?} then {b:?}");
    }
}

#[test]
fn die_circle_with_an_arc_lead_is_tangent_verified_and_posts_its_goldens() {
    let world = led(DIE, FIXTURE, "C110", WireSide::Die, (0.0, 0.0), false, 1.0);
    let path = generated_led(&world);
    assert_tangent_leads(&path);
    // The two lead arcs and the two half circles.
    assert_eq!(path.arc_count(), 4, "{:?}", path.moves);
    assert!(path.notes.iter().any(|n| n.contains("tangent arc lead-in of radius 1.0000 mm")), "{:?}", path.notes);
    let nc = golden(&world, ISO, "wire-iso/wire_dxf_die_circle_arc_lead.nc");
    assert_eq!(arc_blocks(&nc).len(), 4, "{nc}");
    // Sodick: the first cut move is the straight from the hole.
    let nc = golden(&world, SODICK, "sodick-wire/wire_dxf_die_circle_arc_lead.nc");
    assert_eq!(arc_blocks(&nc).len(), 4, "{nc}");
    let gedm = led(DIE, GEDM_FIXTURE, "R1", WireSide::Die, (0.0, 0.0), false, 1.0);
    generated_led(&gedm);
    let nc = golden(&gedm, GEDM, "g-edm/wire_dxf_die_circle_arc_lead.nc");
    assert_eq!(arc_blocks(&nc).len(), 4, "{nc}");
}

#[test]
fn line_and_arc_punch_and_a_polygon_take_the_arc_lead_and_pass_the_verdict() {
    let world = led(PUNCH, FIXTURE, "C110", WireSide::Punch, (20.0, -3.0), true, 1.5);
    let path = generated_led(&world);
    assert_tangent_leads(&path);
    // The fillet, four pivots and the two lead arcs.
    assert_eq!(path.arc_count(), 7, "{:?}", path.moves);
    golden(&world, ISO, "wire-iso/wire_dxf_punch_fillet_arc_lead.nc");
    // An exact polygon is cut on its lines with the arc lead (the
    // straight lead keeps the polygon path, byte for byte).
    let (world, mut h) = document(DXF);
    let op = WireContourOp { lead_arc_radius: Some(1.0), ..wire_op() };
    h.execute(&world, EditOperationParams::new(&world, "Cut", OpParams::WireContour(op)).expect("edit"));
    let path = generated_led(&world);
    assert_tangent_leads(&path);
    assert!(path.arc_count() >= 2, "{:?}", path.moves);
}

#[test]
fn a_hole_too_near_and_a_pair_the_arc_lead_is_not_cut_with_are_refused_by_name() {
    // The hole stands 1.335 mm off the radius 4.835 path: a radius 1 lead
    // needs more than 2.
    let world = led(DIE, FIXTURE, "C110", WireSide::Die, (3.5, 0.0), false, 1.0);
    let why = refusal(&world);
    assert!(why.contains("too near the entry point") && why.contains("tangent lead arc of radius 1"), "{why}");
    // A taper beside the arc lead is refused at the command door.
    let op = WireContourOp { lead_arc_radius: Some(1.0), taper_deg: 1.0, ..wire_op() };
    let why = EditOperationParams::new(&world, "Cut", OpParams::WireContour(op)).err().expect("refused").to_string();
    assert!(why.contains("tangent arc lead") && why.contains("taper"), "{why}");
}

/// Validate early (#3991 review): a radius at or under the kerf offset of
/// the resolved condition (C110: 0.25 mm wire, 0.04 mm gap, 0.165 mm) is
/// refused when the command is made - an edit and a new operation alike.
/// The refused edit leaves the operation and the undo history unchanged.
#[test]
fn a_radius_at_or_under_the_kerf_offset_is_refused_at_the_command_door() {
    let (world, h) = document(DXF);
    let e = model::find_by_name(&world, "Cut").expect("operation");
    let before = model::operation_of(&world, &e).expect("op").params;
    let done = h.done_len();
    let offset = 0.25 * 0.5 + 0.04;
    for radius in [offset, 0.1] {
        let op = WireContourOp { lead_arc_radius: Some(radius), ..wire_op() };
        let why = EditOperationParams::new(&world, "Cut", OpParams::WireContour(op.clone())).err().expect("edit refused").to_string();
        assert!(why.contains("must be larger than the kerf offset (0.1650 mm)"), "{radius}: {why}");
        let why = CreateOperation::new(&world, "Cut 2", "Wire", SrcRef::Entity("drawing".into()), OpParams::WireContour(op))
            .err()
            .expect("create refused")
            .to_string();
        assert!(why.contains("must be larger than the kerf offset"), "{radius}: {why}");
    }
    assert_eq!(model::operation_of(&world, &e).expect("op").params, before, "the refused edit changes nothing");
    assert!(model::find_by_name(&world, "Cut 2").is_none(), "the refused create makes no operation");
    assert_eq!(h.done_len(), done, "no history entry");
    // Just over the offset passes the door.
    let op = WireContourOp { lead_arc_radius: Some(offset + 0.001), ..wire_op() };
    assert!(EditOperationParams::new(&world, "Cut", OpParams::WireContour(op)).is_ok());
}
