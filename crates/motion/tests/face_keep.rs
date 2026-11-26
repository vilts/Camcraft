//! #3343: Face rows kept clear of a "Do not touch" keep-out - each level's
//! rows are cut short at the keep-out, a row-end step that crosses it
//! becomes a link over it, a keep-out that takes every row refuses by
//! name, and an empty keep-out is `face_over` to the byte.
//!
//! Gate map (#3343): covers camcraft_motion::machining2d (face, face_keep).

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{face_over, face_over_kept, FaceHoles, FaceKeep, FaceParams, GenError};

fn sq(c: (f64, f64), hx: f64, hy: f64) -> Region {
    Region::new(vec![Loop::new(vec![
        Point2::new(c.0 - hx, c.1 - hy),
        Point2::new(c.0 + hx, c.1 - hy),
        Point2::new(c.0 + hx, c.1 + hy),
        Point2::new(c.0 - hx, c.1 + hy),
    ])
    .unwrap()])
    .unwrap()
}

fn p(blend: bool, staged: bool) -> FaceParams {
    FaceParams {
        tool_diameter: 6.0,
        stepover: 3.0,
        clearance_z: 5.0,
        cut_z: -2.0,
        top_z: staged.then_some(0.0),
        stepdown: staged.then_some(1.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        blend_turns: blend,
        blend_radius: None,
        holes: FaceHoles::default(),
        stock_top: None,
    }
}

/// The first move with a sample (below the clearance plane) inside the box `c` +- `(hx, hy)`.
fn enters(t: &Toolpath, c: (f64, f64), hx: f64, hy: f64) -> Option<String> {
    for m in &t.moves {
        if m.from.z >= 5.0 - 1e-9 && m.to.z >= 5.0 - 1e-9 {
            continue;
        }
        for k in 0..=50 {
            let s = f64::from(k) / 50.0;
            let x = m.from.x + (m.to.x - m.from.x) * s;
            let y = m.from.y + (m.to.y - m.from.y) * s;
            if (x - c.0).abs() < hx - 1e-6 && (y - c.1).abs() < hy - 1e-6 {
                return Some(format!("{m:?}"));
            }
        }
    }
    None
}

#[test]
fn empty_keepout_is_face_over_to_the_byte() {
    let region = sq((0.0, 0.0), 30.0, 20.0);
    let none = |_z: f64| -> Result<Region, GenError> { Ok(Region::empty()) };
    for (blend, staged) in [(false, false), (true, false), (false, true), (true, true)] {
        let keep = FaceKeep { names: "nothing".into(), at: &none };
        let plain = face_over(&region, &p(blend, staged), 0.0, None).unwrap();
        let kept = face_over_kept(&region, &p(blend, staged), 0.0, None, Some(&keep)).unwrap();
        assert_eq!(plain, kept, "blend {blend}, staged {staged}");
    }
}

#[test]
fn rows_and_steps_keep_out_of_the_keepout_and_link_over_it() {
    let region = sq((0.0, 0.0), 30.0, 20.0);
    // One keep-out in the middle; one between the rows at Y -5 and -2, over
    // the +X end where the step from the one row to the other runs.
    let at = |_z: f64| -> Result<Region, GenError> {
        sq((0.0, 0.0), 6.0, 6.0).union(&sq((33.0, -3.5), 2.0, 1.0)).map_err(|e| GenError::Invalid(e.to_string()))
    };
    for blend in [false, true] {
        let keep = FaceKeep { names: "box".into(), at: &at };
        let t = face_over_kept(&region, &p(blend, true), 0.0, None, Some(&keep)).unwrap();
        assert_eq!(enters(&t, (0.0, 0.0), 6.0, 6.0), None, "blend {blend}: a move enters the middle keep-out");
        let ends = enters(&t, (33.0, -3.5), 2.0, 1.0);
        assert_eq!(ends, None, "blend {blend}: a step crosses the row-end keep-out");
        assert!(t.notes.iter().any(|n| n.contains("Do not touch (box)")), "{:?}", t.notes);
        assert!(t.moves.iter().any(|m| m.kind == MoveKind::Feed));
    }
}

#[test]
fn a_keepout_over_every_row_refuses_by_name() {
    let region = sq((0.0, 0.0), 30.0, 20.0);
    let all = |_z: f64| -> Result<Region, GenError> { Ok(sq((0.0, 0.0), 100.0, 100.0)) };
    let keep = FaceKeep { names: "plate face #1".into(), at: &all };
    let err = face_over_kept(&region, &p(false, true), 0.0, None, Some(&keep)).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("plate face #1") && msg.contains("Do not touch"), "{msg}");
}

/// #3526: a keep-out that grows at the lower levels (an item wider under
/// the face) blocks the level at Cut Z. The face would stop one level
/// above Cut Z: refused by name, with the depth that it reaches.
#[test]
fn a_keepout_that_blocks_the_lower_levels_refuses_with_the_depth_reached() {
    let region = sq((0.0, 0.0), 30.0, 20.0);
    let wider_below = |z: f64| -> Result<Region, GenError> {
        Ok(if z < -1.5 { sq((0.0, 0.0), 100.0, 100.0) } else { sq((0.0, 0.0), 6.0, 6.0) })
    };
    let keep = FaceKeep { names: "flange face #7".into(), at: &wider_below };
    let err = face_over_kept(&region, &p(false, true), 0.0, None, Some(&keep)).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("flange face #7") && msg.contains("Do not touch"), "names the item: {msg}");
    assert!(msg.contains("Cut Z -2.000"), "names Cut Z: {msg}");
    assert!(msg.contains("Z -1.000"), "names the depth reached: {msg}");
}
