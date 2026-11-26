//! #3215: a Wire Contour of a meshed model posts its circles and fillets
//! as G2 / G3 arcs, not as G01 chords. The model's facet polygon is read
//! back as arcs within the model's chord tolerance, cut by the analytic
//! wire contour (#3937), judged by the wire laws under that chord, and
//! posted by the generic ISO and the Sodick wire posts.
//!
//! Gate map (#3215): covers app::operation (wire_op, wire_verify,
//! generate's wire arm), app::posting_wire, motion::wire_edm::model_arcs,
//! sim::verify::wire (the chord tolerance on a vertical cut), the
//! wire-iso.cps and sodick-wire.cps scripts.
//! Provenance: our own tests over our own synthetic fixture.

use super::*;

const SODICK: &str = "Sodick Wire EDM (Mark 21)";
/// The fixture's C110: wire 0.25 mm, gap 0.04 mm.
const K: f64 = 0.165;

/// A dia-8 hole's opening meshed at the importer's chord: 63 facets.
fn hole() -> Mesh {
    let pts: Vec<(f64, f64)> = (0..63)
        .map(|i| {
            let t = std::f64::consts::TAU * f64::from(i) / 63.0;
            (4.0 * t.cos(), 4.0 * t.sin())
        })
        .collect();
    prism(&pts)
}

/// A die cut round the opening `mesh` from a start hole at the origin,
/// generated.
fn die_of(mesh: Mesh) -> World {
    let world = new_world();
    let mut h = History::default();
    let m = parse_mch(FIXTURE).expect("the fixture parses");
    h.execute(&world, LoadMachine::new(&world, "wire-iso.mch".into(), Some(m)).expect("mount"));
    h.execute(
        &world,
        camcraft_app::commands::AddTool::new(&world, "EDM-W025", camcraft_app::wire_tools::wire_tool(0.25)).expect("wire"),
    );
    h.execute(&world, ImportModel::from_mesh("opening", mesh));
    let op = WireContourOp {
        top_z: Height::fixed(12.0),
        bottom_z: Height::fixed(0.0),
        condition: "C110".into(),
        side: WireSide::Die,
        picks: camcraft_app::path_picks::PathPicks { start: vec![camcraft_app::path_picks::PathPick::xy(camcraft_app::geo2d::Point2::new(0.0, 0.0))], exit: Vec::new() },
        tabs: None,
        capture: true,
        both_pieces_restrained: false,
        stock_centered_in_xy_travel: false,
        taper_deg: 0.0,
        four_axis: None,
        burnout: None,
        clearance: operation::DEFAULT_CLEARANCE,
        skims: Vec::new(),
        sharp_corners: false,
        lead_arc_radius: None,
    };
    let c = CreateOperation::new(&world, "Die1", "EDM-W025", SrcRef::Entity("opening".into()), OpParams::WireContour(op)).expect("op");
    h.execute(&world, c);
    h.execute(&world, GenerateOperation::new(&world, "Die1").expect("generates"));
    world
}

fn posted(world: &World, post: &str) -> String {
    let out = std::env::temp_dir().join(format!("camcraft-wire-arcs-{}-{}.nc", post.len(), std::process::id()));
    let posted = posting::post_script(world, &out, cps(post)).expect("the wire program posts");
    let _ = std::fs::remove_file(&out);
    posted.program
}

/// One posted arc: start, end, and the centre the I J increments state.
#[derive(Debug)]
struct Arc {
    from: (f64, f64),
    to: (f64, f64),
    centre: (f64, f64),
}

