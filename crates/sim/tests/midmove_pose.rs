//! Mid-move pose changes judged along the move (M13.7's last owed
//! item): a single 5-axis cut that leans from one clear pose to
//! another clear pose THROUGH a wall at mid-travel is a gouge the
//! end-pose reading of the solid law (b2) cannot see - the end pose
//! sweeps clean, the from pose sweeps clean, the lean between them
//! runs the flute into the wall. The posed-contact law (f2) reads the
//! same intervals: a ball that leaves the part between two contact
//! poses is rest. Constant-pose moves keep their verdicts.
//!
//! Provenance: our own fixtures; the laws are the Prime Directive's
//! ("never gouge"), the pose rule RS-274's modal letters. No vendor
//! material consulted.
//!
//! Gate map (M13.7 mid-move): covers camcraft_sim::verify (midpose,
//! solid, contact, probe, solid_index).

use camcraft_sim::cl::{angles_to_axis, Move, MoveKind, Toolpath};
use camcraft_sim::geometry::{Point3, Triangle};
use camcraft_sim::verify::{posed_contact, solid_hits, Cutter, SolidLaw, ToolGeom, Warning};

/// A square-end stack: flute 20, shank 10, holder 10 - reach 40.
fn cutter() -> Cutter {
    Cutter {
        radius: 2.0,
        corner_r: 0.0,
        tip_angle: 0.0,
        flute_len: 20.0,
        shank_radius: 2.0,
        shank_len: 10.0,
        holder_radius: 4.0,
        holder_len: 10.0,
        body: None,
        neck: None,
    }
}

fn law() -> SolidLaw {
    SolidLaw { step: 0.25, allow: 0.01, hole_walls: false }
}

/// A solid box as facet soup, outward CCW.
fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| {
        Point3::new(
            [min[0], max[0]][i & 1],
            [min[1], max[1]][(i >> 1) & 1],
            [min[2], max[2]][(i >> 2) & 1],
        )
    };
    let quad = |a: usize, b: usize, c: usize, d: usize| -> [Triangle; 2] {
        [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))]
    };
    let mut t = Vec::with_capacity(12);
    t.extend(quad(4, 5, 7, 6)); // +z top
    t.extend(quad(0, 2, 3, 1)); // -z bottom
    t.extend(quad(0, 1, 5, 4)); // -y
    t.extend(quad(2, 6, 7, 3)); // +y
    t.extend(quad(1, 3, 7, 5)); // +x
    t.extend(quad(2, 0, 4, 6)); // -x
    t
}

fn axis(deg: [f64; 3]) -> [f64; 3] {
    angles_to_axis(deg).unwrap_or([0.0, 0.0, 1.0])
}

/// The pose held coming in (A -60, B 60: leaning +x and +y) and the
/// one the cut states (A +60, B 60: leaning +x and -y). Their letters
/// lerp through A 0, B 60 at mid-travel: a 60-degree lean straight
/// toward +x, where the wall stands.
const FROM: [f64; 3] = [-60.0, 60.0, 0.0];
const TO: [f64; 3] = [60.0, 60.0, 0.0];

/// A wall beside the path, reached only by the mid-travel lean: at
/// the end poses the stack passes it well off in y.
fn wall(x0: f64) -> Vec<Triangle> {
    box_facets([x0, -2.0, 0.0], [x0 + 4.0, 2.0, 30.0])
}

fn feed(from: Point3, to: Point3, pose: [f64; 3]) -> Move {
    let mut m = Move::new(MoveKind::Feed, from, to, Some(500.0));
    m.axis = Some(axis(pose));
    m
}

/// The cut along y at x 0, z 5: a stating move holds FROM coming in,
/// then the cut leans to TO while travelling.
fn leaning_cut() -> Toolpath {
    let a = Point3::new(0.0, -12.0, 5.0);
    let b = Point3::new(0.0, -10.0, 5.0);
    let c = Point3::new(0.0, 10.0, 5.0);
    Toolpath { moves: vec![feed(a, b, FROM), feed(b, c, TO)], cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), threads: Vec::new(), probes: Vec::new(), breaks: Vec::new(), plasma: None }
}

fn hits(path: &Toolpath, facets: &[Triangle]) -> Vec<Warning> {
    solid_hits(path, facets, &cutter(), &law())
}

#[test]
fn lean_through_a_wall_at_mid_travel_is_a_gouge() {
    let wall = wall(15.0);
    let path = leaning_cut();
    assert_eq!(hits(&path, &wall), vec![Warning::Gouge { at: 1 }]);
}

