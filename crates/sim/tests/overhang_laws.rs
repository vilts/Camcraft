//! M13.7 B: uncut stock under an overhang convicts the reach law (c),
//! its mid-turn twin (c3) and the rapid law (d). A plane section at the
//! band's or the rapid's height shows only the post under a ledge, never
//! the stock the ledge hides from a vertical tool - so a necked shank or
//! a short-flute tool's rapid sliding under the ledge through that stock
//! was silent. The same moves above the ledge, or beside a part without
//! one, stay clean.
//!
//! Provenance: our own fixtures, specced from the Prime Directive and the
//! shop rule "never traverse through uncut stock"; no vendor material.
//!
//! M13.7 cut order: a move is judged against the cuts BEFORE it - a
//! rapid through stock a later feed removes convicts, column law and
//! hidden-stock backstop alike.
//!
//! Gate map (M13.7 B, cut order): covers camcraft_sim::verify (rest,
//! cut_index, hidden, reach, sweep, rapids).

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::geometry::{Point3, Triangle, cube};
use camcraft_sim::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let l = Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ]);
    match l.and_then(|l| Region::new(vec![l])) {
        Ok(r) => r,
        Err(e) => panic!("rect: {e:?}"),
    }
}

fn mv(kind: MoveKind, from: Point3, to: Point3) -> Move {
    Move {
        rotary: None,
        kind,
        from,
        to,
        feed: if kind == MoveKind::Rapid { None } else { Some(500.0) },
        arc: None,
        axis: None,
    }
}

fn path(moves: Vec<Move>) -> Toolpath {
    Toolpath { moves, cycles: Vec::new(), notes: Vec::new(), comps: Vec::new(), threads: Vec::new(), probes: Vec::new(), breaks: Vec::new(), plasma: None }
}

/// A post X/Y [-5, 5], Z [0, 10] carrying a ledge X [-5, 15], Y [-5, 5],
/// Z [10, 12]: the ledge overhangs X (5, 15] - stock under it is hidden
/// from above. `ledge = false` is the bare post.
fn part(ledge: bool) -> Vec<Triangle> {
    let mut f: Vec<Triangle> = cube(Point3::new(-5.0, -5.0, 0.0), Point3::new(5.0, 5.0, 10.0)).facets.to_vec();
    if ledge {
        f.extend(cube(Point3::new(-5.0, -5.0, 10.0), Point3::new(15.0, 5.0, 12.0)).facets.iter().cloned());
    }
    f
}

/// Check `p` against the part with a stock box Z top 20 and `tool`.
fn judge(p: &Toolpath, facets: &[Triangle], tool: &ToolGeom) -> Vec<Warning> {
    let region = rect(-40.0, -40.0, 60.0, 60.0);
    let cx = VerifyContext {
        region: &region,
        stock: Some(&region),
        model: Some(facets),
        tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: 20.0,
        clearance_z: 30.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-3,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(p, &cx)
}

/// A short-flute tool with a NARROW neck: its 4 mm flute disc can sit
/// under the ledge while the neck stands clear of it - the only stack
/// whose body the solid law (b2) does not already convict there.
fn necked() -> ToolGeom {
    ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 4.0,
        shank_radius: 1.5,
        shank_len: 40.0,
        holder_radius: 1.5,
        holder_len: 10.0,
    }
}

/// A tool whose WIDE shank band Z [cut + 4, cut + 8] stays under the
/// ledge's underside at a floor cut; its neck-thin holder is not a band.
fn wide_shank() -> ToolGeom {
    ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 4.0,
        shank_radius: 6.0,
        shank_len: 4.0,
        holder_radius: 1.5,
        holder_len: 1.0,
    }
}

/// Floor cuts at X 17 on both sides of the ledge (the far side first,
/// reached over the clearance plane), then the gap between them bridged,
/// then a lateral rapid at `z` along X 17 from one slot to the other:
/// the disc X [14, 20] crosses the hidden band X [14, 15] under the
/// ledge. The slots alone leave Y (-5.5, 5.5) of that disc uncut to the
/// stock top - the old fixture's three column samples (start, middle,
/// end) all stood in the slots, and the gap was only silent because the
/// rapid was never marched (issue #140; #202). So the gap is bridged
/// BEFORE the rapid (M13.7 cut order): a floor pass along X 18 (disc
/// X [15, 21] - it stops at the ledge's edge, leaving the band under
/// it) and a pass above the ledge at Z 12.5 along X 17 (disc X [14, 20]
/// down to 12.5). What stands in the rapid's footprint then is exactly
/// the strip X [14, 15) under Z 12.5 - under the ledge, the hidden band.
/// Above the ledge every column is cleared to the rapid's height. The
/// judged rapid is move [`RAPID`].
fn rapid_under(z: f64) -> Toolpath {
    path(rapid_under_moves(z))
}

