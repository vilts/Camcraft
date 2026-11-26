//! A selected DXF chain with arcs (#3937): a die circle and a line-and-arc
//! punch cut as true arcs, verified by the wire laws and posted as G2 / G3
//! with I J through the generic ISO, Sodick and G-EDM wire posts, each to
//! its golden. TEXT and DIMENSION entities no longer block the cut. A closed SPLINE is read
//! as tangent arcs (#1865) and cut the same way.
//!
//! Regenerate the goldens after an intentional change, then read the diff:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test wire_dxf_selection arcs`

use super::*;

pub(super) const ISO: &str = "Generic ISO Wire EDM";
pub(super) const SODICK: &str = "Sodick Wire EDM (Mark 21)";
pub(super) const GEDM: &str = "G-EDM Wire EDM";
pub(super) const GEDM_FIXTURE: &str = include_str!("../edm/g-edm-fixture.mch");

/// A radius 5 circle about the drawing's origin, with a TEXT and a
/// DIMENSION entity beside it.
pub(super) const DIE: &str = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
    0\nCIRCLE\n8\noutline\n10\n0\n20\n0\n40\n5\n\
    0\nTEXT\n8\nnotes\n10\n20\n20\n20\n40\n2\n1\nDIE 10\n\
    0\nDIMENSION\n8\nnotes\n10\n-5\n20\n-8\n0\nENDSEC\n0\nEOF\n";

/// A 20 x 20 L about the origin: its corner at (10, -10) rounded by a
/// radius 3 fillet (bulge tan(22.5 degrees)), a sharp inside corner at
/// (0, 0).
pub(super) const PUNCH: &str = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
    0\nLWPOLYLINE\n8\noutline\n90\n7\n70\n1\n10\n-10\n20\n-10\n10\n7\n20\n-10\n42\n0.41421356237309503\n\
    10\n10\n20\n-7\n10\n10\n20\n0\n10\n0\n20\n0\n10\n0\n20\n10\n10\n-10\n20\n10\n0\nENDSEC\n0\nEOF\n";

/// A four-centre oval of tangent bulged arcs (#3959), as a DXF SPLINE or
/// ELLIPSE is read: no arc centre stands on the lattice.
const OVAL: &str = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
    0\nLWPOLYLINE\n8\noutline\n90\n4\n70\n1\n\
    10\n8.528028654224\n20\n-3.490278871102\n42\n0.281861053657224\n\
    10\n8.528028654224\n20\n3.490278871102\n42\n0.560231504260063\n\
    10\n-8.528028654224\n20\n3.490278871102\n42\n0.281861053657224\n\
    10\n-8.528028654224\n20\n-3.490278871102\n42\n0.560231504260063\n0\nENDSEC\n0\nEOF\n";

/// A start pick at a world point (#3339 moved `start` into `picks`).
pub(super) fn start_at(p: [f64; 2]) -> camcraft_app::path_picks::PathPicks {
    camcraft_app::path_picks::PathPicks {
        start: vec![camcraft_app::path_picks::PathPick::xy(camcraft_app::geo2d::Point2::new(p[0], p[1]))],
        exit: Vec::new(),
    }
}

/// The drawing point (`x`, `y`) in world XY.
pub(super) fn to_world(world: &World, x: f64, y: f64) -> [f64; 2] {
    let (e, _) = sketch::find(world, "drawing").expect("drawing");
    let pose = sketch::world_pose(world, &e).expect("placed");
    let q = camcraft_app::geometry::transform_point(Point3::new(x, y, 0.0), pose.pos, pose.rot);
    [q.x, q.y]
}

/// `dxf` placed and picked as `document` does, on `machine`, cut on
/// `side` from the drawing point `hole` under `condition`.
pub(super) fn doc(dxf: &str, machine: &str, condition: &str, side: WireSide, hole: (f64, f64), sharp: bool) -> World {
    let (world, mut h) = document(dxf);
    if machine != FIXTURE {
        let m = machine::parse_mch(machine).expect("machine");
        h.execute(&world, LoadMachine::new(&world, "wire.mch".into(), Some(m)).expect("mount"));
    }
    let op = WireContourOp {
        condition: condition.into(),
        side,
        picks: start_at(to_world(&world, hole.0, hole.1)),
        sharp_corners: sharp,
        ..wire_op()
    };
    h.execute(&world, EditOperationParams::new(&world, "Cut", OpParams::WireContour(op)).expect("edit"));
    world
}

fn generated(world: &World) -> camcraft_app::cl::Toolpath {
    let cmd = GenerateOperation::new(world, "Cut").unwrap_or_else(|e| panic!("generates: {e}"));
    let mut h = History::default();
    h.execute(world, cmd);
    let e = model::find_by_name(world, "Cut").expect("operation");
    let verdict = operation::wire_verify::wire_verdict(world, &e).expect("wire").expect("verifiable");
    assert!(verdict.findings.is_empty() && verdict.captured && verdict.slugs.len() == 1, "{verdict:?}");
    model::toolpath_of(world, &e).expect("cache").path.clone()
}

