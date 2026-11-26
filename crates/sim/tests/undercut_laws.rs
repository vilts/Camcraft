//! #1444: the undercut cutters in the verifier and the stock simulation.
//! A T-slot, keyseat, dovetail or lollipop head is wider than its neck
//! and cuts sideways under a roof. The laws pinned here: a Z move with
//! the head in or under material is refused (a retract inside the
//! undercut, a dive into stock); the neck against a slot mouth too
//! narrow for it is a collision, and a slot the program ahead cut wide
//! enough is air; the solid law judges a dovetail's frustum and a
//! lollipop's ball, not a cylinder; and the field stock removes the
//! undercut volume with the true head and neck, leaving the roof.
//! Provenance: plane and solid geometry of the handbook cutter side
//! views; our own code, per the Clean-Room Protocol.
//!
//! Gate map: covers camcraft_sim::verify::undercut, the undercut heads
//! of camcraft_sim::verify::{probe, swept} and
//! camcraft_sim::stock_sim::{field_undercut, field_solid}.

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::geometry::{cube, Point3, Triangle};
use camcraft_sim::stock_sim::{Backend, Columns, Dexel, Stock, FIELD_BAND};
use camcraft_sim::verify::{acquit_undercut_by_prior, solid_hits, verify, Cutter, Mode, SolidLaw, VerifyContext, Warning};
use camcraft_tools::tools::{Tool, ToolKind};
use camcraft_tools::undercut::{cutter, Undercut};

const TOP: f64 = 0.0;
const FLOOR: f64 = -12.0;
const CLEAR: f64 = 10.0;

fn tslot() -> Tool {
    cutter(ToolKind::TSlot, 20.0, 6.0, 8.0, 30.0, 0.0)
}

fn dovetail() -> Tool {
    cutter(ToolKind::Dovetail, 20.0, 5.0, 8.0, 30.0, 60.0)
}

fn lollipop() -> Tool {
    cutter(ToolKind::Lollipop, 10.0, 5.0, 4.0, 30.0, 0.0)
}

fn mv(kind: MoveKind, a: (f64, f64, f64), b: (f64, f64, f64)) -> Move {
    Move {
        rotary: None,
        kind,
        from: Point3::new(a.0, a.1, a.2),
        to: Point3::new(b.0, b.1, b.2),
        feed: if kind == MoveKind::Rapid { None } else { Some(80.0) },
        arc: None,
        axis: None,
    }
}

/// The stock: x 0..60, y -20..20, top Z 0.
fn stock() -> Region {
    let l = Loop::new(vec![
        Point2::new(0.0, -20.0),
        Point2::new(60.0, -20.0),
        Point2::new(60.0, 20.0),
        Point2::new(0.0, 20.0),
    ])
    .unwrap();
    Region::new(vec![l]).unwrap()
}

/// In from the station at x -15 (outside the wall), along y 0 to x
/// `reach`, and - `out` - back to the station before the lift; else the
/// lift happens where the cutter stands, inside the undercut.
fn pass(reach: f64, out: bool) -> Toolpath {
    let s = (-15.0, 0.0);
    let mut m = vec![
        mv(MoveKind::Rapid, (0.0, 0.0, CLEAR), (s.0, s.1, CLEAR)),
        mv(MoveKind::Plunge, (s.0, s.1, CLEAR), (s.0, s.1, FLOOR)),
        mv(MoveKind::Feed, (s.0, s.1, FLOOR), (reach, 0.0, FLOOR)),
    ];
    if out {
        m.push(mv(MoveKind::Feed, (reach, 0.0, FLOOR), (s.0, s.1, FLOOR)));
        m.push(mv(MoveKind::Rapid, (s.0, s.1, FLOOR), (s.0, s.1, CLEAR)));
    } else {
        m.push(mv(MoveKind::Rapid, (reach, 0.0, FLOOR), (reach, 0.0, CLEAR)));
    }
    Toolpath::new(m)
}

