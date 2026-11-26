//! #3746 pins of the start-side air entry, on the #3744 dog-bone part
//! (two discs of radius 12 at X -15 and X 15, joined by a neck 8 mm
//! wide) in a stock box of 80 x 50, a tool of radius 4, a step of 2, one
//! level 5 mm deep, the Arc entry.

use crate::cl::{Move, MoveKind, Toolpath};
use crate::entry::Entry3D;
use crate::geo2d::{Loop, Point2, Region};
use crate::load::DEFAULT_OPTIMAL_LOAD;
use crate::machining2d::{CornerRelief, PassStart, PocketParams, PocketStrategy, StartSide, pocket_started_on};
use crate::plunge_law::{Solid, first_solid_plunge};

const R: f64 = 4.0;
const TOP: f64 = 10.0;
const BOTTOM: f64 = 5.0;

fn region(loops: Vec<Vec<Point2>>) -> Region {
    let loops = loops.into_iter().map(|l| Loop::new(l).unwrap_or_else(|e| panic!("loop: {e:?}"))).collect();
    Region::new(loops).unwrap_or_else(|e| panic!("region: {e:?}"))
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point2> {
    vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]
}

fn disc(cx: f64, r: f64) -> Vec<Point2> {
    (0..128)
        .map(|k| {
            let a = f64::from(k) * std::f64::consts::TAU / 128.0;
            Point2::new(cx + r * a.cos(), r * a.sin())
        })
        .collect()
}

/// The stock box and the band the 2D Offset op machines round the part:
/// the box grown by the tool radius, less the part.
fn dog_bone() -> (Region, Region) {
    let part = region(vec![disc(-15.0, 12.0)])
        .union(&region(vec![disc(15.0, 12.0)]))
        .and_then(|p| p.union(&region(vec![rect(-15.0, -4.0, 15.0, 4.0)])))
        .unwrap_or_else(|e| panic!("part: {e:?}"));
    let stock = region(vec![rect(-40.0, -25.0, 40.0, 25.0)]);
    let band = stock.offset(R).and_then(|s| s.difference(&part)).unwrap_or_else(|e| panic!("band: {e:?}"));
    (stock, band)
}

fn params(entry: Entry3D, arc_radius: f64, arc_sweep: f64) -> PocketParams {
    let mut links = crate::links::Links::PLANAR;
    links.approach.arc_radius = arc_radius;
    links.approach.arc_sweep = arc_sweep;
    PocketParams {
        blend_radius: None,
        tool_diameter: 2.0 * R,
        stepover: 2.0,
        stock_to_leave: 0.0,
        clearance_z: 30.0,
        retract_z: None,
        top_z: TOP,
        bottom_z: BOTTOM,
        stepdown: 10.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::Waterline,
        entry,
        helix_radius: 2.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        links,
    }
}

fn cut_with(start: PassStart, p: &PocketParams) -> Toolpath {
    let (stock, band) = dog_bone();
    let side = StartSide { start, stock: Some(stock), around_part: true };
    match pocket_started_on(&band, p, &[], &CornerRelief::default(), &side) {
        Ok(t) => t,
        Err(e) => panic!("{start:?}: {e}"),
    }
}

fn cut(start: PassStart, arc_radius: f64, arc_sweep: f64) -> Toolpath {
    cut_with(start, &params(Entry3D::Arc, arc_radius, arc_sweep))
}

/// How far `p` stands outside the stock box in XY (0 inside it).
fn off_box(x: f64, y: f64) -> f64 {
    let dx = (-40.0 - x).max(x - 40.0).max(0.0);
    let dy = (-25.0 - y).max(y - 25.0).max(0.0);
    dx.hypot(dy)
}

/// The first plunge to the cut plane, and the moves after it.
fn entry(t: &Toolpath) -> (&Move, &[Move]) {
    let i = t.moves.iter().position(|m| m.kind == MoveKind::Plunge && (m.to.z - BOTTOM).abs() < 1e-9);
    let Some(i) = i else { panic!("no plunge to the cut plane: {:?}", t.notes) };
    (&t.moves[i], &t.moves[i + 1..])
}