/// The program's arcs, and its straight feed blocks.
fn moves(text: &str) -> (Vec<Arc>, usize) {
    let (mut x, mut y) = (0.0, 0.0);
    let (mut arcs, mut lines) = (Vec::new(), 0);
    for line in text.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        let word = |c: char| words.iter().find(|w| w.starts_with(c) && w[1..].parse::<f64>().is_ok()).map(|w| w[1..].parse::<f64>().unwrap_or(0.0));
        let g = words.iter().find(|w| w.starts_with('G')).copied().unwrap_or("");
        let (nx, ny) = (word('X').unwrap_or(x), word('Y').unwrap_or(y));
        match g {
            "G2" | "G02" | "G3" | "G03" => {
                let (i, j) = (word('I').unwrap_or(0.0), word('J').unwrap_or(0.0));
                arcs.push(Arc { from: (x, y), to: (nx, ny), centre: (x + i, y + j) });
            }
            "G1" | "G01" if word('X').is_some() || word('Y').is_some() => lines += 1,
            _ => {}
        }
        (x, y) = (nx, ny);
    }
    (arcs, lines)
}

fn radius(a: &Arc) -> f64 {
    (a.from.0 - a.centre.0).hypot(a.from.1 - a.centre.1)
}

#[test]
fn a_meshed_hole_posts_as_two_half_arcs_not_chords() {
    let world = die_of(hole());
    for post in [WIRE, SODICK] {
        let text = posted(&world, post);
        let (arcs, lines) = moves(&text);
        assert_eq!(arcs.len(), 2, "{post}: two half arcs\n{text}");
        for a in &arcs {
            assert!(a.centre.0.abs() < 1.5e-3 && a.centre.1.abs() < 1.5e-3, "{post}: the centre is the hole's: {a:?}");
            assert!((radius(a) - (4.0 - K)).abs() < 1.5e-3, "{post}: the radius is the hole's less the kerf: {a:?}");
            let end = (a.to.0 - a.centre.0).hypot(a.to.1 - a.centre.1);
            assert!((end - radius(a)).abs() < 1.5e-3, "{post}: {a:?}");
        }
        assert!(lines <= 2, "{post}: only the lead in and out are straight ({lines})\n{text}");
    }
}

#[test]
fn meshed_fillets_post_as_arcs_between_the_straight_sides() {
    let world = die_of(opening());
    for post in [WIRE, SODICK] {
        let text = posted(&world, post);
        let (arcs, lines) = moves(&text);
        assert_eq!(arcs.len(), 4, "{post}: one arc per fillet\n{text}");
        for a in &arcs {
            assert!((a.centre.0.abs() - 18.0).abs() < 1.5e-3 && (a.centre.1.abs() - 8.0).abs() < 1.5e-3, "{post}: {a:?}");
            assert!((radius(a) - (2.0 - K)).abs() < 1.5e-3, "{post}: {a:?}");
        }
        // Four sides (the seam splits one) plus the lead in and out: no
        // chord of a fillet.
        assert!(lines <= 7, "{post}: {lines} straight blocks\n{text}");
    }
}

#[test]
fn the_wire_laws_judge_the_read_arcs_under_the_model_chord() {
    let world = die_of(hole());
    let e = model::find_by_name(&world, "Die1").expect("op");
    let path = model::toolpath_of(&world, &e).expect("generated").path;
    assert!(path.notes.iter().any(|n| n.starts_with(camcraft_app::wire_edm::MODEL_ARCS_NOTE)), "{:?}", path.notes);
    let v = operation::wire_verify::wire_verdict(&world, &e).expect("a wire cut").expect("the laws run");
    assert!(v.findings.is_empty(), "{:?}", v.findings);
    let said = v.tolerance_said().expect("the chord is said");
    assert!(said.contains("the model's chord tolerance 0.0060"), "{said}");
}

#[test]
fn a_profile_with_no_arcs_keeps_its_polygon_cut() {
    let world = punch_doc(FIXTURE);
    let e = model::find_by_name(&world, "Punch1").expect("op");
    let path = model::toolpath_of(&world, &e).expect("generated").path;
    assert!(path.moves.iter().all(|m| m.arc.is_none()), "a sharp slug has no arc");
    assert!(!path.notes.iter().any(|n| n.starts_with(camcraft_app::wire_edm::MODEL_ARCS_NOTE)), "{:?}", path.notes);
}