fn judge(path: &Toolpath, tool: &Tool) -> Vec<Warning> {
    judge_on(path, tool, &stock())
}

fn judge_on(path: &Toolpath, tool: &Tool, region: &Region) -> Vec<Warning> {
    let geom = tool.geom();
    let cx = VerifyContext {
        region,
        stock: Some(region),
        model: None,
        tool: &geom,
        cutter: Some(Cutter::of(tool)),
        mode: Mode::Undercut,
        stock_top_z: TOP,
        clearance_z: CLEAR,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-3,
        ..Default::default()
    };
    verify(path, &cx)
}

/// The stock as the program ahead left it: a slot `width` wide along
/// y 0 over the whole length, cut down to Z `depth`.
fn slotted(width: f64, depth: f64) -> Dexel {
    let mut d = Dexel::block(0.0, -20.0, 60.0, 20.0, TOP, 0.25).unwrap();
    for iy in 0..d.ny {
        for ix in 0..d.nx {
            let c = d.center(ix, iy);
            if c.y.abs() < width * 0.5
                && let Some(k) = d.col(c)
            {
                d.lower(k, depth);
            }
        }
    }
    d
}

fn acquitted(path: &Toolpath, tool: &Tool, prior: &Dexel) -> Vec<Warning> {
    let mut w = judge(path, tool);
    acquit_undercut_by_prior(path, &Cutter::of(tool), &Columns::of_grid(prior), 1e-3, 0.01, &mut w);
    w
}

#[test]
fn a_retract_inside_the_undercut_is_refused() {
    // The neck slot is there (9 mm for the 8 mm neck, down to the head's
    // top at Z -6), so only the lift is wrong: the head stands under the
    // roof at x 30 and the rapid drives it up through it.
    let path = pass(30.0, false);
    let w = acquitted(&path, &tslot(), &slotted(9.0, -6.0));
    assert_eq!(w, vec![Warning::UndercutLift { at: 3 }], "{w:?}");
    assert!(w[0].blocks_post());
    assert!(w[0].to_string().contains("moves the undercut cutter in Z"), "{}", w[0]);
}