/// The same program with the floor bridge along X 17 instead: its disc
/// X [14, 20] clears the rapid's whole footprint down to the floor. Under
/// a ledge that is the clearing pass; beside a bare post, nothing hides
/// the strip, and this is the program that cleared it.
fn rapid_cleared(z: f64) -> Toolpath {
    path(rapid_moves(z, 17.0))
}

/// The judged lateral rapid's index in [`rapid_under`].
const RAPID: usize = 12;

fn rapid_under_moves(z: f64) -> Vec<Move> {
    rapid_moves(z, 18.0)
}

/// [`rapid_under`]'s moves with the floor bridge along X `bridge`.
fn rapid_moves(z: f64, bridge: f64) -> Vec<Move> {
    vec![
        mv(MoveKind::Feed, Point3::new(17.0, 45.0, 0.0), Point3::new(17.0, 8.5, 0.0)),
        mv(MoveKind::Rapid, Point3::new(17.0, 8.5, 0.0), Point3::new(17.0, 8.5, 30.0)),
        mv(MoveKind::Rapid, Point3::new(17.0, 8.5, 30.0), Point3::new(17.0, -25.0, 30.0)),
        mv(MoveKind::Feed, Point3::new(17.0, -25.0, 30.0), Point3::new(17.0, -25.0, 0.0)),
        mv(MoveKind::Feed, Point3::new(17.0, -25.0, 0.0), Point3::new(17.0, -8.5, 0.0)),
        // The floor bridge, then the bridge above the ledge.
        mv(MoveKind::Feed, Point3::new(17.0, -8.5, 0.0), Point3::new(bridge, -8.5, 0.0)),
        mv(MoveKind::Feed, Point3::new(bridge, -8.5, 0.0), Point3::new(bridge, 8.5, 0.0)),
        mv(MoveKind::Feed, Point3::new(bridge, 8.5, 0.0), Point3::new(bridge, 8.5, 12.5)),
        mv(MoveKind::Feed, Point3::new(bridge, 8.5, 12.5), Point3::new(17.0, 8.5, 12.5)),
        mv(MoveKind::Feed, Point3::new(17.0, 8.5, 12.5), Point3::new(17.0, -8.5, 12.5)),
        mv(MoveKind::Feed, Point3::new(17.0, -8.5, 12.5), Point3::new(17.0, -8.5, 0.0)),
        mv(MoveKind::Rapid, Point3::new(17.0, -8.5, 0.0), Point3::new(17.0, -8.5, z)),
        mv(MoveKind::Rapid, Point3::new(17.0, -8.5, z), Point3::new(17.0, 45.0, z)),
        mv(MoveKind::Rapid, Point3::new(17.0, 45.0, z), Point3::new(17.0, 45.0, 0.0)),
    ]
}

/// A floor cut at height `z` along X 19 up to Y 0: the shank annulus
/// (3, 6] about its end reaches X 13 - under the ledge at a floor cut.
fn shank_under(z: f64, x: f64) -> Toolpath {
    path(vec![mv(MoveKind::Feed, Point3::new(x, -25.0, z), Point3::new(x, 0.0, z))])
}

#[test]
fn a_rapid_under_the_ledge_through_hidden_stock_is_a_rapid_in_stock() {
    let w = judge(&rapid_under(5.0), &part(true), &necked());
    assert_eq!(w, vec![Warning::RapidInStock { at: RAPID }], "{w:?}");
}

