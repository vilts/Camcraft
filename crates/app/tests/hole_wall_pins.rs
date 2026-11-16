//! #1205's hole-wall excuse (`sim::verify::hole_wall`, `WALL_SAG`) pinned
//! with the cases of UMA's #1217 / #1231 tests (PRs #1228, #1235), which
//! #1205 superseded: a coarse polygon hole (an octagon, a 16-gon, corners
//! on r 5) cut out to its corner circle by a drill, a bore orbit or a
//! thread-mill helix convicts on its flats whatever radius the tool path
//! was declared at; a full-fidelity round hole (a 96-gon) and a finely
//! tessellated one (a 32-gon r 3 / r 5, a 16-gon r 2) stay clean when
//! cut at their own size. With no declared hole axis nothing is excused.
//! (#1205's excuse is measured about the axis, not a declared rim radius,
//! so the old "rim declared elsewhere" cases are the same axis here.)
//!
//! Provenance: our own fixtures, specced from the Prime Directive and the
//! chord sagitta s = r - sqrt(r^2 - (l/2)^2); no vendor material.
//!
//! Gate map (solid law): covers sim::verify (solid_hits_walled, hole_wall).

mod common;

use common::solid_part::{circle, mill, plate, rect};

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Point3, Triangle};
use camcraft_app::verify::{Cutter, HoleAxis, SolidLaw, Warning, solid_hits_walled};

const C: (f64, f64) = (20.0, 20.0);

/// A 40 x 40 plate, z -10..0, with a blind n-gon hole (corners on `r`) down to z -6.
fn part(n: usize, r: f64) -> Vec<Triangle> {
    let hole = circle(C, r, n);
    let mut f = Vec::new();
    plate(&rect(0.0, 0.0, 40.0, 40.0), Some((&hole, -6.0)), -10.0, 0.0, &mut f);
    f
}

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The containment law as the operation layer runs it; `axis` is whether
/// the op declares the hole.
fn contain(t: &Toolpath, part: &[Triangle], d: f64, axis: bool) -> Vec<Warning> {
    let law = SolidLaw { step: 0.25, allow: 0.01, hole_walls: true };
    let axes = if axis { vec![HoleAxis::vertical(C.0, C.1)] } else { Vec::new() };
    solid_hits_walled(t, part, &Cutter::flat(&mill(d)), &law, &axes)
}

fn drill() -> Toolpath {
    Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(C.0, C.1, 5.0), p(C.0, C.1, 1.0), None),
        Move::new(MoveKind::Plunge, p(C.0, C.1, 1.0), p(C.0, C.1, -3.0), Some(100.0)),
        Move::new(MoveKind::Rapid, p(C.0, C.1, -3.0), p(C.0, C.1, 5.0), None),
    ])
}

/// A 6 mm cutter orbiting r 2 (reach r 5): flat at z -2 (a bore) or
/// falling `pitch` a turn (a thread mill's helix).
fn orbit(pitch: f64) -> Toolpath {
    let (x0, x1) = (C.0 + 2.0, C.0 - 2.0);
    let c = Point2::new(C.0, C.1);
    let z = -2.0;
    let half = 0.5 * pitch;
    Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(x0, C.1, 5.0), p(x0, C.1, 1.0), None),
        Move::new(MoveKind::Plunge, p(x0, C.1, 1.0), p(x0, C.1, z), Some(100.0)),
        Move::new(MoveKind::Feed, p(x0, C.1, z), p(x1, C.1, z - half), Some(300.0)).with_arc(c, true),
        Move::new(MoveKind::Feed, p(x1, C.1, z - half), p(x0, C.1, z - pitch), Some(300.0)).with_arc(c, true),
        Move::new(MoveKind::Rapid, p(x0, C.1, z - pitch), p(x0, C.1, 5.0), None),
    ])
}

fn gouged(w: &[Warning]) -> bool {
    w.iter().any(|w| matches!(w, Warning::Gouge { .. }))
}

#[test]
fn a_coarse_polygon_holes_flats_cut_at_its_corner_circle_convict() {
    for n in [8, 16] {
        let f = part(n, 5.0);
        for axis in [true, false] {
            assert!(gouged(&contain(&orbit(0.0), &f, 6.0, axis)), "{n}-gon axis {axis}: bore cut its flats silently");
            assert!(gouged(&contain(&orbit(1.0), &f, 6.0, axis)), "{n}-gon axis {axis}: thread helix cut its flats silently");
            assert!(gouged(&contain(&drill(), &f, 10.0, axis)), "{n}-gon axis {axis}: drill cut its flats silently");
        }
    }
}

#[test]
fn a_full_fidelity_round_hole_stays_clean_for_every_containment_kind() {
    let f = part(96, 5.0);
    for axis in [true, false] {
        assert_eq!(contain(&drill(), &f, 10.0, axis), Vec::new(), "drill axis {axis}");
        assert_eq!(contain(&orbit(0.0), &f, 6.0, axis), Vec::new(), "bore axis {axis}");
        assert_eq!(contain(&orbit(1.0), &f, 6.0, axis), Vec::new(), "thread helix axis {axis}");
    }
}

#[test]
fn a_coarse_round_hole_is_excused_only_on_its_declared_axis() {
    // A 32-gon at r 5 dips 0.024 mm: past the mesh tolerance, so only a
    // declared axis excuses it.
    let f = part(32, 5.0);
    assert_eq!(contain(&drill(), &f, 10.0, true), Vec::new(), "drill on its axis");
    assert_eq!(contain(&orbit(0.0), &f, 6.0, true), Vec::new(), "bore on its axis");
    assert!(gouged(&contain(&drill(), &f, 10.0, false)), "drill with no axis declared");
}

#[test]
fn a_32_gon_r3_matched_drill_stays_clean() {
    assert_eq!(contain(&drill(), &part(32, 3.0), 6.0, true), Vec::new());
}

#[test]
fn a_small_tessellated_round_hole_keeps_its_axis_credit() {
    // A 16-gon r 2 hole dips 0.038 mm: tessellation, so a matched 4 mm
    // drill is clean - and gouges when no axis is declared.
    let f = part(16, 2.0);
    assert_eq!(contain(&drill(), &f, 4.0, true), Vec::new());
    assert!(gouged(&contain(&drill(), &f, 4.0, false)), "no axis, no credit");
}
