//! #3326: the Face raster reads the start and exit picks - the start pick's
//! nearest corner sets the first row and the step direction, the exit pick
//! names the side where the last row ends (refused by name when the row
//! count cannot reach it), the rows themselves never change, and no pick
//! is `face_over_kept` to the byte.
//!
//! Gate map (#3326): covers camcraft_motion::machining2d (face, face_raster).

use camcraft_motion::cl::{text::to_text, MoveKind, Toolpath};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{face_over, face_picked, FaceHoles, FaceKeep, FaceParams, GenError};
use camcraft_motion::path_picks::{PathPick, PathPicks};

const CUT_Z: f64 = -2.0;

fn rect(c: (f64, f64), hx: f64, hy: f64) -> Loop {
    Loop::new(vec![
        Point2::new(c.0 - hx, c.1 - hy),
        Point2::new(c.0 + hx, c.1 - hy),
        Point2::new(c.0 + hx, c.1 + hy),
        Point2::new(c.0 - hx, c.1 + hy),
    ])
    .unwrap()
}

/// A 40 x 20 plate centred on the origin: rows at Y -7 .. 7, X ends at -23 and 23.
fn plate() -> Region {
    Region::new(vec![rect((0.0, 0.0), 20.0, 10.0)]).unwrap()
}

/// Six rows at stepover 3.0, five at 3.5.
fn p(stepover: f64) -> FaceParams {
    FaceParams {
        tool_diameter: 6.0,
        stepover,
        clearance_z: 5.0,
        cut_z: CUT_Z,
        top_z: None,
        stepdown: None,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        blend_turns: false,
        blend_radius: None,
        holes: FaceHoles::default(),
        stock_top: None,
    }
}

fn picks(start: Option<(f64, f64)>, exit: Option<(f64, f64)>) -> PathPicks {
    let of = |q: Option<(f64, f64)>| q.map(|(x, y)| PathPick::xy(Point2::new(x, y))).into_iter().collect();
    PathPicks { start: of(start), exit: of(exit) }
}

fn run(region: &Region, p: &FaceParams, k: &PathPicks) -> Result<Toolpath, GenError> {
    face_picked(region, p, 0.0, None, None, k)
}

/// Where the first plunge lands.
fn entry(t: &Toolpath) -> (f64, f64) {
    let m = t.moves.iter().find(|m| m.kind == MoveKind::Plunge).expect("a plunge");
    (m.to.x, m.to.y)
}

/// Where the last move on the cut plane ends.
fn leave(t: &Toolpath) -> (f64, f64) {
    let m = t.moves.iter().rev().find(|m| m.kind == MoveKind::Feed).expect("a feed");
    (m.to.x, m.to.y)
}

/// The straight row pieces, each as (y, low x, high x) in microns, sorted:
/// what the raster cuts, whatever its order and direction.
fn row_pieces(t: &Toolpath) -> Vec<(i64, i64, i64)> {
    let q = |v: f64| (v * 1000.0).round() as i64;
    let mut out: Vec<_> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.arc.is_none() && m.from.y == m.to.y && m.from.z == m.to.z)
        .map(|m| (q(m.to.y), q(m.from.x.min(m.to.x)), q(m.from.x.max(m.to.x))))
        .collect();
    out.sort_unstable();
    out
}

fn golden(name: &str, path: &Toolpath) {
    let text = to_text(path).expect("the generated path has a valid CL text form");
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name);
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(&file, text).expect("cannot write the golden CL file");
        return;
    }
    let want = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("cannot read golden {}: {e}", file.display()));
    assert_eq!(want.replace("\r\n", "\n"), text, "golden {} is stale", file.display());
}

fn refusal(r: Result<Toolpath, GenError>) -> String {
    match r {
        Ok(t) => panic!("refused by name, not cut: entry {:?}, leaves {:?}", entry(&t), leave(&t)),
        Err(e) => e.to_string(),
    }
}