/// The program `post` writes for `world`.
fn posted(world: &World, post: &str, tag: &str) -> String {
    let file = std::env::temp_dir().join(format!("camcraft-dxf-arcs-{}-{}.nc", std::process::id(), tag.replace('/', "-")));
    let nc = posting::post_script(world, &file, posting::builtin_post_text(post).expect("post"))
        .unwrap_or_else(|e| panic!("{post} posts: {e}"))
        .program;
    let _ = std::fs::remove_file(file);
    nc
}

/// Post through `post` and hold the program to `golden`.
pub(super) fn golden(world: &World, post: &str, golden: &str) -> String {
    let nc = posted(world, post, golden);
    let path = format!("tests/golden/nc/{golden}");
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(&path, &nc).expect("write golden");
    }
    let want = std::fs::read_to_string(&path).expect("golden present");
    assert_eq!(nc, want, "{post} drifted from {golden}");
    nc
}

pub(super) fn arc_blocks(nc: &str) -> Vec<&str> {
    nc.lines().filter(|l| l.starts_with("G2 ") || l.starts_with("G3 ") || l.starts_with("G02") || l.starts_with("G03")).collect()
}

#[test]
fn die_circle_cuts_two_half_arcs_from_its_centre_and_posts_g3_with_ij() {
    let world = doc(DIE, FIXTURE, "C110", WireSide::Die, (0.0, 0.0), false);
    let path = generated(&world);
    assert_eq!(path.arc_count(), 2, "{:?}", path.moves);
    assert!(path.notes.iter().any(|n| n.contains("towards +X")), "{:?}", path.notes);
    assert!(path.notes.iter().any(|n| n.contains("2 TEXT, MTEXT or DIMENSION entities")), "{:?}", path.notes);
    // The arcs stand on the kerf circle about the drawn centre.
    let c = to_world(&world, 0.0, 0.0);
    for m in path.moves.iter().filter(|m| m.arc.is_some()) {
        let a = m.arc.expect("arc");
        assert!((a.center.x - c[0]).abs() < 1e-9 && (a.center.y - c[1]).abs() < 1e-9, "{a:?}");
        let r = (m.to.x - c[0]).hypot(m.to.y - c[1]);
        assert!((r - (5.0 - 0.165)).abs() < 1e-9, "{r}");
    }
    let nc = golden(&world, ISO, "wire-iso/wire_dxf_die_circle.nc");
    let arcs = arc_blocks(&nc);
    assert_eq!(arcs.len(), 2, "{nc}");
    // Each half arc turns counter-clockwise about the centre: I = -4.835 or +4.835.
    assert!(arcs.iter().all(|l| l.starts_with("G3") && (l.contains("I-4.835") || l.contains("I4.835"))), "{arcs:?}");
    let nc = golden(&world, SODICK, "sodick-wire/wire_dxf_die_circle.nc");
    assert_eq!(arc_blocks(&nc).len(), 2, "{nc}");
    let gedm = doc(DIE, GEDM_FIXTURE, "R1", WireSide::Die, (0.0, 0.0), false);
    generated(&gedm);
    let nc = golden(&gedm, GEDM, "g-edm/wire_dxf_die_circle.nc");
    assert_eq!(arc_blocks(&nc).len(), 2, "{nc}");
}

#[test]
fn line_and_arc_punch_pivots_its_corners_and_posts_its_goldens() {
    let world = doc(PUNCH, FIXTURE, "C110", WireSide::Punch, (20.0, 0.0), true);
    let path = generated(&world);
    // The fillet, and a pivot about each of the four sharp outside corners.
    assert_eq!(path.arc_count(), 5, "{:?}", path.moves);
    assert!(path.notes.iter().any(|n| n.contains("sharp inside corners stated")), "{:?}", path.notes);
    for (post, file) in [(ISO, "wire-iso/wire_dxf_punch_fillet.nc"), (SODICK, "sodick-wire/wire_dxf_punch_fillet.nc")] {
        let nc = golden(&world, post, file);
        assert_eq!(arc_blocks(&nc).len(), 5, "{nc}");
    }
    let gedm = doc(PUNCH, GEDM_FIXTURE, "R1", WireSide::Punch, (20.0, 0.0), true);
    generated(&gedm);
    let nc = golden(&gedm, GEDM, "g-edm/wire_dxf_punch_fillet.nc");
    assert_eq!(arc_blocks(&nc).len(), 5, "{nc}");
}