#[test]
fn the_same_rapid_above_the_ledge_is_clean() {
    let w = judge(&rapid_under(12.5), &part(true), &necked());
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn the_same_rapid_beside_a_post_without_a_ledge_is_clean() {
    // Beside a bare post nothing hides the strip X [14, 15): the program
    // that bridges the gap along X 17 has cleared it, and the rapid is
    // clean. (`rapid_under` beside the bare post still convicts - the
    // strip is uncut stock there too, open from above, never cut.)
    let w = judge(&rapid_cleared(5.0), &part(false), &necked());
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_shank_under_the_ledge_in_hidden_stock_is_a_shank_hit() {
    let w = judge(&shank_under(0.0, 19.0), &part(true), &wide_shank());
    assert_eq!(w, vec![Warning::ShankHit { at: 0, part: false }], "{w:?}");
}

#[test]
fn the_same_shank_above_the_ledge_is_clean() {
    let w = judge(&shank_under(12.5, 19.0), &part(true), &wide_shank());
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_shank_whose_annulus_misses_the_hidden_stock_is_clean() {
    // X 22: the annulus reaches X 16, past the ledge's edge at X 15.
    let w = judge(&shank_under(0.0, 22.0), &part(true), &wide_shank());
    assert!(w.is_empty(), "{w:?}");
    // And the bare post never hides anything.
    let w = judge(&shank_under(0.0, 19.0), &part(false), &wide_shank());
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_2d_path_without_a_model_is_unchanged() {
    // No model: no shadow to read, the column law alone speaks - and it
    // finds the column the floor bridge cleared.
    let region = rect(-40.0, -40.0, 60.0, 60.0);
    let tool = necked();
    let cx = VerifyContext {
        region: &region,
        stock: Some(&region),
        model: None,
        tool: &tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: 20.0,
        clearance_z: 30.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-3,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    let w = verify(&rapid_cleared(5.0), &cx);
    assert!(w.is_empty(), "{w:?}");
}

// --- M13.7 cut order: a move is judged against the cuts BEFORE it ---

/// The no-model context of [`a_2d_path_without_a_model_is_unchanged`].
fn judge_2d(p: &Toolpath, tool: &ToolGeom) -> Vec<Warning> {
    let region = rect(-40.0, -40.0, 60.0, 60.0);
    let cx = VerifyContext {
        region: &region,
        stock: Some(&region),
        model: None,
        tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: 20.0,
        clearance_z: 30.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-3,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(p, &cx)
}

#[test]
fn a_rapid_through_stock_a_later_feed_removes_is_a_rapid_in_stock() {
    // The rapid at Z 5 slides through full stock; the feed that clears
    // its columns down to Z 5 comes after it. An order-blind rest model
    // read those columns as cleared and stayed silent.
    let p = path(vec![
        mv(MoveKind::Rapid, Point3::new(0.0, 0.0, 5.0), Point3::new(20.0, 0.0, 5.0)),
        mv(MoveKind::Feed, Point3::new(20.0, 0.0, 5.0), Point3::new(0.0, 0.0, 5.0)),
    ]);
    let w = judge_2d(&p, &necked());
    assert_eq!(w, vec![Warning::RapidInStock { at: 0 }], "{w:?}");
}

#[test]
fn the_same_rapid_after_the_feed_is_clean() {
    let p = path(vec![
        mv(MoveKind::Feed, Point3::new(0.0, 0.0, 5.0), Point3::new(20.0, 0.0, 5.0)),
        mv(MoveKind::Rapid, Point3::new(20.0, 0.0, 5.0), Point3::new(0.0, 0.0, 5.0)),
    ]);
    let w = judge_2d(&p, &necked());
    assert!(w.is_empty(), "{w:?}");
}

/// The floor pass along X 17 the whole way under the ledge: its disc
/// X [14, 20] clears the hidden band X [14, 15] the judged rapid crosses.
fn clearing_pass(from_y: f64, to_y: f64) -> Move {
    mv(MoveKind::Feed, Point3::new(17.0, from_y, 0.0), Point3::new(17.0, to_y, 0.0))
}

#[test]
fn a_rapid_under_the_ledge_before_the_clearing_pass_is_a_rapid_in_stock() {
    // The pass comes after the rapid: the hidden stock still stands when
    // the rapid slides through it - the backstop's own cut-order clause.
    let mut moves = rapid_under_moves(5.0);
    moves.push(clearing_pass(45.0, -25.0));
    let w = judge(&path(moves), &part(true), &necked());
    assert_eq!(w, vec![Warning::RapidInStock { at: RAPID }], "{w:?}");
}

#[test]
fn the_same_rapid_after_the_clearing_pass_is_clean() {
    let mut moves = vec![clearing_pass(-25.0, 45.0)];
    moves.extend(rapid_under_moves(5.0));
    let w = judge(&path(moves), &part(true), &necked());
    assert!(w.is_empty(), "{w:?}");
}

/// A ball end mill r6 on a long flute - the tip whose rim the march
/// lifts (#106) while the hidden-stock backstop reads the tool disc at
/// the tip plane.
fn ball6() -> ToolGeom {
    ToolGeom {
        radius: 6.0,
        nose_radius: 6.0,
        flute_len: 25.0,
        shank_radius: 6.0,
        shank_len: 20.0,
        holder_radius: 6.0,
        holder_len: 10.0,
    }
}

/// [`judge`] with the judged cutter stated as the ball itself.
fn judge_ball(p: &Toolpath, facets: &[Triangle]) -> Vec<Warning> {
    let tool = ball6();
    let region = rect(-40.0, -40.0, 60.0, 60.0);
    let cutter = camcraft_sim::verify::Cutter {
        radius: 6.0,
        corner_r: 6.0,
        tip_angle: 0.0,
        flute_len: tool.flute_len,
        shank_radius: tool.shank_radius,
        shank_len: tool.shank_len,
        holder_radius: tool.holder_radius,
        holder_len: tool.holder_len,
        body: None,
        neck: None,
    };
    let cx = VerifyContext {
        region: &region,
        stock: Some(&region),
        model: Some(facets),
        tool: &tool,
        cutter: Some(cutter),
        mode: Mode::Area,
        stock_top_z: 20.0,
        clearance_z: 30.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-3,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(p, &cx)
}

/// The judged lateral rapid's index in [`ball_rim_over_the_ledge`].
const BALL_RAPID: usize = 6;

/// Issue #206's program for [`ball6`]: a floor pass along X 21 (disc
/// X [15, 27] - it stops at the ledge's edge), a pass at Z 12 along X 15
/// (the ledge top: columns X [9, 21] cleared down to 12, nothing under
/// the ledge cleared - a chord clears the hidden stock only from its own
/// plane up), then a lateral rapid at Z 9.5 along X 20.5. Its footprint
/// reaches X 14.51; only the march's rim ring (radius 5.99, judged at
/// the 7th ring's lift 3.08) stands over X < 15.26, where the columns
/// read 12 - and 9.5 + 3.08 clears them, so the march is silent. The
/// tip plane Z 9.5 under the ledge (X (5, 15], Z [10, 12]) still holds
/// the uncut hidden band X [14.51, 15): only `hidden_along` sees it.
fn ball_rim_over_the_ledge() -> Toolpath {
    path(vec![
        mv(MoveKind::Feed, Point3::new(21.0, -40.0, 0.0), Point3::new(21.0, 60.0, 0.0)),
        mv(MoveKind::Rapid, Point3::new(21.0, 60.0, 0.0), Point3::new(21.0, 60.0, 30.0)),
        mv(MoveKind::Rapid, Point3::new(21.0, 60.0, 30.0), Point3::new(15.0, 60.0, 30.0)),
        mv(MoveKind::Feed, Point3::new(15.0, 60.0, 30.0), Point3::new(15.0, 60.0, 12.0)),
        mv(MoveKind::Feed, Point3::new(15.0, 60.0, 12.0), Point3::new(15.0, -40.0, 12.0)),
        mv(MoveKind::Feed, Point3::new(15.0, -40.0, 12.0), Point3::new(20.5, -40.0, 9.5)),
        mv(MoveKind::Rapid, Point3::new(20.5, -40.0, 9.5), Point3::new(20.5, 60.0, 9.5)),
    ])
}

#[test]
fn a_ball_rim_the_march_lifts_clear_is_still_convicted_through_hidden_stock() {
    // Issue #206: since #140 the march subsumes the hidden-stock arm for a
    // flat tool on a straight lateral rapid; this ball rapid is the case
    // only `Rest::hidden_along` convicts - deleting that call in
    // verify/rapids.rs turns this test red. (The arm reads the disc at
    // the tip plane - conservative for a ball, whose rim stands higher;
    // it can only over-report, which is the backstop's stated posture.)
    let w = judge_ball(&ball_rim_over_the_ledge(), &part(true));
    assert!(w.contains(&Warning::RapidInStock { at: BALL_RAPID }), "{w:?}");
}

#[test]
fn the_same_ball_rapid_beside_a_bare_post_is_clean() {
    // No ledge, no hidden stock: the same moves leave the march as the
    // only reader, and it acquits the lifted rim - so the conviction above
    // is the hidden-stock arm's alone.
    let w = judge_ball(&ball_rim_over_the_ledge(), &part(false));
    assert!(!w.iter().any(|w| matches!(w, Warning::RapidInStock { .. })), "{w:?}");
}