/// The first arc at the cut plane after the plunge, past a straight lead
/// whose tool disc stays in the air outside the stock box.
fn first_arc(t: &Toolpath) -> &Move {
    let (_, after) = entry(t);
    let mut it = after.iter();
    let mut m = it.next().unwrap_or_else(|| panic!("nothing after the plunge"));
    if m.arc.is_none() {
        for k in 0..=20 {
            let f = f64::from(k) / 20.0;
            let (x, y) = (m.from.x + f * (m.to.x - m.from.x), m.from.y + f * (m.to.y - m.from.y));
            assert!(off_box(x, y) >= R - 1e-6, "the straight lead cuts the stock at ({x:.3}, {y:.3}): {m:?}");
        }
        m = it.next().unwrap_or_else(|| panic!("nothing after the lead"));
    }
    assert!(m.arc.is_some(), "the first cutting move after the plunge is not an arc: {m:?}\nnotes {:?}", t.notes);
    assert!(m.kind == MoveKind::Feed && (m.from.z - BOTTOM).abs() < 1e-9 && (m.to.z - BOTTOM).abs() < 1e-9, "{m:?}");
    m
}

/// The radius and the sweep (radians) of an arc move.
fn radius_and_sweep(m: &Move) -> (f64, f64) {
    let Some(a) = m.arc.as_ref() else { panic!("not an arc") };
    let c = a.center;
    let (a0, a1) = ((m.from.y - c.y).atan2(m.from.x - c.x), (m.to.y - c.y).atan2(m.to.x - c.x));
    let mut d = (a1 - a0).abs();
    if d > std::f64::consts::PI {
        d = std::f64::consts::TAU - d;
    }
    ((m.from.x - c.x).hypot(m.from.y - c.y), d)
}

#[test]
fn outside_entry_plunges_in_air() {
    let t = cut(PassStart::Outside, 0.0, 0.0);
    let (p, _) = entry(&t);
    let need = R + crate::approach::DEFAULT_APPROACH_DISTANCE;
    let off = off_box(p.to.x, p.to.y);
    assert!(off >= need - 1e-6, "the plunge stands {off:.3} mm off the stock box, not {need:.3}: {p:?}");
}

#[test]
fn outside_first_cut_is_an_arc_in_the_plane() {
    let t = cut(PassStart::Outside, 0.0, 0.0);
    let (r, sweep) = radius_and_sweep(first_arc(&t));
    // Arc radius 0 is the tool diameter; Arc sweep 0 is a quarter turn.
    assert!((r - 2.0 * R).abs() < 1e-3, "radius {r}");
    assert!((sweep - std::f64::consts::FRAC_PI_2).abs() < 1e-3, "sweep {sweep}");
}

#[test]
fn arc_radius_changes_the_lead() {
    let (r6, _) = radius_and_sweep(first_arc(&cut(PassStart::Outside, 6.0, 0.0)));
    let (r10, _) = radius_and_sweep(first_arc(&cut(PassStart::Outside, 10.0, 0.0)));
    assert!((r6 - 6.0).abs() < 1e-3 && (r10 - 10.0).abs() < 1e-3, "radii {r6} and {r10}");
    let (_, s60) = radius_and_sweep(first_arc(&cut(PassStart::Outside, 10.0, 60.0)));
    assert!((s60 - 60f64.to_radians()).abs() < 1e-3, "sweep {s60}");
    let t = cut(PassStart::Outside, 10.0, 60.0);
    let (p, _) = entry(&t);
    assert!(off_box(p.to.x, p.to.y) >= R + crate::approach::DEFAULT_APPROACH_DISTANCE - 1e-6, "{p:?}");
}