#[test]
fn the_same_pass_leaving_the_way_it_came_is_clean() {
    let w = acquitted(&pass(30.0, true), &tslot(), &slotted(9.0, -6.0));
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn a_dive_into_uncut_stock_is_refused() {
    // Down at x 30, in the middle of the block: no side-cutting head plunges.
    let path = Toolpath::new(vec![
        mv(MoveKind::Rapid, (0.0, 0.0, CLEAR), (30.0, 0.0, CLEAR)),
        mv(MoveKind::Plunge, (30.0, 0.0, CLEAR), (30.0, 0.0, FLOOR)),
    ]);
    let w = acquitted(&path, &tslot(), &slotted(9.0, -6.0));
    assert!(w.contains(&Warning::UndercutLift { at: 1 }), "{w:?}");
}

#[test]
fn a_stock_corner_inside_the_head_between_samples_blocks_the_plunge_and_the_retract() {
    // The 20 mm head stands at (0, 0). The stock corner (9.7, 0.8) is
    // 9.733 mm from the axis: 0.267 mm inside the head's rim, but between
    // any ring samples. The 8 mm neck does not reach this stock. The
    // path goes away from the stock and comes back to the same station.
    let corner = |lo_y: f64| {
        let l = Loop::new(vec![
            Point2::new(9.7, lo_y),
            Point2::new(20.0, lo_y),
            Point2::new(20.0, 20.0),
            Point2::new(9.7, 20.0),
        ])
        .unwrap();
        Region::new(vec![l]).unwrap()
    };
    let s = (0.0, 0.0);
    let path = Toolpath::new(vec![
        mv(MoveKind::Rapid, (-75.0, 0.0, CLEAR), (s.0, s.1, CLEAR)),
        mv(MoveKind::Plunge, (s.0, s.1, CLEAR), (s.0, s.1, FLOOR)),
        mv(MoveKind::Feed, (s.0, s.1, FLOOR), (-15.0, 0.0, FLOOR)),
        mv(MoveKind::Feed, (-15.0, 0.0, FLOOR), (-75.0, 0.0, FLOOR)),
        mv(MoveKind::Feed, (-75.0, 0.0, FLOOR), (-15.0, 0.0, FLOOR)),
        mv(MoveKind::Feed, (-15.0, 0.0, FLOOR), (s.0, s.1, FLOOR)),
        mv(MoveKind::Rapid, (s.0, s.1, FLOOR), (s.0, s.1, CLEAR)),
    ]);
    let lifts = vec![Warning::UndercutLift { at: 1 }, Warning::UndercutLift { at: 6 }];
    assert_eq!(judge_on(&path, &tslot(), &corner(0.8)), lifts, "the corner inside the head");
    assert_eq!(judge_on(&path, &tslot(), &corner(0.0)), lifts, "the control: an edge inside the head");
    // Move the corner 0.69 mm outside the rim: the same path is clean.
    assert!(judge_on(&path, &tslot(), &corner(4.5)).is_empty(), "the corner outside the head");
}

#[test]
fn the_neck_through_uncut_stock_is_a_collision() {
    // No slot was cut for the neck: the raw block stands over its path.
    let path = pass(30.0, true);
    let raw = Dexel::block(0.0, -20.0, 60.0, 20.0, TOP, 0.25).unwrap();
    let w = acquitted(&path, &tslot(), &raw);
    assert_eq!(w, vec![Warning::ShankHit { at: 2, part: false }, Warning::ShankHit { at: 3, part: false }], "{w:?}");
    // The verifier alone says the same: with no program ahead the raw
    // stock is all there is.
    assert_eq!(judge(&path, &tslot()), w);
}

#[test]
fn a_slot_mouth_narrower_than_the_neck_is_a_collision() {
    // 7 mm for an 8 mm neck.
    let w = acquitted(&pass(30.0, true), &tslot(), &slotted(7.0, -6.0));
    assert!(w.contains(&Warning::ShankHit { at: 2, part: false }), "{w:?}");
    // And a slot wide enough but too shallow leaves material over the head's top.
    let w = acquitted(&pass(30.0, true), &tslot(), &slotted(9.0, -4.0));
    assert!(w.contains(&Warning::ShankHit { at: 2, part: false }), "{w:?}");
}

#[test]
fn an_unknown_prior_height_keeps_the_conviction() {
    let mut prior = slotted(9.0, -6.0);
    let k = prior.col(Point2::new(20.0, 0.0)).unwrap();
    prior.set(k, f64::NAN);
    let w = acquitted(&pass(30.0, true), &tslot(), &prior);
    assert!(w.contains(&Warning::ShankHit { at: 2, part: false }), "{w:?}");
}

/// A feed along y at `x`, tip on Z 0, past a block hanging over
/// x 0..10 from Z `under` up.
fn graze(x: f64, under: f64, cutter: &Cutter) -> Vec<Warning> {
    let block: Vec<Triangle> = cube(Point3::new(0.0, -5.0, under), Point3::new(10.0, 5.0, under + 5.0)).facets.to_vec();
    let path = Toolpath::new(vec![mv(MoveKind::Feed, (x, -20.0, 0.0), (x, 20.0, 0.0))]);
    solid_hits(&path, &block, cutter, &SolidLaw { step: 0.25, allow: 0.01, hole_walls: false })
}

#[test]
fn the_solid_law_reads_a_dovetail_as_a_frustum() {
    // 60 degrees: at Z 3 the 10 mm head has narrowed to 10 - 3 / tan 60 = 8.27 mm.
    let t = dovetail();
    let real = Cutter::of(&t);
    assert_eq!(real.undercut, Undercut::of(&t));
    assert!(graze(-8.5, 3.0, &real).is_empty(), "the flank clears the block");
    assert_eq!(graze(-8.0, 3.0, &real), vec![Warning::Gouge { at: 0 }], "0.27 mm inside the flank");
    // The square stack would convict the clean pass: the profile matters.
    assert_eq!(graze(-8.5, 3.0, &Cutter::flat(&t.geom())), vec![Warning::Gouge { at: 0 }]);
}

#[test]
fn the_solid_law_reads_a_lollipop_as_a_whole_ball() {
    // The block hangs from Z 8, above the neck's start at Z 5: the 5 mm
    // ball is 4 mm wide there, and a cylinder as tall as the flute
    // length would not reach it at all.
    let t = lollipop();
    let real = Cutter::of(&t);
    assert!(graze(-4.5, 8.0, &real).is_empty(), "the ball clears the block");
    assert_eq!(graze(-3.5, 8.0, &real), vec![Warning::Gouge { at: 0 }], "half a mm inside the ball");
    // Beside the ball the 2 mm neck is all that stands at Z 8.
    assert!(graze(-2.5, 9.9, &real).is_empty());
}

/// A feed along y at `x`, tip on Z 0, past a wall standing over
/// x 0..10 from below the tip to above the head.
fn pass_wall(x: f64, cutter: &Cutter) -> Vec<Warning> {
    let wall: Vec<Triangle> = cube(Point3::new(0.0, -5.0, -5.0), Point3::new(10.0, 5.0, 20.0)).facets.to_vec();
    let path = Toolpath::new(vec![mv(MoveKind::Feed, (x, -20.0, 0.0), (x, 20.0, 0.0))]);
    solid_hits(&path, &wall, cutter, &SolidLaw { step: 0.25, allow: 0.01, hole_walls: false })
}

#[test]
fn a_wall_is_judged_at_the_heads_widest_height_not_at_its_top() {
    // The wall's face spans the whole head, so only the profile's own
    // widest radius can convict: the dovetail's is at its tip (10 mm; its
    // top is 7.11), the ball's at its equator (5 mm; its top is 0).
    let dove = Cutter::of(&dovetail());
    assert_eq!(pass_wall(-9.0, &dove), vec![Warning::Gouge { at: 0 }], "1 mm inside the tip corner");
    assert!(pass_wall(-10.5, &dove).is_empty());
    let ball = Cutter::of(&lollipop());
    assert_eq!(pass_wall(-4.5, &ball), vec![Warning::Gouge { at: 0 }], "half a mm inside the equator");
    assert!(pass_wall(-5.5, &ball).is_empty());
}

#[test]
fn a_head_standing_under_its_roof_is_contact_and_one_lifted_into_it_is_a_gouge() {
    // The block hangs from Z 6: exactly the T-slot head's top face.
    let real = Cutter::of(&tslot());
    let roof: Vec<Triangle> = cube(Point3::new(0.0, -5.0, 6.0), Point3::new(10.0, 5.0, 11.0)).facets.to_vec();
    let law = SolidLaw { step: 0.25, allow: 0.01, hole_walls: false };
    let under = |z: f64| Toolpath::new(vec![mv(MoveKind::Feed, (-20.0, 0.0, z), (-9.0, 0.0, z))]);
    assert!(solid_hits(&under(0.0), &roof, &real, &law).is_empty(), "the head's top touches the roof");
    assert_eq!(solid_hits(&under(0.1), &roof, &real, &law), vec![Warning::Gouge { at: 0 }], "0.1 mm into the roof");
    // An end mill of the same numbers keeps the strict reading of its top.
    let plain = Cutter::flat(&tslot().geom());
    assert_eq!(solid_hits(&under(0.0), &roof, &plain, &law), vec![Warning::Gouge { at: 0 }]);
}

const FIELD: Backend = Backend::Field { band: FIELD_BAND };

/// A block x 0..60, y -20..20, top Z 0, cut by `tool` along y 0 at tip
/// Z -12 from outside the wall to x 40.
fn cut_field(tool: &Tool) -> Stock {
    let mut s = Stock::block(FIELD, 0.0, -20.0, 60.0, 20.0, TOP, 0.5).unwrap();
    let u = Undercut::of(tool).unwrap();
    s.simulate_undercut(&pass(40.0, true).moves, &u, tool.shank_len).unwrap();
    s
}

fn air(s: &Stock, x: f64, y: f64, z: f64) -> bool {
    s.distance(Point3::new(x, y, z)) > 0.0
}

#[test]
fn the_field_removes_the_undercut_and_keeps_the_roof() {
    let s = cut_field(&tslot());
    // Inside the head's sweep: 20 wide, Z -12..-6.
    assert!(air(&s, 20.0, 0.0, -9.0) && air(&s, 20.0, 8.5, -9.0) && air(&s, 20.0, -8.5, -11.0));
    // The neck's kerf: 8 wide above the head.
    assert!(air(&s, 20.0, 0.0, -3.0) && air(&s, 20.0, 3.0, -3.0));
    // The roof beside the neck stands, and so do the wall and the floor.
    assert!(!air(&s, 20.0, 7.0, -3.0), "the roof over the head stays");
    assert!(!air(&s, 20.0, -7.0, -4.5));
    assert!(!air(&s, 20.0, 11.5, -9.0), "the wall past the head stays");
    assert!(!air(&s, 20.0, 0.0, -13.5), "the floor under the head stays");
    // Past the end of the pass nothing is cut.
    assert!(!air(&s, 55.0, 0.0, -9.0));
}

#[test]
fn the_field_cuts_a_dovetail_wall_on_its_slope() {
    let s = cut_field(&dovetail());
    // Head Z -12..-7, 10 mm at the floor, 7.11 mm at its top.
    assert!(air(&s, 20.0, 9.0, -11.5), "under the flank at the floor");
    assert!(!air(&s, 20.0, 9.0, -8.0), "the flank has leaned in to 7.7 mm here");
    assert!(air(&s, 20.0, 6.5, -8.0));
}

#[test]
fn the_field_cuts_a_lollipop_groove_round() {
    let s = cut_field(&lollipop());
    // The 5 mm ball's centre rides Z -7.
    assert!(air(&s, 20.0, 4.0, -7.0) && air(&s, 20.0, 0.0, -11.0));
    assert!(!air(&s, 20.0, 4.0, -10.6), "outside the ball below its equator");
    assert!(!air(&s, 20.0, 4.0, -3.4), "outside the ball above its equator");
    assert!(air(&s, 20.0, 0.0, -1.0), "the neck's kerf");
}

#[test]
fn the_column_arm_cuts_nothing_and_bad_input_is_refused_by_name() {
    let mut c = Stock::block(Backend::Column, 0.0, -20.0, 60.0, 20.0, TOP, 0.5).unwrap();
    let u = Undercut::of(&tslot()).unwrap();
    c.simulate_undercut(&pass(40.0, true).moves, &u, 30.0).unwrap();
    assert_eq!(c.top(20.0, 0.0), TOP, "a column cannot hold air under a roof: it is left as it stood");
    let e = c.simulate_undercut(&[], &Undercut { neck_r: 12.0, ..u }, 30.0).unwrap_err();
    assert!(format!("{e:?}").contains("a neck under its head"), "{e:?}");
    let mut tilted = pass(40.0, true).moves;
    tilted[2].axis = Some([0.0, 0.6, 0.8]);
    let mut f = Stock::block(FIELD, 0.0, -20.0, 60.0, 20.0, TOP, 0.5).unwrap();
    let e = f.simulate_undercut(&tilted, &u, 30.0).unwrap_err();
    assert!(format!("{e:?}").contains("posed moves"), "{e:?}");
}
