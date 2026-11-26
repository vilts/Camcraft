//! #3747: the rings of a 2D Offset round a part stay at depth, and the
//! link from ring to ring stays in the cut plane. A lift is permitted only
//! where each in-plane link would cut the part or a fixture (or a law the
//! user set, such as the stay-down limit), and a note names its reason.
//! The cases: a disc part and the dog-bone part of #3744 in a stock box of
//! 80 x 50 (the Outside reach: the box grown by the tool diameter), a tool
//! of radius 4, a step of 2, one level. The part splits no ring; the box
//! edge splits the outer rings into pieces that end in air.

use super::{box_facets, mask, params, rect, region};
use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geo2d::{Point2, Region};
use camcraft_motion::machining2d::{
    CornerRelief, PassStart, StartSide, pocket_avoid_started_on, pocket_started_on,
};
use camcraft_motion::tools::ToolGeom;
use camcraft_motion::verify::{AVOID_VERIFY_TOL, AvoidItem, AvoidSpec, Cutter, avoid_hits};

const R: f64 = super::R;
const Z: f64 = super::Z;

fn disc(cx: f64, r: f64) -> Vec<Point2> {
    (0..128)
        .map(|k| {
            let a = f64::from(k) * std::f64::consts::TAU / 128.0;
            Point2::new(cx + r * a.cos(), r * a.sin())
        })
        .collect()
}

pub(super) fn stock() -> Region {
    region(vec![rect(-40.0, -25.0, 40.0, 25.0)])
}

/// A disc of radius 12 in the middle of the box.
pub(super) fn disc_part() -> Region {
    region(vec![disc(0.0, 12.0)])
}

/// The #3744 dog bone: two discs of radius 12 at X -15 and X 15, joined by
/// a neck 8 mm wide.
pub(super) fn dog_bone() -> Region {
    region(vec![disc(-15.0, 12.0)])
        .union(&region(vec![disc(15.0, 12.0)]))
        .and_then(|p| p.union(&region(vec![rect(-15.0, -4.0, 15.0, 4.0)])))
        .expect("part")
}

/// The band the disc may sweep: the stock grown by the diameter (the
/// Outside reach), less the part.
pub(super) fn band(part: &Region) -> Region {
    stock()
        .offset(2.0 * R)
        .and_then(|s| s.difference(part))
        .expect("band")
}

pub(super) fn side(start: PassStart) -> StartSide {
    StartSide {
        start,
        stock: Some(stock()),
        around_part: true,
    }
}

fn plan(part: &Region, start: PassStart) -> Toolpath {
    let mut p = params();
    p.stepover = 2.0;
    pocket_started_on(&band(part), &p, &[], &CornerRelief::default(), &side(start)).expect("plan")
}

/// The lifts and re-entries of the one level at `Z`: a lift leaves the
/// floor with a cut on the floor still to come; a re-entry comes back to
/// the floor after the first floor cut.
pub(super) fn lifts(path: &Toolpath) -> (usize, usize) {
    let on = |i: usize| {
        path.moves.get(i).is_some_and(|m| {
            m.kind != MoveKind::Rapid && (m.from.z - Z).abs() < 1e-9 && (m.to.z - Z).abs() < 1e-9
        })
    };
    let floor: Vec<usize> = (0..path.moves.len()).filter(|&i| on(i)).collect();
    let (Some(&first), Some(&last)) = (floor.first(), floor.last()) else {
        return (0, 0);
    };
    let mut runs = 0;
    let mut was = false;
    for i in first..=last {
        let now = on(i);
        if now && !was {
            runs += 1;
        }
        was = now;
    }
    let up = (first..last)
        .filter(|&i| path.moves[i].to.z > Z + 1e-6 && path.moves[i].from.z <= Z + 1e-6)
        .count();
    (up, runs - 1)
}

/// #3744: a crossing of two planar feeds blocks the post; the in-plane
/// links along the edge of the area touch the pieces that end there.
pub(super) fn assert_no_crossing(name: &str, path: &Toolpath) {
    let planar = |i: usize| {
        path.moves
            .get(i)
            .is_some_and(|m| (m.from.z - m.to.z).abs() < 1e-9)
    };
    let crossed: Vec<[usize; 2]> = camcraft_motion::verify::crossed_pairs(path, 0.01)
        .into_iter()
        .filter(|pr| planar(pr[0]) && planar(pr[1]))
        .collect();
    assert!(crossed.is_empty(), "{name}: crossing feeds {crossed:?}");
}

