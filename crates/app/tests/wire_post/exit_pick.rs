//! #3332 (part of #1448): the Wire Contour's exit pick through the
//! commands. An open run ends at the end nearest the pick; a closed
//! profile is cut all the way round, then left at the pick; each pick
//! the wire cannot hold is refused by name; undo and redo restore the
//! path; a file with the old `start` key opens to the same path.
//!
//! Gate map (#3332): covers app::operation (pick_kinds, wire_op,
//! wire_sketch_arcs), app::project (wire_edm_dto), app::posting_wire,
//! motion::wire_edm::exit.
//! Provenance: our own tests over our own synthetic fixture.

use super::*;

use camcraft_app::geo2d::Point2;
use camcraft_app::path_picks::PathPick;

fn op_of(world: &World, name: &str) -> WireContourOp {
    let e = model::find_by_name(world, name).expect("op");
    match model::operation_of(world, &e).expect("operation").params {
        OpParams::WireContour(w) => w,
        other => panic!("not a wire contour: {other:?}"),
    }
}

fn points(world: &World, name: &str) -> Vec<(f64, f64)> {
    let e = model::find_by_name(world, name).expect("op");
    let path = model::toolpath_of(world, &e).expect("generated").path;
    let mut v = vec![(path.moves[0].from.x, path.moves[0].from.y)];
    v.extend(path.moves.iter().map(|m| (m.to.x, m.to.y)));
    v
}

fn cl_text(world: &World, name: &str) -> String {
    let e = model::find_by_name(world, name).expect("op");
    camcraft_app::cl::text::to_text(&model::toolpath_of(world, &e).expect("generated").path).expect("cl text")
}

/// Edit `name` to `exit` (other fields kept) and generate it again.
fn set_exit(world: &World, h: &mut History, name: &str, exit: Option<(f64, f64)>) -> Result<(), String> {
    let mut w = op_of(world, name);
    w.picks.exit = exit.map(|(x, y)| PathPick::xy(Point2::new(x, y))).into_iter().collect();
    let edit = EditOperationParams::new(world, name, OpParams::WireContour(w)).map_err(|e| e.to_string())?;
    h.execute(world, edit);
    let run = GenerateOperation::new(world, name).map_err(|e| e.to_string())?;
    h.execute(world, run);
    Ok(())
}

/// Done means: an exit at the pick on an open run. The cut-off at
/// x = 60 across the 50 mm bar ends at the end nearest the exit pick,
/// on the same line, the kerf on the same side; it passes the wire laws
/// and posts with the wire threaded at the other end.
#[test]
fn an_exit_pick_on_an_open_run_ends_the_cut_at_the_end_nearest_the_pick() {
    let world = cutoff_doc(false);
    let mut h = History::default();
    let auto = points(&world, "Cutoff1");
    let (first, last) = (auto[0], auto[auto.len() - 1]);
    assert!((first.1 - last.1).abs() > 50.0, "the run crosses the bar: {auto:?}");

    // A pick near the end where the automatic cut starts turns the cut round.
    set_exit(&world, &mut h, "Cutoff1", Some((61.0, first.1))).expect("the exit pick is taken");
    let turned = points(&world, "Cutoff1");
    let back: Vec<(f64, f64)> = auto.iter().rev().copied().collect();
    assert_eq!(turned, back, "the same line, walked the other way");
    assert!(turned.iter().all(|q| (q.0 - 60.165).abs() < 1e-9), "the kerf stays on the offcut's side: {turned:?}");
    let clause = posting::wire_clause(&world);
    assert!(clause.problems.is_empty(), "{:?}", clause.problems);
    let text = post_text(&world);
    let g0 = text.lines().find(|l| l.starts_with("G0 ")).expect("one positioning move");
    let y = g0.split_whitespace().find_map(|t| t.strip_prefix('Y')?.parse::<f64>().ok()).expect("a Y word");
    assert!((y - last.1).abs() < 1e-3, "threaded at the far end, {last:?}: {text}");

    // A pick near the end where the automatic cut already ends changes no move.
    set_exit(&world, &mut h, "Cutoff1", Some((59.0, last.1))).expect("the exit pick is taken");
    assert_eq!(points(&world, "Cutoff1"), auto);
}

