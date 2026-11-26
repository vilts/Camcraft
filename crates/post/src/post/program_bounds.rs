//! The program's cut XY bounding box (#1882), published to a script as
//! `programBounds` before `onOpen`, and arc linearization for a control
//! that has no G2/G3 (`linearizeArcs`, `chordTolerance`).
//!
//! Provenance: our own code. The extremes of an arc are its endpoints
//! and the quarter points that its sweep passes (`judge::move_extremes`,
//! general geometry); the chords are `cl::Move::segments3`, the one
//! sagitta law of the code base. A sheet-cutting control (a waterjet
//! table) checks the program extent against its travel before it runs.

use super::cnc::{MoveRec, Record};
use super::js::Script;
use super::PostError;
use crate::cl::{Move, MoveKind};
use crate::geo2d::Point2;
use crate::geometry::Point3;
use serde_json::{json, Value};

/// The `cl::Move` that a stream record traverses from `from`.
fn as_move(from: (f64, f64, f64), m: &MoveRec) -> Move {
    let mv = Move::new(m.kind, Point3::new(from.0, from.1, from.2), Point3::new(m.x, m.y, m.z), m.feed);
    match m.center {
        Some((u, v)) => mv.with_arc_in(Point2::new(u, v), m.ccw, m.plane),
        None => mv,
    }
}

/// The XY box of every cutting record (feeds, plunges, arcs at their
/// quarter points, cycle centres), as `{minX, maxX, minY, maxY}` in mm,
/// or `null` when the stream cuts nothing. Rapids do not count: the box
/// is where material is cut. A non-finite point is skipped, never
/// published as a bound.
#[must_use]
pub fn bounds(records: &[Record]) -> Value {
    let mut pos = (0.0f64, 0.0f64, 0.0f64);
    let mut b: Option<[f64; 4]> = None;
    let mut grow = |x: f64, y: f64| {
        if !(x.is_finite() && y.is_finite()) {
            return;
        }
        let r = b.get_or_insert([x, x, y, y]);
        *r = [r[0].min(x), r[1].max(x), r[2].min(y), r[3].max(y)];
    };
    for r in records {
        match r {
            Record::Param { name, value } => match name.as_str() {
                "StartX" => pos.0 = value.as_f64().unwrap_or(0.0),
                "StartY" => pos.1 = value.as_f64().unwrap_or(0.0),
                "StartZ" => pos.2 = value.as_f64().unwrap_or(0.0),
                _ => {}
            },
            Record::Move(m) => {
                if m.kind != MoveKind::Rapid {
                    for p in crate::machine::move_extremes(&as_move(pos, m)) {
                        grow(p.x, p.y);
                    }
                }
                pos = (m.x, m.y, m.z);
            }
            Record::Cycle(c) => {
                grow(c.x, c.y);
                pos = (c.x, c.y, c.r_z);
            }
            Record::Touch(t) => pos = (t.expect[0], t.expect[1], t.expect[2]),
            Record::Retreat(t) => pos = (t.expect[0], t.expect[1], t.expect[2]),
            Record::Guard(g) => pos = (g.to[0], g.to[1], g.to[2]),
            _ => {}
        }
    }
    b.map_or(Value::Null, |[x0, x1, y0, y1]| json!({ "minX": x0, "maxX": x1, "minY": y0, "maxY": y1 }))
}

/// The chord tolerance (mm) when the script asks for linear arcs
/// (`linearizeArcs = true`, read after `onOpen`, so a script may set it
/// there from a declared option), `None` when it does not. `chordTolerance`
/// defaults to 0.01 mm; a stated value that is not a positive finite
/// number is refused by name.
pub fn linearize_tolerance(s: &mut Script) -> Result<Option<f64>, PostError> {
    if !super::js::is_true(&s.eval_src("typeof linearizeArcs !== \"undefined\" && linearizeArcs === true")?) {
        return Ok(None);
    }
    let stated = !super::js::is_true(&s.eval_src("typeof chordTolerance === \"undefined\"")?);
    match s.number_global("chordTolerance")? {
        Some(t) if t.is_finite() && t > 0.0 => Ok(Some(t)),
        None if !stated => Ok(Some(DEFAULT_CHORD_TOL)),
        _ => Err(PostError::Refused(
            "this post script states linearizeArcs, but its chordTolerance is not a positive number of millimetres".into(),
        )),
    }
}

/// The chord tolerance when a script asks for linear arcs and states
/// none, mm.
pub const DEFAULT_CHORD_TOL: f64 = 0.01;

/// The straight pieces of an arc record from `from`, at sagitta `tol`:
/// each piece is a feed record with the arc's feed and angles, the
/// last ending exactly on the arc's end point.
pub fn arc_pieces(from: (f64, f64, f64), m: &MoveRec, tol: f64) -> Result<Vec<MoveRec>, PostError> {
    let mv = as_move(from, m);
    let mut arcs = vec![mv];
    if let Some(arc) = mv.arc {
        let (au, av, aw) = arc.plane.uvw(mv.from);
        let (bu, bv, bw) = arc.plane.uvw(mv.to);
        let (cu, cv) = arc.plane.center_uv(arc.center);
        // Equal in-plane endpoints denote a full circle in a posted stream.
        // segments3 treats them as zero sweep, so tessellate two half arcs.
        if au == bu && av == bv {
            let mid = arc
                .plane
                .point(2.0 * cu - au, 2.0 * cv - av, aw * 0.5 + bw * 0.5);
            arcs = vec![Move { to: mid, ..mv }, Move { from: mid, ..mv }];
        }
    }
    let mut out = Vec::new();
    for arc_move in arcs {
        let segments = arc_move.segments3(tol);
        if let Some(arc) = arc_move.arc {
            let (au, av, _) = arc.plane.uvw(arc_move.from);
            let (bu, bv, _) = arc.plane.uvw(arc_move.to);
            let (cu, cv) = arc.plane.center_uv(arc.center);
            let ra = (au - cu).hypot(av - cv);
            let rb = (bu - cu).hypot(bv - cv);
            let start = (av - cv).atan2(au - cu);
            let end = (bv - cv).atan2(bu - cu);
            let sweep =
                if arc.ccw { end - start } else { start - end }.rem_euclid(std::f64::consts::TAU);
            // segments3 has a hard cap. Check its original curvature before
            // posting: more straight subdivisions cannot recover a lost arc.
            let half_step_sin = (sweep / (4.0 * segments.len().max(1) as f64)).sin();
            let error = ra.max(rb) * 2.0 * half_step_sin * half_step_sin + (ra - rb).abs();
            if !error.is_finite() || error > tol {
                return Err(PostError::Refused(format!(
                    "arc subdivision limit cannot meet chordTolerance {tol} mm"
                )));
            }
        }
        out.extend(segments.into_iter().map(|(_, b)| MoveRec {
            x: b.x,
            y: b.y,
            z: b.z,
            center: None,
            ..m.clone()
        }));
    }
    // The record's own end point closes the arc, bit for bit.
    match out.last_mut() {
        Some(last) => (last.x, last.y, last.z) = (m.x, m.y, m.z),
        None => out.push(MoveRec {
            center: None,
            ..m.clone()
        }),
    }
    Ok(out)
}