/// A closed clamped cubic SPLINE (#1865): it starts and ends at (10, 0) with the same +Y
/// tangent, so the loop is smooth. The reader lays it as tangent arcs within 1 um.
const SPLINE: &str = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
    0\nSPLINE\n8\noutline\n70\n8\n71\n3\n72\n10\n73\n6\n74\n0\n\
    40\n0\n40\n0\n40\n0\n40\n0\n40\n1\n40\n2\n40\n3\n40\n3\n40\n3\n40\n3\n\
    10\n10\n20\n0\n10\n10\n20\n10\n10\n-10\n20\n10\n10\n-10\n20\n-10\n10\n10\n20\n-10\n10\n10\n20\n0\n\
    0\nENDSEC\n0\nEOF\n";

#[test]
fn closed_spline_die_cuts_as_tangent_arcs_and_posts_its_golden() {
    let world = doc(SPLINE, FIXTURE, "C110", WireSide::Die, (0.0, 0.0), false);
    let (_, drawing) = sketch::find(&world, "drawing").expect("drawing");
    assert_eq!(drawing.data.chains().closed.len(), 1, "one closed chain");
    let path = generated(&world);
    // The spline's arcs, offset by the kerf: far fewer moves than a chord path.
    assert!(path.arc_count() >= 8, "{:?}", path.moves);
    assert!(path.moves.len() < 200, "{} moves", path.moves.len());
    let nc = golden(&world, ISO, "wire-iso/wire_dxf_spline_die.nc");
    assert_eq!(arc_blocks(&nc).len(), path.arc_count(), "{nc}");
}

#[test]
fn arcs_stay_refused_for_taper_and_a_hatch_still_blocks_the_cut() {
    let world = doc(DIE, FIXTURE, "C110", WireSide::Die, (0.0, 0.0), false);
    let tapered = WireContourOp { picks: start_at(to_world(&world, 0.0, 0.0)), taper_deg: 1.0, ..wire_op() };
    let mut h = History::default();
    h.execute(&world, EditOperationParams::new(&world, "Cut", OpParams::WireContour(tapered)).expect("taper"));
    let why = refusal(&world);
    assert!(why.contains("single vertical pass"), "{why}");
    let hatch = DIE.replace("0\nENDSEC\n0\nEOF", "0\nHATCH\n8\nnotes\n0\nENDSEC\n0\nEOF");
    let world = doc(&hatch, FIXTURE, "C110", WireSide::Die, (0.0, 0.0), false);
    let why = refusal(&world);
    assert!(why.contains("skipped 1 HATCH entity"), "{why}");
    // #1865: a block reference no longer stops the import, but its clearance from the wire
    // cannot be checked, so the cut is refused by name.
    let insert = DIE.replace("0\nENDSEC\n0\nEOF", "0\nINSERT\n8\nnotes\n2\nBOLT\n10\n30\n20\n0\n0\nENDSEC\n0\nEOF");
    let world = doc(&insert, FIXTURE, "C110", WireSide::Die, (0.0, 0.0), false);
    let why = refusal(&world);
    assert!(why.contains("skipped 1 INSERT entity"), "{why}");
}

/// The X and Y words of `line`, with the modal values of `at` where a word
/// is not written.
fn xy_of(line: &str, at: (f64, f64)) -> (f64, f64) {
    let word = |c: char| line.split_whitespace().find_map(|w| w.strip_prefix(c).and_then(|v| v.parse::<f64>().ok()));
    (word('X').unwrap_or(at.0), word('Y').unwrap_or(at.1))
}

#[test]
fn tangent_arcs_post_arc_to_arc_with_no_block_that_stands_still() {
    let world = doc(OVAL, FIXTURE, "C110", WireSide::Die, (0.0, 0.0), false);
    let path = generated(&world);
    assert_eq!(path.arc_count(), 5, "four arcs, the seam's arc in two parts: {:?}", path.moves);
    let nc = posted(&world, ISO, "oval");
    let blocks: Vec<&str> = nc.lines().filter(|l| ["G0 ", "G1 ", "G2 ", "G3 "].iter().any(|g| l.starts_with(g))).collect();
    let first = blocks.iter().position(|l| l.starts_with("G3")).expect("an arc block");
    let last = blocks.iter().rposition(|l| l.starts_with("G3")).expect("an arc block");
    assert_eq!(last - first + 1, 5, "no block between two arcs: {nc}");
    assert!(blocks[first..=last].iter().all(|l| l.starts_with("G3")), "{nc}");
    // Every motion block moves the wire.
    let mut at = xy_of(blocks[0], (f64::NAN, f64::NAN));
    for l in &blocks[1..] {
        let to = xy_of(l, at);
        assert!((to.0 - at.0).hypot(to.1 - at.1) > 1e-6, "a block with no motion: {l}\n{nc}");
        at = to;
    }
}