#[test]
fn no_pick_is_the_automatic_face_to_the_byte() {
    let hole = Region::new(vec![rect((0.0, 0.0), 20.0, 10.0), rect((0.0, 0.0), 4.0, 3.0)]).unwrap();
    for region in [plate(), hole] {
        for (climb, blend, staged) in [(true, false, false), (false, false, false), (true, true, true), (false, true, false)] {
            let mut fp = p(3.0);
            fp.climb = climb;
            fp.blend_turns = blend;
            if staged {
                fp.top_z = Some(0.0);
                fp.stepdown = Some(1.0);
            }
            let auto = face_over(&region, &fp, 0.0, None).expect("the automatic face");
            let same = run(&region, &fp, &PathPicks::default()).expect("no pick");
            assert_eq!(auto, same, "climb {climb}, blend {blend}, staged {staged}");
        }
    }
}

/// One corner: the pick beside it, the entry, the second row's Y, and
/// where the six-row raster leaves.
fn corner(name: &str, pick: (f64, f64), at: (f64, f64), next_y: f64, out: (f64, f64)) {
    let t = run(&plate(), &p(3.0), &picks(Some(pick), None)).expect(name);
    assert_eq!(entry(&t), at, "{name}: the raster enters at the picked corner");
    let second = t.moves.iter().find(|m| m.kind == MoveKind::Feed && m.to.y != at.1).expect("a step");
    assert_eq!(second.to.y, next_y, "{name}: the step goes away from the picked corner");
    assert_eq!(leave(&t), out, "{name}: six rows end on the entry's side, at the far edge");
    let auto = face_over(&plate(), &p(3.0), 0.0, None).expect("the automatic face");
    assert_eq!(row_pieces(&t), row_pieces(&auto), "{name}: the rows never change");
    // Climb does not move a picked corner.
    let mut conv = p(3.0);
    conv.climb = false;
    let c = run(&plate(), &conv, &picks(Some(pick), None)).expect(name);
    assert_eq!(c, t, "{name}: the pick, not the climb switch, sets the first row");
    golden(&format!("face_start_{name}.cl"), &t);
}

#[test]
fn start_at_the_low_x_low_y_corner_matches_golden() {
    corner("low_x_low_y", (-19.0, -9.5), (-23.0, -7.0), -4.0, (-23.0, 7.0));
    // That corner is the climb default's own: the pick changes nothing.
    let t = run(&plate(), &p(3.0), &picks(Some((-19.0, -9.5)), None)).expect("picked");
    assert_eq!(t, face_over(&plate(), &p(3.0), 0.0, None).expect("auto"));
}

#[test]
fn start_at_the_high_x_low_y_corner_matches_golden() {
    corner("high_x_low_y", (19.0, -9.5), (23.0, -7.0), -4.0, (23.0, 7.0));
}

#[test]
fn start_at_the_low_x_high_y_corner_matches_golden() {
    corner("low_x_high_y", (-19.0, 9.5), (-23.0, 7.0), 5.0, (-23.0, -7.0));
}

#[test]
fn start_at_the_high_x_high_y_corner_matches_golden() {
    corner("high_x_high_y", (19.0, 9.5), (23.0, 7.0), 5.0, (23.0, -7.0));
}

#[test]
fn an_exit_on_the_side_the_last_row_reaches_is_taken() {
    // Six rows from the low corner end on the entry's side; five end across.
    let start = Some((-19.0, -9.5));
    let six = run(&plate(), &p(3.0), &picks(start, Some((-19.0, 9.5)))).expect("six rows end at low X");
    assert_eq!(leave(&six), (-23.0, 7.0));
    assert_eq!(six.moves, run(&plate(), &p(3.0), &picks(start, None)).expect("start only").moves);
    let five = run(&plate(), &p(3.5), &picks(start, Some((19.0, 9.5)))).expect("five rows end at high X");
    assert_eq!(leave(&five), (23.0, 7.0));
}

#[test]
fn an_exit_the_row_count_cannot_reach_is_refused_by_name() {
    let start = Some((-19.0, -9.5));
    for (stepover, exit, rows) in [(3.0, (19.0, 9.5), "6 rows"), (3.5, (-19.0, 9.5), "5 rows")] {
        let why = refusal(run(&plate(), &p(stepover), &picks(start, Some(exit))));
        assert!(why.contains("exit point"), "names the pick: {why}");
        assert!(why.contains(rows), "names the row count: {why}");
        assert!(why.contains("start"), "names the start corner as the cause: {why}");
    }
}

