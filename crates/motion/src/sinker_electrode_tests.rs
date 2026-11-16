//! #547: the cavity read and the electrode derivation, pinned - a
//! rectangular cavity's faces yield its electrode, and every degenerate
//! face set is refused by its own name. The seeded fuzz runs are in
//! `sinker_electrode_fuzz`.
//!
//! Provenance: our own SYNTHETIC fixtures, built in code.

use crate::geo2d::Point2;
use crate::geometry::{Point3, Triangle};
use crate::sinker_cavity::{cavity_of, Cavity, PickedFace};
use crate::sinker_electrode::{derive, rect_of, standing_in, ElectrodeSpec};

pub(crate) fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

pub(crate) fn face(label: &str, normal: Point3, outer: Vec<Point3>) -> PickedFace {
    PickedFace { label: label.into(), normal, curved: false, open: false, outer }
}

/// The wall standing on the floor edge `a -> b` (counter-clockwise
/// ring), `floor` to `top`, facing into the cavity.
pub(crate) fn wall(label: &str, a: (f64, f64), b: (f64, f64), floor: f64, top: f64) -> PickedFace {
    let len = (b.0 - a.0).hypot(b.1 - a.1);
    let n = p(-(b.1 - a.1) / len, (b.0 - a.0) / len, 0.0);
    face(label, n, vec![p(a.0, a.1, floor), p(b.0, b.1, floor), p(b.0, b.1, top), p(a.0, a.1, top)])
}

/// A rectangular cavity's five faces: the floor first, then its walls.
pub(crate) fn rect_cavity(lo: (f64, f64), hi: (f64, f64), floor: f64, top: f64) -> Vec<PickedFace> {
    let ring = [(lo.0, lo.1), (hi.0, lo.1), (hi.0, hi.1), (lo.0, hi.1)];
    let mut out = vec![face("floor", p(0.0, 0.0, 1.0), ring.iter().map(|c| p(c.0, c.1, floor)).collect())];
    for i in 0..4 {
        out.push(wall(&format!("wall {i}"), ring[i], ring[(i + 1) % 4], floor, top));
    }
    out
}

pub(crate) fn spec(undersize: f64) -> ElectrodeSpec {
    ElectrodeSpec { undersize, orbit: None, stub: 10.0, pickup: 15.0 }
}