/// Done means: an impossible pick is refused by name. A pick the same
/// distance from the two ends of the run names no end.
#[test]
fn an_exit_pick_between_the_ends_of_an_open_run_is_refused_by_name() {
    let world = cutoff_doc(false);
    let mut h = History::default();
    let why = set_exit(&world, &mut h, "Cutoff1", Some((70.0, 25.0))).expect_err("refused");
    assert!(why.contains("exit point") && why.contains("X70.000 Y25.000") && why.contains("both ends"), "{why}");
}

/// A closed profile: the punch round the 40 x 20 slug from (-30, 0) is
/// cut all the way round, rides its own kerf on to the point nearest the
/// exit pick and leaves straight to the pick. The wire laws pass it.
#[test]
fn an_exit_pick_on_a_closed_profile_leaves_the_cut_at_the_pick() {
    let world = punch_doc(FIXTURE);
    let mut h = History::default();
    let auto = points(&world, "Punch1");
    assert_eq!(auto[auto.len() - 1], (-30.0, 0.0), "without a pick the wire returns to the threading point");

    set_exit(&world, &mut h, "Punch1", Some((30.0, 2.0))).expect("the exit pick is taken");
    let v = points(&world, "Punch1");
    let n = v.len();
    assert_eq!(v[0], (-30.0, 0.0), "threaded at the start pick");
    assert_eq!(v[n - 1], (30.0, 2.0), "the cut ends at the exit pick");
    assert!((v[n - 2].0 - 20.165).abs() < 1e-9 && (v[n - 2].1 - 2.0).abs() < 1e-9, "left from the path's nearest point: {:?}", v[n - 2]);
    // The profile is closed before the wire leaves: the whole automatic
    // lap is there, without its return to the threading point.
    assert_eq!(&v[..auto.len() - 1], &auto[..auto.len() - 1], "the lap round the profile does not move");
    let clause = posting::wire_clause(&world);
    assert!(clause.problems.is_empty(), "{:?}", clause.problems);
    assert!(post_text(&world).contains("X30. Y2."), "the program ends at the pick");
}

/// The laws a pick must keep, each refused by name and never moved: an
/// exit in the material that stays, an exit on the wire path, and an
/// exit beside tabs, skims, a taper or a 4-axis cut.
#[test]
fn each_exit_pick_the_wire_cannot_hold_is_refused_by_name() {
    let world = punch_doc(FIXTURE);
    let mut h = History::default();
    let why = set_exit(&world, &mut h, "Punch1", Some((0.0, 0.0))).expect_err("an exit in the punch");
    assert!(why.contains("exit point") && why.contains("X0.000 Y0.000") && why.contains("inside the profile"), "{why}");
    let why = set_exit(&world, &mut h, "Punch1", Some((20.1, 0.0))).expect_err("an exit nearer than the kerf");
    assert!(why.contains("exit point") && why.contains("closer to the profile than the kerf offset"), "{why}");
    let why = set_exit(&world, &mut h, "Punch1", Some((20.165, 0.0))).expect_err("an exit on the path");
    assert!(why.contains("exit point") && why.contains("on the wire path itself"), "{why}");

    let exit = vec![PathPick::xy(Point2::new(30.0, 2.0))];
    let with = |change: &dyn Fn(&mut WireContourOp)| {
        let mut w = op_of(&world, "Punch1");
        w.picks.exit = exit.clone();
        change(&mut w);
        EditOperationParams::new(&world, "Punch1", OpParams::WireContour(w)).err().map(|e| e.to_string()).unwrap_or_default()
    };
    let why = with(&|w| w.skims = vec![skim("S21", false)]);
    assert!(why.contains("exit point") && why.contains("skim"), "{why}");
    let why = with(&|w| w.taper_deg = 3.0);
    assert!(why.contains("exit point") && why.contains("taper"), "{why}");
    let why = with(&|w| w.four_axis = Some(operation::WireFourAxis::default()));
    assert!(why.contains("exit point") && why.contains("4-axis"), "{why}");
    let why = with(&|w| w.burnout = Some(1.0));
    assert!(why.contains("exit point") && why.contains("burnout"), "{why}");
    let why = with(&|w| w.tabs = Some(operation::WireTabs::default()));
    assert!(why.contains("exit point") && why.contains("tabs"), "{why}");
}