#[test]
fn an_exit_alone_ends_the_last_row_at_its_corner() {
    for stepover in [3.0, 3.5] {
        for (exit, out) in [
            ((-19.0, -9.5), (-23.0, -7.0)),
            ((19.0, -9.5), (23.0, -7.0)),
            ((-19.0, 9.5), (-23.0, 7.0)),
            ((19.0, 9.5), (23.0, 7.0)),
        ] {
            let t = run(&plate(), &p(stepover), &picks(None, Some(exit))).expect("an exit alone is always reached");
            assert_eq!(leave(&t), out, "stepover {stepover}: the raster leaves at the corner nearest {exit:?}");
            let auto = face_over(&plate(), &p(stepover), 0.0, None).expect("the automatic face");
            assert_eq!(row_pieces(&t), row_pieces(&auto), "the rows never change");
        }
    }
}

#[test]
fn an_exit_beside_the_first_row_says_where_the_raster_ends() {
    // Low X is reached (six rows), but at the far edge: said, never silent.
    let t = run(&plate(), &p(3.0), &picks(Some((-19.0, -9.5)), Some((-19.0, -9.0)))).expect("the side is reached");
    assert_eq!(leave(&t), (-23.0, 7.0));
    assert!(t.notes.iter().any(|n| n.contains("exit point")), "{:?}", t.notes);
}

#[test]
fn blended_turns_stay_outside_the_footprint_from_every_corner() {
    for radius in [None, Some(1.0)] {
        for pick in [(-19.0, -9.5), (19.0, -9.5), (-19.0, 9.5), (19.0, 9.5)] {
            let mut fp = p(3.0);
            fp.blend_turns = true;
            fp.blend_radius = radius;
            let t = run(&plate(), &fp, &picks(Some(pick), None)).expect("blended");
            let arcs: Vec<_> = t.moves.iter().filter_map(|m| m.arc.as_ref().map(|a| (m, a))).collect();
            assert!(!arcs.is_empty(), "{pick:?}: the reversals are blended");
            for (m, a) in arcs {
                // The arc's midpoint: the start's radius vector turned
                // half the sweep (a semicircle or a quarter) its own way.
                let (vx, vy) = (m.from.x - a.center.x, m.from.y - a.center.y);
                let (wx, wy) = (m.to.x - a.center.x, m.to.y - a.center.y);
                let half = 0.5 * ((vx * wx + vy * wy) / (vx * vx + vy * vy)).clamp(-1.0, 1.0).acos();
                let ang = if a.ccw { half } else { -half };
                let mid_x = a.center.x + vx * ang.cos() - vy * ang.sin();
                assert!(mid_x.abs() > 23.0 + 1e-6, "{pick:?} radius {radius:?}: the turn bulges into the overrun, not the plate: mid x {mid_x}");
            }
            // The reversal plan follows the rows: from a high Y corner the
            // first step is the band's 2 mm snap, one semicircle of 1 mm.
            if radius.is_some() && pick.1 > 0.0 {
                let first = t.moves.iter().find(|m| m.arc.is_some()).expect("an arc");
                assert_eq!((first.from.y, first.to.y), (7.0, 5.0), "{pick:?}: the first turn is the snap row's own");
            }
            let mut sharp = fp;
            sharp.blend_turns = false;
            let s = run(&plate(), &sharp, &picks(Some(pick), None)).expect("sharp");
            assert_eq!(row_pieces(&t), row_pieces(&s), "{pick:?}: the rows never change");
        }
    }
}