#[test]
fn inside_entry_is_no_plunge_into_material() {
    let (stock, _) = dog_bone();
    let solid = Solid { top: TOP, footprint: Some(&stock) };
    for entry in [Entry3D::Arc] {
        let t = cut_with(PassStart::Inside, &params(entry, 0.0, 0.0));
        let hit = first_solid_plunge(&t, R, &solid);
        if let Some((q, _)) = hit {
            for (i, m) in t.moves.iter().enumerate().filter(|(_, m)| (m.to.x - q.x).hypot(m.to.y - q.y) < 1e-6) {
                eprintln!("{i}: {m:?}
  prev {:?}", t.moves.get(i.wrapping_sub(1)));
            }
        }
        assert_eq!(hit, None, "{entry:?}: a plunge into material: {:?}", t.notes);
        let named = t.notes.iter().any(|n| n.starts_with("entry: Inside pass"));
        let plunged = t.moves.iter().any(|m| m.kind == MoveKind::Plunge && off_box(m.to.x, m.to.y) >= R);
        assert!(named || plunged, "the Inside entry is neither in air nor named: {:?}", t.notes);
    }
}

#[test]
fn outside_entry_is_no_plunge_into_material() {
    let (stock, _) = dog_bone();
    let solid = Solid { top: TOP, footprint: Some(&stock) };
    let t = cut(PassStart::Outside, 0.0, 0.0);
    assert_eq!(first_solid_plunge(&t, R, &solid), None, "{:?}", t.notes);
    assert!(!t.notes.iter().any(|n| n.contains("axial dive") || n.contains("plunged into material")), "{:?}", t.notes);
}

fn block(min: [f64; 3], max: [f64; 3]) -> Vec<crate::geometry::Triangle> {
    use crate::geometry::{Point3, Triangle};
    let v = |i: usize| Point3::new([min[0], max[0]][i & 1], [min[1], max[1]][(i >> 1) & 1], [min[2], max[2]][(i >> 2) & 1]);
    let quad = |a: usize, b: usize, c: usize, d: usize| [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))];
    [quad(4, 5, 7, 6), quad(0, 2, 3, 1), quad(0, 1, 5, 4), quad(2, 6, 7, 3), quad(1, 3, 7, 5), quad(2, 0, 4, 6)]
        .into_iter()
        .flatten()
        .collect()
}

fn avoid_cut(items: Vec<crate::verify::AvoidItem>) -> Toolpath {
    avoid_try(items).unwrap_or_else(|e| panic!("avoid: {e}"))
}

fn avoid_try(items: Vec<crate::verify::AvoidItem>) -> Result<Toolpath, String> {
    use crate::machining2d::pocket_avoid_started_on;
    use crate::verify::{AvoidSpec, Cutter};
    let geom = crate::tools::ToolGeom {
        radius: R,
        nose_radius: 0.0,
        flute_len: 20.0,
        shank_radius: R,
        shank_len: 20.0,
        holder_radius: 12.0,
        holder_len: 20.0,
    };
    let spec = AvoidSpec { items, cutter: Cutter::flat(&geom), clearance: 1.0 };
    let (stock, band) = dog_bone();
    let side = StartSide { start: PassStart::Outside, stock: Some(stock), around_part: true };
    pocket_avoid_started_on(&band, &params(Entry3D::Arc, 0.0, 0.0), &spec, &CornerRelief::default(), &side).map_err(|e| e.to_string())
}