/// The lifts that the link notes name ("lifted ... N time(s)").
pub(super) fn named_lifts(path: &Toolpath) -> usize {
    path.notes
        .iter()
        .filter(|n| n.starts_with("link: "))
        .filter_map(|n| {
            n.split(" time(s)")
                .next()?
                .rsplit(' ')
                .next()?
                .parse::<usize>()
                .ok()
        })
        .sum()
}

/// #3747 pin: round a disc, Inside, Outside and Auto stay on the floor for
/// the whole level: no lift, no re-entry. Each link between the pieces
/// that the box edge cuts stays in the plane (in air or over cut floor),
/// cuts nothing outside the band and crosses no feed. Before #3747: 20
/// lifts on Inside, each with no reason.
#[test]
fn start_side_rings_do_not_lift() {
    for start in [PassStart::Inside, PassStart::Outside, PassStart::Auto] {
        let path = plan(&disc_part(), start);
        let (up, back) = lifts(&path);
        eprintln!(
            "disc {start:?}: {up} lift(s), {back} re-entry(ies), notes {:?}",
            path.notes
        );
        assert_eq!(
            (up, back),
            (0, 0),
            "{start:?}: the rings round a disc lift: {:?}",
            path.notes
        );
        mask("ring_links_disc", &band(&disc_part()), &path).assert_no_overcut();
        assert_no_crossing(&format!("disc {start:?}"), &path);
    }
}

/// #3747 pin (the maintainer's #3741 case): on the dog bone, each lift of
/// Inside and Outside has a named reason, and with each link clear of the
/// part no lift is left. The shape mask proves that no in-plane link cuts
/// the part. Before #3747: 15 lifts on Inside, each with no reason.
#[test]
fn dogbone_start_side_lifts() {
    let part = dog_bone();
    for start in [PassStart::Inside, PassStart::Outside] {
        let path = plan(&part, start);
        let (up, back) = lifts(&path);
        eprintln!(
            "dog bone {start:?}: {up} lift(s), {back} re-entry(ies), notes {:?}",
            path.notes
        );
        assert_eq!(
            named_lifts(&path),
            up,
            "{start:?}: a lift with no named reason: {:?}",
            path.notes
        );
        assert_eq!(
            (up, back),
            (0, 0),
            "{start:?}: the dog bone lifts: {:?}",
            path.notes
        );
        mask("ring_links_dog_bone", &band(&part), &path).assert_no_overcut();
        assert_no_crossing(&format!("dog bone {start:?}"), &path);
    }
}

/// #3747 pin: a fixture (a clamp in the band) across the in-plane link
/// gives a lift, the note names the fixture, and the clamp is not cut.
#[test]
fn lift_reason_is_named() {
    let part = disc_part();
    let spec = AvoidSpec {
        items: vec![AvoidItem {
            label: "clamp".into(),
            tris: box_facets([-2.0, 20.0, -10.0], [2.0, 30.0, 5.0]),
            edges: vec![],
        }],
        cutter: Cutter::flat(&ToolGeom {
            radius: R,
            nose_radius: 0.0,
            flute_len: 10.0,
            shank_radius: R,
            shank_len: 20.0,
            holder_radius: 12.0,
            holder_len: 20.0,
        }),
        clearance: 0.5,
    };
    let mut p = params();
    p.stepover = 2.0;
    for start in [PassStart::Inside, PassStart::Outside] {
        let path = pocket_avoid_started_on(
            &band(&part),
            &p,
            &spec,
            &CornerRelief::default(),
            &side(start),
        )
        .expect("plan");
        let hits = avoid_hits(&path, &spec, AVOID_VERIFY_TOL).expect("verify");
        assert!(hits.is_empty(), "{start:?}: the clamp is touched: {hits:?}");
        let (up, _) = lifts(&path);
        eprintln!("clamp {start:?}: {up} lift(s), notes {:?}", path.notes);
        assert!(
            up >= 1,
            "{start:?}: the clamp splits the rings, so a link lifts"
        );
        assert_eq!(
            named_lifts(&path),
            up,
            "{start:?}: a lift with no named reason: {:?}",
            path.notes
        );
        assert!(
            path.notes
                .iter()
                .any(|n| n.starts_with("link: ") && n.contains("fixture")),
            "{start:?}: the fixture is not named: {:?}",
            path.notes
        );
    }
}