#[test]
fn the_holes_bound_the_same_row_pieces_from_every_corner() {
    let region = Region::new(vec![rect((0.0, 0.0), 20.0, 10.0), rect((2.0, 1.0), 4.0, 3.0)]).unwrap();
    let auto = face_over(&region, &p(3.0), 0.0, None).expect("the automatic face");
    assert!(row_pieces(&auto).len() > 6, "the hole splits rows");
    for pick in [(19.0, -9.5), (-19.0, 9.5), (19.0, 9.5)] {
        let t = run(&region, &p(3.0), &picks(Some(pick), None)).expect("picked");
        assert_eq!(row_pieces(&t), row_pieces(&auto), "{pick:?}: the hole law keeps its pieces");
        assert_eq!(t.notes, auto.notes, "{pick:?}: and its notes");
    }
}

#[test]
fn the_lead_runs_on_along_the_picked_first_row() {
    let t = face_picked(&plate(), &p(3.0), 5.0, None, None, &picks(Some((19.0, 9.5)), None)).expect("led");
    assert_eq!(entry(&t), (28.0, 7.0), "the entry stands the lead farther out, on the first row");
}

#[test]
fn a_pick_no_planner_can_read_is_refused() {
    let why = refusal(run(&plate(), &p(3.0), &picks(Some((f64::NAN, 0.0)), None)));
    assert!(why.contains("start point"), "{why}");
    let mut two = picks(None, Some((19.0, 9.5)));
    two.exit.push(two.exit[0]);
    let why = refusal(run(&plate(), &p(3.0), &two));
    assert!(why.contains("one exit point"), "{why}");
}

/// The six-row face of `plate()` with the keep-out `ko` at every level.
fn kept(ko: &Region, k: &PathPicks) -> Result<Toolpath, GenError> {
    let at = |_z: f64| Ok(ko.clone());
    let keep = FaceKeep { names: "Clamp".to_owned(), at: &at };
    face_picked(&plate(), &p(3.0), 0.0, None, Some(&keep), k)
}

/// #3326 review: the picks are judged on the rows that are cut. A keep-out
/// that takes a terminal row is refused with the blocked pick named and
/// marked; it never moves the entry or the exit to a different corner.
#[test]
fn a_keep_out_that_takes_a_terminal_row_refuses_the_blocked_pick_by_name() {
    let k = picks(Some((-19.0, -9.5)), Some((-19.0, 9.5)));
    let spot = |e: &GenError| e.location().map(|at| (at.x, at.y, at.z));
    // The last of six rows (Y 7): without it the cut would end at (23, 5).
    let last = Region::new(vec![rect((0.0, 7.0), 30.0, 1.0)]).unwrap();
    let e = kept(&last, &k).expect_err("the exit row is taken");
    assert!(e.to_string().contains("the exit point is blocked") && e.to_string().contains("last row"), "{e}");
    assert_eq!(spot(&e), Some((-19.0, 9.5, CUT_Z)), "the exit pick is marked");
    // The picked first row (Y -7).
    let first = Region::new(vec![rect((0.0, -7.0), 30.0, 1.0)]).unwrap();
    let e = kept(&first, &k).expect_err("the start row is taken");
    assert!(e.to_string().contains("the start point is blocked") && e.to_string().contains("first row"), "{e}");
    assert_eq!(spot(&e), Some((-19.0, -9.5, CUT_Z)), "the start pick is marked");
    // An exit pick alone promises its corner: the same law.
    let why = refusal(kept(&last, &picks(None, Some((-19.0, 9.5)))));
    assert!(why.contains("the exit point is blocked"), "{why}");
    // With no pick the keep-out law is unchanged: the rows are cut short.
    for ko in [&last, &first] {
        kept(ko, &PathPicks::default()).expect("no pick, nothing to hold");
    }
    // A keep-out on a middle row leaves the two picks as they were.
    let mid = Region::new(vec![rect((0.0, -1.0), 4.0, 0.5)]).unwrap();
    let t = kept(&mid, &k).expect("the terminal rows are whole");
    assert_eq!((entry(&t), leave(&t)), ((-23.0, -7.0), (-23.0, 7.0)));
    let in_keep = |m: &camcraft_motion::cl::Move| m.to.z == CUT_Z && m.to.y == -1.0 && m.to.x.abs() < 4.0;
    assert!(!t.moves.iter().any(in_keep), "the keep-out stays clear");
}