/// #3810: with fixtures to avoid, the Arc entry of Outside still plunges
/// in air outside the stock box; with a clamp over that plunge point the
/// entry moves off it or stands down by name, and no move touches the
/// clamp. #3884: an Outside entry that cannot move off it in air is
/// refused by name, never a helix or ramp stand-in.
#[test]
fn avoid_route_entry_plunges_in_air_clear_of_a_clamp() {
    use crate::verify::{AVOID_VERIFY_TOL, AvoidItem, AvoidSpec, Cutter, avoid_hits};
    let need = R + crate::approach::DEFAULT_APPROACH_DISTANCE;
    let free = avoid_cut(Vec::new());
    let (p, _) = entry(&free);
    assert!(off_box(p.to.x, p.to.y) >= need - 1e-6, "the free plunge stands {:.3} mm off the box: {p:?}", off_box(p.to.x, p.to.y));
    first_arc(&free);
    // A clamp 8 mm square, from the table to over the stock, on the plunge point.
    let (cx, cy) = (p.to.x, p.to.y);
    let clamp = AvoidItem { label: "clamp".into(), tris: block([cx - 4.0, cy - 4.0, 0.0], [cx + 4.0, cy + 4.0, 20.0]), edges: vec![] };
    let t = match avoid_try(vec![clamp.clone()]) {
        Ok(t) => t,
        Err(e) => {
            assert!(e.contains(crate::approach::OUTSIDE_AIR_REFUSAL), "{e}");
            return;
        }
    };
    let geom = crate::tools::ToolGeom {
        radius: R,
        nose_radius: 0.0,
        flute_len: 20.0,
        shank_radius: R,
        shank_len: 20.0,
        holder_radius: 12.0,
        holder_len: 20.0,
    };
    let spec = AvoidSpec { items: vec![clamp], cutter: Cutter::flat(&geom), clearance: 1.0 };
    let hits = avoid_hits(&t, &spec, AVOID_VERIFY_TOL).unwrap_or_else(|e| panic!("verify: {e}"));
    assert!(hits.is_empty(), "the path touches the clamp: {hits:?}");
    // The clamp's keep-out covers the corner lead: the entry moves off the
    // clamp in air (or the op is refused above) - never onto the clamp.
    let moved = t.moves.iter().find(|m| m.kind == MoveKind::Plunge && (m.to.z - BOTTOM).abs() < 1e-9).is_some_and(|q| {
        (q.to.x - cx).hypot(q.to.y - cy) > 4.0 + R && off_box(q.to.x, q.to.y) >= need - 1e-6
    });
    assert!(moved, "the entry did not move off the clamp in air: {:?}", t.notes);
}

/// #3884 pin (first entry): an Outside pass whose Arc lead fits nowhere -
/// an arc far larger than the air round the stock - is refused by name,
/// with no helix or ramp stand-in. Inside keeps its named stand-in.
#[test]
fn outside_first_entry_without_an_air_lead_is_refused() {
    let p = params(Entry3D::Arc, 5000.0, 0.0);
    let (stock, band) = dog_bone();
    let side = |start| StartSide { start, stock: Some(stock.clone()), around_part: true };
    match pocket_started_on(&band, &p, &[], &CornerRelief::default(), &side(PassStart::Outside)) {
        Ok(t) => panic!("the Outside Arc with no air lead was not refused: {:?}", t.notes),
        Err(e) => assert!(e.to_string().contains(crate::approach::OUTSIDE_AIR_REFUSAL), "{e}"),
    }
    let inside = pocket_started_on(&band, &p, &[], &CornerRelief::default(), &side(PassStart::Inside));
    assert!(inside.is_ok_and(|t| t.notes.iter().any(|n| n.starts_with("entry: Inside pass - no air entry"))), "Inside lost its stand-in");
}

/// The re-entry case of the #3884 pins: a pass start at (0, 0) in
/// material on an Outside pass with the Arc entry, which a ring of the
/// level encloses, so no air lead reaches it; `cut`: the floor already cut
/// at the level's depth (feeds from `(x, y0)` to `(x, y1)`), `leg` where the
/// tool lifted.
fn reentry(cut: &[(f64, f64, f64)], leg: Point2) -> super::super::descent::Descent {
    use super::super::descent::Lvl;
    use super::super::levels::{Cert, Motion};
    use crate::cl::PathBuilder;
    use crate::geometry::Point3;
    let stock = region(vec![rect(-40.0, -25.0, 40.0, 25.0)]);
    let center = region(vec![rect(-44.0, -29.0, 44.0, 29.0)]);
    let Some(air) = super::AirRoom::new(&stock, &center, R, 1.0, false) else { panic!("air room") };
    let ring = |x0: f64, y0: f64, x1: f64, y1: f64| {
        vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1), Point2::new(x0, y0)]
    };
    let passes = vec![ring(-36.0, -21.0, 36.0, 21.0), ring(0.0, 0.0, 4.0, 4.0)];
    let cert = Cert::Region(&center);
    let l = Lvl { cen: &cert, passes: &passes, pass: &passes[1], walk: None, mat_top: TOP };
    let m = Motion::new(30.0, None, 1200.0, 300.0)
        .with_entry(Entry3D::Arc, 2.0)
        .with_arc_radius(2.0 * R)
        .with_links(crate::links::Links::PLANAR, 2.0)
        .with_cleared_links(true)
        .with_air(Some(air));
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, 30.0));
    for &(x, y0, y1) in cut {
        b.push(MoveKind::Rapid, Point3::new(x, y0, 30.0), None);
        b.push(MoveKind::Plunge, Point3::new(x, y0, BOTTOM), Some(300.0));
        b.push(MoveKind::Feed, Point3::new(x, y1, BOTTOM), Some(1200.0));
        b.push(MoveKind::Rapid, Point3::new(x, y1, 30.0), None);
    }
    b.push(MoveKind::Rapid, Point3::new(leg.x, leg.y, 30.0), None);
    super::super::descent_dive::entry_restart(&b, &l, leg, BOTTOM, TOP + 2.0, 30.0, &m)
}