fn why(faces: &[PickedFace]) -> String {
    match cavity_of(faces).and_then(|c| derive(&c, &spec(0.1))) {
        Ok(e) => panic!("expected a refusal, got {e:?}"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn a_rectangular_cavitys_faces_yield_its_electrode() {
    let faces = rect_cavity((-4.0, -10.0), (16.0, 2.0), 14.0, 20.0);
    let c = cavity_of(&faces).expect("one cavity");
    assert_eq!((c.floor_z, c.top_z, c.walls), (14.0, 20.0, 4));
    let e = derive(&c, &spec(0.3)).expect("an electrode");
    assert!((e.face.0 - 19.4).abs() < 1e-12 && (e.face.1 - 11.4).abs() < 1e-12, "{:?}", e.face);
    assert_eq!((e.center, e.cavity, e.depth, e.length, e.pickup), ((6.0, -4.0), (20.0, 12.0), 6.0, 16.0, 15.0));
    assert_eq!(e.outline.len(), 4, "the offset outline is the electrode's section: {:?}", e.outline);
    let t = e.tool();
    assert!(t.is_sane() && t.torch_refusal().is_none());
    assert_eq!((t.diameter, t.width, t.flute_len, t.shank_len), (e.face.0, e.face.1, 16.0, 15.0));

    // An orbit radius is taken off the electrode as well (#544's hook).
    let o = derive(&c, &ElectrodeSpec { orbit: Some(0.2), ..spec(0.3) }).expect("an orbiting electrode");
    assert!((o.face.0 - 19.0).abs() < 1e-12 && (o.face.1 - 11.0).abs() < 1e-12, "{:?}", o.face);
    assert_eq!((o.inset, o.orbit), (0.5, 0.2));
    // A stated zero undersize is the cavity's own size.
    assert_eq!(derive(&c, &spec(0.0)).expect("net size").face, (20.0, 12.0));
}

#[test]
fn a_cavity_reads_whatever_the_winding_split_or_order() {
    // The floor ring clockwise with a collinear extra vertex, one wall in
    // two pieces, the walls' rings reversed, the faces shuffled.
    let (floor, top) = (-3.0, 5.0);
    let mut faces = vec![
        wall("s1", (0.0, 0.0), (4.0, 0.0), floor, top),
        wall("n", (10.0, 8.0), (0.0, 8.0), floor, top),
        face("floor", p(0.0, 0.0, 1.0), vec![p(0.0, 8.0, floor), p(10.0, 8.0, floor), p(10.0, 0.0, floor), p(4.0, 0.0, floor), p(0.0, 0.0, floor)]),
        wall("e", (10.0, 0.0), (10.0, 8.0), floor, 7.5),
        wall("s2", (4.0, 0.0), (10.0, 0.0), floor, top),
        wall("w", (0.0, 8.0), (0.0, 0.0), floor, top),
    ];
    for f in faces.iter_mut().filter(|f| f.label != "floor") {
        f.outer.reverse();
    }
    let c = cavity_of(&faces).expect("one cavity");
    assert_eq!((c.walls, c.floor_z, c.top_z), (5, floor, 7.5), "the mouth is the highest wall");
    let e = derive(&c, &spec(0.5)).expect("an electrode");
    assert_eq!((e.face, e.center), ((9.0, 7.0), (5.0, 4.0)));
}

#[test]
fn every_degenerate_face_set_is_refused_by_its_own_name() {
    let good = || rect_cavity((0.0, 0.0), (20.0, 12.0), 0.0, 6.0);
    let with = |edit: &dyn Fn(&mut Vec<PickedFace>)| {
        let mut f = good();
        edit(&mut f);
        why(&f)
    };
    assert!(why(&[]).contains("no faces are picked"));
    assert!(with(&|f| drop(f.remove(0))).contains("no floor is picked"));
    assert!(with(&|f| f.truncate(1)).contains("only the floor"));
    assert!(with(&|f| drop(f.remove(2))).contains("do not close around the floor"), "an open loop of walls");
    assert!(with(&|f| f[1].curved = true).contains("curved wall"));
    assert!(with(&|f| f[1].open = true).contains("open loop"));
    assert!(with(&|f| f[3].outer.truncate(2)).contains("open loop"));
    assert!(with(&|f| f[2].outer[1].y = f64::NAN).contains("not a finite number"));
    assert!(with(&|f| f[0].normal.z = f64::INFINITY).contains("not a finite number"));
    assert!(with(&|f| f[0].outer[0].x = 1e300).contains("outside the machinable range"));
    assert!(with(&|f| f[1].normal = p(0.0, 0.0, 0.0)).contains("no usable normal"));
    assert!(with(&|f| f[1].outer[2].y += 0.01).contains("is not planar"), "a non-planar wall");
    assert!(with(&|f| {
        let a = f[1].outer[0];
        f[1].outer = vec![a, a, a, a];
    })
    .contains("zero area"));
    assert!(with(&|f| f[0].normal = p(0.0, 0.0, -1.0)).contains("faces down"));
    assert!(with(&|f| f[0].outer[2].z += 0.5).contains("is not planar"), "a tilted floor's ring leaves the level plane");
    // A drafted wall: 5 degrees, its ring true to its own plane.
    assert!(with(&|f| {
        let t = 5.0_f64.to_radians();
        f[1].normal = p(0.0, t.cos(), t.sin());
        f[1].outer[2].y -= 6.0 * t.tan();
        f[1].outer[3].y -= 6.0 * t.tan();
    })
    .contains("leans 5.000 degrees"));
    // Faces from different cavities: a second floor, and a far wall.
    assert!(with(&|f| f.push(face("floor B", p(0.0, 0.0, 1.0), vec![p(40.0, 0.0, 0.0), p(50.0, 0.0, 0.0), p(50.0, 9.0, 0.0)])))
        .contains("2 floors are picked"));
    assert!(with(&|f| f.push(wall("wall B", (40.0, 0.0), (50.0, 0.0), 0.0, 6.0))).contains("face of a different cavity"));
    assert!(with(&|f| f.push(wall("deep", (0.0, 0.0), (20.0, 0.0), -8.0, -2.0))).contains("below the picked floor"));
    assert!(with(&|f| f.push(wall("low", (0.0, 0.0), (20.0, 0.0), 0.0, 0.0005))).contains("does not rise above the picked floor"));
    assert!(with(&|f| f[1].normal = p(0.0, -1.0, 0.0)).contains("faces away from it"), "a boss's side");
    assert!(with(&|f| f[1].outer[1].x = 12.0).contains("do not close around the floor"), "a wall short of its edge");
}

#[test]
fn a_cavity_that_is_no_rectangle_yields_a_body_electrode() {
    // #1152: a rectangle turned 30 degrees, an L, a chamfered corner -
    // each refused by name until the stock sim had an outline atom.
    let c = |outline: Vec<Point2>| Cavity { outline, floor_z: 0.0, top_z: 5.0, walls: 4 };
    let q = Point2::new;
    let (s, k) = (0.5_f64, 0.75_f64.sqrt());
    let turned: Vec<Point2> = [(0.0, 0.0), (20.0, 0.0), (20.0, 12.0), (0.0, 12.0)].iter().map(|(x, y)| q(x * k - y * s, x * s + y * k)).collect();
    let ell = vec![q(0.0, 0.0), q(20.0, 0.0), q(20.0, 6.0), q(8.0, 6.0), q(8.0, 12.0), q(0.0, 12.0)];
    let chamfered = vec![q(0.0, 0.0), q(19.0, 0.0), q(20.0, 1.0), q(20.0, 12.0), q(0.0, 12.0)];
    for (shape, area) in [(turned, 240.0), (ell.clone(), 168.0), (chamfered, 239.5)] {
        let cav = c(shape);
        let e = derive(&cav, &spec(0.1)).expect("a body electrode");
        assert!(e.body && e.ring == cav.outline && e.inset == 0.1 && e.length == 15.0, "{e:?}");
        // The section stands the inset inside every wall, and no further
        // than a mitre's reach: its area is the cavity's less a 0.1 band.
        let n = e.outline.len();
        let got = 0.5 * (0..n).map(|i| e.outline[i].x * e.outline[(i + 1) % n].y - e.outline[(i + 1) % n].x * e.outline[i].y).sum::<f64>();
        let ring_len: f64 = (0..cav.outline.len())
            .map(|i| {
                let (a, b) = (cav.outline[i], cav.outline[(i + 1) % cav.outline.len()]);
                (b.x - a.x).hypot(b.y - a.y)
            })
            .sum();
        assert!(got < area && (got - (area - 0.1 * ring_len)).abs() < 0.05, "area {got} of {area}, ring {ring_len}");
        // The record: an electrode with that section about the burn
        // point, its two sizes the section's extents.
        let t = e.tool();
        assert!(t.is_sane() && t.torch_refusal().is_none(), "{:?}", t.torch_refusal());
        assert_eq!(t.outline.len(), n);
        assert!((t.diameter - e.face.0).abs() < 1e-9 && (t.width - e.face.1).abs() < 1e-9, "{t:?}");
        let back = q(t.outline[0][0] + e.center.0, t.outline[0][1] + e.center.1);
        assert!((back.x - e.outline[0].x).abs() < 1e-9 && (back.y - e.outline[0].y).abs() < 1e-9);
    }
    // The L's electrode keeps its notch: the bounds' corner is no part
    // of it. (The box of its extents would burn the part there.)
    let e = derive(&c(ell), &spec(0.1)).expect("the L");
    assert_eq!(e.outline.len(), 6, "{:?}", e.outline);
    assert!(e.outline.iter().any(|p| (p.x - 7.9).abs() < 1e-5 && (p.y - 5.9).abs() < 1e-5), "the inner corner: {:?}", e.outline);
    assert!((e.face.0 - 19.8).abs() < 1e-5 && (e.face.1 - 11.8).abs() < 1e-5 && e.cavity == (20.0, 12.0), "{e:?}");
    // The body's own refusals, by name.
    let refusal = |cav: &Cavity, s: &ElectrodeSpec| derive(cav, s).expect_err("refused").to_string();
    let l = c(vec![q(0.0, 0.0), q(20.0, 0.0), q(20.0, 6.0), q(8.0, 6.0), q(8.0, 12.0), q(0.0, 12.0)]);
    assert!(refusal(&l, &spec(6.0)).contains("leaves no electrode"));
    // A dumbbell: two 10 x 10 rooms joined by a 2-wide neck.
    let bell = c(vec![
        q(0.0, 0.0), q(10.0, 0.0), q(10.0, 4.0), q(20.0, 4.0), q(20.0, 0.0), q(30.0, 0.0),
        q(30.0, 10.0), q(20.0, 10.0), q(20.0, 6.0), q(10.0, 6.0), q(10.0, 10.0), q(0.0, 10.0),
    ]);
    assert!(derive(&bell, &spec(0.5)).is_ok_and(|e| e.body));
    assert!(refusal(&bell, &spec(1.5)).contains("splits the electrode into 2 pieces"));
    // More points than the stock sim sweeps.
    let fine: Vec<Point2> = (0..1500).map(|i| {
        let a = f64::from(i) * std::f64::consts::TAU / 1500.0;
        q(40.0 * a.cos(), 40.0 * a.sin())
    }).collect();
    let why = refusal(&c(fine), &spec(0.1));
    assert!(why.contains("more than the 1024 the stock sim sweeps"), "{why}");
    let wide = c(vec![q(0.0, 0.0), q(600.0, 0.0), q(601.0, 1.0), q(601.0, 12.0), q(0.0, 12.0)]);
    assert!(refusal(&wide, &spec(0.1)).contains("larger than the 500 mm"));
    let short = ElectrodeSpec { stub: 1e-3, ..spec(0.1) };
    assert!(refusal(&Cavity { floor_z: -1e17, top_z: 1e17, ..l.clone() }, &short).contains("does not stand above"));
}

#[test]
fn a_cavity_the_box_atom_burns_is_judged_by_its_own_numbers() {
    let c = |outline: Vec<Point2>| Cavity { outline, floor_z: 0.0, top_z: 5.0, walls: 4 };
    let q = Point2::new;
    let refusal = |cav: &Cavity, s: &ElectrodeSpec| derive(cav, s).expect_err("refused").to_string();
    assert!(rect_of(&[q(0.0, 0.0), q(20.0, 0.0), q(20.0, 12.0), q(0.0, 12.0)], 1e-6).is_some());
    assert!(rect_of(&[q(0.0, 0.0), q(20.0, 0.0), q(20.0, 12.0), q(0.0, 12.000_01)], 1e-6).is_none(), "ten nanometres off is not a rectangle");
    // A bow-tie floor is self-intersecting.
    let bow = c(vec![q(0.0, 0.0), q(20.0, 12.0), q(20.0, 0.0), q(0.0, 12.0)]);
    assert!(refusal(&bow, &spec(0.1)).contains("self-intersecting"));
    // The inset consumes the cavity; the spec's own numbers.
    let rect = c(vec![q(0.0, 0.0), q(20.0, 0.0), q(20.0, 12.0), q(0.0, 12.0)]);
    assert!(refusal(&rect, &spec(6.0)).contains("leaves no electrode"));
    assert!(refusal(&rect, &ElectrodeSpec { orbit: Some(5.9), ..spec(0.2) }).contains("leaves no electrode"));
    assert!(refusal(&rect, &spec(f64::NAN)).contains("undersize"));
    assert!(refusal(&rect, &spec(-0.1)).contains("undersize"));
    assert!(refusal(&rect, &ElectrodeSpec { orbit: Some(f64::INFINITY), ..spec(0.1) }).contains("orbit radius"));
    assert!(refusal(&rect, &ElectrodeSpec { stub: 0.0, ..spec(0.1) }).contains("holder stub"));
    assert!(refusal(&rect, &ElectrodeSpec { pickup: -1.0, ..spec(0.1) }).contains("pickup offset"));
    let wide = c(vec![q(0.0, 0.0), q(600.0, 0.0), q(600.0, 12.0), q(0.0, 12.0)]);
    assert!(refusal(&wide, &spec(0.1)).contains("larger than the 500 mm"));
    assert!(refusal(&Cavity { top_z: 0.0, ..rect }, &spec(0.1)).contains("no depth"));
}

#[test]
fn the_model_standing_in_the_cavitys_column_is_found() {
    let c = cavity_of(&rect_cavity((0.0, 0.0), (20.0, 12.0), 0.0, 6.0)).expect("cavity");
    let e = derive(&c, &spec(0.1)).expect("electrode");
    let t = |a: Point3, b: Point3, c: Point3| Triangle::new(a, b, c);
    // The cavity's own floor and walls, and the part's top round it: clear.
    let own = vec![
        t(p(0.0, 0.0, 0.0), p(20.0, 0.0, 0.0), p(20.0, 12.0, 0.0)),
        t(p(0.0, 0.0, 0.0), p(20.0, 0.0, 0.0), p(20.0, 0.0, 6.0)),
        t(p(20.0, 0.0, 6.0), p(40.0, 0.0, 6.0), p(40.0, 12.0, 6.0)),
        t(p(5.0, 5.0, -4.0), p(6.0, 5.0, -4.0), p(5.0, 6.0, -1.0)),
        t(p(f64::NAN, 5.0, 3.0), p(6.0, 5.0, 3.0), p(5.0, 6.0, 3.0)),
    ];
    assert_eq!(standing_in(&e, &own), Ok(None));
    // A boss's top, a rib's side, an overhang across the mouth (no
    // corner of it inside the column).
    let boss = t(p(8.0, 4.0, 2.0), p(12.0, 4.0, 2.0), p(12.0, 8.0, 2.0));
    let rib = t(p(10.0, 0.0, 0.0), p(10.0, 12.0, 0.0), p(10.0, 12.0, 3.0));
    let over = t(p(-50.0, -50.0, 9.0), p(90.0, -50.0, 9.0), p(20.0, 90.0, 9.0));
    for obstacle in [boss, rib, over] {
        let mut all = own.clone();
        all.push(obstacle);
        let at = standing_in(&e, &all).expect("judged").expect("found");
        assert!(at.x > 0.0 && at.x < 20.0 && at.y > 0.0 && at.y < 12.0 && at.z > 0.0, "{at:?}");
    }
}

#[test]
fn a_body_electrodes_column_is_the_cavitys_outline_not_its_bounds() {
    // #1152: an L cavity. The part fills the bounds' upper right - it
    // stands beside the cavity, not in it.
    let q = Point2::new;
    let cav = Cavity { outline: vec![q(0.0, 0.0), q(20.0, 0.0), q(20.0, 6.0), q(8.0, 6.0), q(8.0, 12.0), q(0.0, 12.0)], floor_z: 0.0, top_z: 6.0, walls: 6 };
    let e = derive(&cav, &spec(0.1)).expect("the L's electrode");
    assert!(e.body);
    let t = |a: Point3, b: Point3, c: Point3| Triangle::new(a, b, c);
    let own = vec![
        // The floor, a wall, the notch's two walls.
        t(p(0.0, 0.0, 0.0), p(20.0, 0.0, 0.0), p(20.0, 6.0, 0.0)),
        t(p(0.0, 0.0, 0.0), p(20.0, 0.0, 0.0), p(20.0, 0.0, 6.0)),
        t(p(8.0, 6.0, 0.0), p(20.0, 6.0, 0.0), p(20.0, 6.0, 6.0)),
        t(p(8.0, 6.0, 0.0), p(8.0, 12.0, 0.0), p(8.0, 12.0, 6.0)),
        // The part's top over the notch: inside the bounds, outside the L.
        t(p(8.0, 6.0, 6.0), p(20.0, 6.0, 6.0), p(20.0, 12.0, 6.0)),
        t(p(8.0, 6.0, 6.0), p(20.0, 12.0, 6.0), p(8.0, 12.0, 6.0)),
        t(p(f64::NAN, 5.0, 3.0), p(6.0, 5.0, 3.0), p(5.0, 6.0, 3.0)),
    ];
    assert_eq!(standing_in(&e, &own), Ok(None), "the part round the L is not in its way");
    // A boss's top, an upright rib, an overhang with no corner in the
    // column - each in the L itself.
    let boss = t(p(2.0, 8.0, 2.0), p(6.0, 8.0, 2.0), p(6.0, 10.0, 2.0));
    let rib = t(p(4.0, 0.0, 0.0), p(4.0, 12.0, 0.0), p(4.0, 12.0, 3.0));
    let over = t(p(-50.0, -50.0, 9.0), p(90.0, -50.0, 9.0), p(20.0, 90.0, 9.0));
    for obstacle in [boss, rib, over] {
        let mut all = own.clone();
        all.push(obstacle);
        let at = standing_in(&e, &all).expect("judged").expect("found");
        assert!(at.z > 0.0 && (at.x < 8.0 || at.y < 6.0) && at.x > 0.0 && at.y > 0.0, "{at:?}");
    }
    // A rib in the notch (beside the cavity) is no obstacle.
    let mut beside = own.clone();
    beside.push(t(p(14.0, 7.0, 0.0), p(14.0, 11.0, 0.0), p(14.0, 11.0, 5.0)));
    assert_eq!(standing_in(&e, &beside), Ok(None));
}