/// Done means: undo and redo restore the previous path.
#[test]
fn undo_and_redo_restore_the_path_of_an_exit_pick() {
    let world = punch_doc(FIXTURE);
    let mut h = History::default();
    let auto = cl_text(&world, "Punch1");
    set_exit(&world, &mut h, "Punch1", Some((30.0, 2.0))).expect("the exit pick is taken");
    let picked = cl_text(&world, "Punch1");
    assert_ne!(picked, auto);
    // Undo the generation and the edit.
    assert!(h.undo(&world) && h.undo(&world));
    assert!(op_of(&world, "Punch1").picks.exit.is_empty(), "the exit pick is gone");
    assert_eq!(cl_text(&world, "Punch1"), auto, "undo restores the path, byte for byte");
    assert!(h.redo(&world) && h.redo(&world));
    assert_eq!(op_of(&world, "Punch1").picks.exit.len(), 1, "the exit pick is back");
    assert_eq!(cl_text(&world, "Punch1"), picked, "redo restores the path, byte for byte");
}

/// Done means: a file with the old `start` key opens to the same path,
/// byte-identical, and to the same stamp (the cached path stays current).
#[test]
fn a_file_with_the_old_start_key_opens_to_the_same_path() {
    let world = punch_doc(FIXTURE);
    let auto = cl_text(&world, "Punch1");
    let dir = std::env::temp_dir().join(format!("camcraft-wire-exit-3332-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let file = dir.join("new.camproj");
    project::save(&world, &file).expect("save");
    let text = std::fs::read_to_string(&file).expect("read");
    let mut v: serde_json::Value = serde_json::from_str(&text).expect("json");
    // Rewrite every wire contour to the key of the files before #3339.
    fn rewrite(v: &mut serde_json::Value, n: &mut usize) {
        match v {
            serde_json::Value::Object(m) => {
                if let Some(w) = m.get_mut("wire_contour").and_then(|w| w.as_object_mut())
                    && let Some(picks) = w.remove("picks")
                {
                    w.insert("start".into(), picks["start"][0].clone());
                    *n += 1;
                }
                m.values_mut().for_each(|c| rewrite(c, n));
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(|c| rewrite(c, n)),
            _ => {}
        }
    }
    let mut n = 0;
    rewrite(&mut v, &mut n);
    assert_eq!(n, 1, "one wire contour is rewritten: {text:.400}");
    let old_text = v.to_string();
    assert!(old_text.contains(r#""start":[-30.0,0.0]"#) && !old_text.contains("picks"), "{old_text:.600}");
    let old = dir.join("old.camproj");
    std::fs::write(&old, old_text).expect("write");

    let back = new_world();
    let loaded = project::load(&back, &old);
    assert!(loaded.is_ok(), "{:?}", loaded.err());
    let w = op_of(&back, "Punch1");
    assert_eq!(w.picks, op_of(&world, "Punch1").picks, "the old key is the start pick");
    let mut h = History::default();
    h.execute(&back, GenerateOperation::new(&back, "Punch1").expect("generates"));
    assert_eq!(cl_text(&back, "Punch1"), auto, "the same path, byte for byte");
    let _ = std::fs::remove_dir_all(&dir);
}