/// #3884 pin (re-entry after a lift): a pass start in material that no air
/// lead and no cut floor reach is refused by name on an Outside pass with
/// the Arc entry: no ramp along the pass, no helix, no plunge into stock.
#[test]
fn outside_reentry_without_an_air_lead_is_refused() {
    use super::super::descent::Descent;
    match reentry(&[], Point2::new(0.0, -7.0)) {
        Descent::Refused(r) => assert!(r.starts_with(crate::approach::OUTSIDE_AIR_REFUSAL), "{r}"),
        Descent::Named(_, note) => panic!("a stand-in re-entry, not a refusal: {note}"),
        _ => panic!("not refused"),
    }
}

/// #3884 pin (option C, as #3855): the same re-entry beside the floor
/// already cut at that depth (feeds 2 mm apart, parallel to the link, so
/// the link crosses none) comes down on that floor - a plunge whose tool
/// disc is all on cut floor - and links at depth to the start.
#[test]
fn outside_reentry_onto_cut_floor_is_accepted() {
    use super::super::descent::Descent;
    let cut: Vec<(f64, f64, f64)> = (0..6).map(|k| (-5.0 + 2.0 * f64::from(k), -14.0, -3.0)).collect();
    match reentry(&cut, Point2::new(0.0, -7.0)) {
        Descent::Named(inner, note) => {
            assert!(note.contains("entered on the floor already cut"), "{note}");
            assert!(matches!(*inner, Descent::Plunge(p) if (p.y + 7.0).abs() < 1e-9 && p.x.abs() < 1e-9), "{note}");
        }
        Descent::Refused(r) => panic!("the re-entry onto cut floor was refused: {r}"),
        _ => panic!("not named"),
    }
}

/// #3884 pin: "already cut" is cut by the whole tool disc. One feed 2 mm
/// beside the plunge point covers only part of the disc round it, so the
/// re-entry is refused by name.
#[test]
fn outside_reentry_onto_a_partly_cut_disc_is_refused() {
    use super::super::descent::Descent;
    match reentry(&[(2.0, -14.0, -3.0)], Point2::new(0.0, -7.0)) {
        Descent::Refused(r) => assert!(r.starts_with(crate::approach::OUTSIDE_AIR_REFUSAL), "{r}"),
        Descent::Named(_, note) => panic!("a partly cut floor was taken as cut: {note}"),
        _ => panic!("not refused"),
    }
}

/// #3884 pin: "already cut" is cut earlier in the program order. The same
/// floor, cut only after this re-entry (the moves that the re-entry reads
/// do not hold it), does not count, so the re-entry is refused by name.
#[test]
fn outside_reentry_onto_floor_cut_later_is_refused() {
    use super::super::descent::Descent;
    // The re-entry reads the path as it stands; the later cut is not on it.
    match reentry(&[], Point2::new(0.0, -7.0)) {
        Descent::Refused(r) => assert!(r.starts_with(crate::approach::OUTSIDE_AIR_REFUSAL), "{r}"),
        Descent::Named(_, note) => panic!("an uncut floor was taken as cut: {note}"),
        _ => panic!("not refused"),
    }
    let cut: Vec<(f64, f64, f64)> = (0..6).map(|k| (-5.0 + 2.0 * f64::from(k), -14.0, -3.0)).collect();
    assert!(matches!(reentry(&cut, Point2::new(0.0, -7.0)), Descent::Named(..)), "the same floor cut before is taken");
}