#[test]
fn end_pose_alone_reads_the_same_cut_clean() {
    // The reading the law held before: the cut's whole travel swept at
    // its END pose (a lone move holds no pose change - its from pose
    // IS its stated one), and the same at the FROM pose. Both clean:
    // only the lean between them meets the wall.
    let wall = wall(15.0);
    let b = Point3::new(0.0, -10.0, 5.0);
    let c = Point3::new(0.0, 10.0, 5.0);
    for pose in [TO, FROM] {
        let lone = Toolpath { moves: vec![feed(b, c, pose)], cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), threads: Vec::new(), probes: Vec::new(), breaks: Vec::new(), plasma: None };
        assert_eq!(hits(&lone, &wall), Vec::new(), "constant pose {pose:?}");
    }
}

#[test]
fn the_wall_moved_away_is_clean() {
    // Beyond the stack's reach at every lerped pose (40 * sin 60 + 4).
    let wall = wall(60.0);
    assert_eq!(hits(&leaning_cut(), &wall), Vec::new());
}

#[test]
fn constant_pose_moves_keep_their_verdict() {
    // A posed cut whose stated pose never changes: the end-pose sweep
    // is the whole law, the wall at x 15 convicts it exactly where a
    // 60-degree +x lean runs the flute in - at every move, since every
    // one of them holds that lean - and only there.
    let wall = wall(15.0);
    let lean = [0.0, 60.0, 0.0];
    let p = |y: f64| Point3::new(0.0, y, 5.0);
    let posed = Toolpath {
        moves: vec![feed(p(-12.0), p(-10.0), lean), feed(p(-10.0), p(10.0), lean), feed(p(10.0), p(12.0), lean)],
        cycles: Vec::new(),
        notes: Vec::new(),
        comps: Vec::new(),
        threads: Vec::new(),
        probes: Vec::new(),
        breaks: Vec::new(),
        plasma: None,
    };
    assert_eq!(hits(&posed, &wall), vec![Warning::Gouge { at: 1 }]);
    // And the upright program beside the wall: clean, byte for byte.
    let mut upright = posed.clone();
    for m in &mut upright.moves {
        m.axis = None;
    }
    assert_eq!(hits(&upright, &wall), Vec::new());
}

/// The posed-contact law (f2) on a ball that leaves the part between
/// two contact poses: a ball of r 3 riding a flat top at z 10, whose
/// cut lerps its lean ACROSS the upright - A -10 to A +10 at B 0 - so
/// the ball centre stands r above the top at both ends (contact) and
/// within the finish budget of it between (the tip's straight line
/// lifts the centre by r (1 - cos 10) = 0.046 mm at mid-travel; the
/// budget is 0.05). Then the same cut over a top that drops away
/// under the middle: rest at mid-travel the end pose never sees.
fn ball() -> ToolGeom {
    ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 20.0,
        shank_radius: 3.0,
        shank_len: 20.0,
        holder_radius: 8.0,
        holder_len: 20.0,
    }
}

#[test]
fn contact_claimed_along_a_leaning_cut_is_judged_at_mid_travel() {
    let r = 3.0;
    // The ball's tip rides the top plane z 10 along y; under a lean A
    // the tip is where the ball's axis-end sits, so the centre is
    // tip + r * axis - the stream's tip contract - and the skin the
    // ball contacts at the ends is the top under that centre.
    let from = [-10.0, 0.0, 0.0];
    let to = [10.0, 0.0, 0.0];
    let a0 = axis(from);
    let tip = |y: f64, a: [f64; 3]| Point3::new(0.0 - r * a[0], y - r * a[1], 10.0 + r - r * a[2]);
    let a1 = axis(to);
    let mut lead = Move::new(MoveKind::Feed, tip(-22.0, a0), tip(-20.0, a0), Some(500.0));
    lead.axis = Some(a0);
    let mut cut = Move::new(MoveKind::Feed, tip(-20.0, a0), tip(20.0, a1), Some(500.0));
    cut.axis = Some(a1);
    let path = Toolpath { moves: vec![lead, cut], cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), threads: Vec::new(), probes: Vec::new(), breaks: Vec::new(), plasma: None };
    let tol = 0.05;
    // The full top: contact everywhere, no verdict.
    let full = box_facets([-40.0, -40.0, 0.0], [40.0, 40.0, 10.0]);
    assert_eq!(posed_contact(&path, &full, &ball(), r, tol, 0.0), None);
    // The top dropped away under the middle (two blocks, a 20 mm gap
    // between them): both ends still touch, the middle stands over
    // air by the block height - rest the end-pose reading never sees.
    let mut split = box_facets([-40.0, -40.0, 0.0], [40.0, -10.0, 10.0]);
    split.extend(box_facets([-40.0, 10.0, 0.0], [40.0, 40.0, 10.0]));
    let v = posed_contact(&path, &split, &ball(), r, tol, 0.0);
    match v {
        Some(Warning::Envelope { at, rest_mm, .. }) => {
            assert_eq!(at, 1);
            assert!(rest_mm > 5.0, "rest {rest_mm}");
        }
        other => panic!("expected mid-travel rest, got {other:?}"),
    }
}
