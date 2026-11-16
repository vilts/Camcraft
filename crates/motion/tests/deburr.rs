//! M2.5 E B deburr law tests: what the cone-ruled edge break kernel
//! promises on its own ground - the rim of an open mesh rides, pencil's
//! concavity gate mirrored keeps convex creases and refuses concave
//! ones, the window clips along the edge's own line, a weld gone wrong
//! (flips, manifold keys) is skipped and SPOKEN, an edge nothing
//! exposes answers Empty by name - and the congruence that ties it to
//! `chamfer.rs`: a 90-deg cone on a horizontal-top/vertical-wall
//! corner lands on the wall loop's XY and `corner_z`, byte law.
//!
//! The envelope VERDICTS (what the cone's flank does near neighboring
//! faces) ride sim's Mode::Envelope lane, not this file - deburr
//! states the ride, verify states the reach.
//!
//! Gate map (M4.4): covers camcraft_motion::finishing3d (the deburr
//! kernel, the chain/crease law it rides) and
//! camcraft_motion::machining2d::chamfer (the congruence half).
//! Lane shape (M4.4): every law here runs in seconds; no ignored
//! verdict lives in this file.

use camcraft_motion::cl::MoveKind;
use camcraft_motion::finishing3d::{deburr, DeburrParams};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::{chamfer, ChamferParams, GenError};

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A 90-deg cone taking a 2-wide break above the z-window; the
/// rapids plane sits where every op's does.
fn params(top_z: f64, bottom_z: f64) -> DeburrParams {
    DeburrParams {
        width: 2.0,
        tip_angle_deg: 90.0,
        clearance_z: 20.0,
        top_z,
        bottom_z,
        feed: 500.0,
        plunge_feed: 100.0,
    }
}

/// The tip stands `width / tan(half-angle)` under the edge: at 90
/// degrees of included angle the half-angle is 45, tan is 1, and the
/// drop is the width itself. Stated as a number once, here.
const DROP: f64 = 2.0;

/// A closed box: x0..x1, y0..y1, z0..z1, every triangle wound
/// outward (the normals are the cone's premise - the crease gate
/// reads material under, never above).
fn box_mesh(x0: f64, y0: f64, x1: f64, y1: f64, z0: f64, z1: f64) -> Vec<Triangle> {
    let q = Triangle::new;
    vec![
        // bottom (-z)
        q(p3(x0, y0, z0), p3(x1, y1, z0), p3(x1, y0, z0)),
        q(p3(x0, y0, z0), p3(x0, y1, z0), p3(x1, y1, z0)),
        // top (+z)
        q(p3(x0, y0, z1), p3(x1, y0, z1), p3(x1, y1, z1)),
        q(p3(x0, y0, z1), p3(x1, y1, z1), p3(x0, y1, z1)),
        // y0 wall (-y)
        q(p3(x0, y0, z0), p3(x1, y0, z0), p3(x1, y0, z1)),
        q(p3(x0, y0, z0), p3(x1, y0, z1), p3(x0, y0, z1)),
        // y1 wall (+y)
        q(p3(x0, y1, z0), p3(x1, y1, z1), p3(x1, y1, z0)),
        q(p3(x0, y1, z0), p3(x0, y1, z1), p3(x1, y1, z1)),
        // x0 wall (-x)
        q(p3(x0, y0, z0), p3(x0, y0, z1), p3(x0, y1, z1)),
        q(p3(x0, y0, z0), p3(x0, y1, z1), p3(x0, y1, z0)),
        // x1 wall (+x)
        q(p3(x1, y0, z0), p3(x1, y1, z1), p3(x1, y0, z1)),
        q(p3(x1, y0, z0), p3(x1, y1, z0), p3(x1, y1, z1)),
    ]
}

/// A flat 8x4 patch of two coplanar quads split at x = 4: the seam
/// is a face meeting, not an edge - only the boundary rim rides.
fn split_patch() -> Vec<Triangle> {
    let mut v = Vec::new();
    for x0 in [0.0, 4.0] {
        v.push(Triangle::new(p3(x0, 0.0, 0.0), p3(x0 + 4.0, 0.0, 0.0), p3(x0 + 4.0, 4.0, 0.0)));
        v.push(Triangle::new(p3(x0, 0.0, 0.0), p3(x0 + 4.0, 4.0, 0.0), p3(x0, 4.0, 0.0)));
    }
    v
}

/// A vertical curtain: one flat wall x0..x1, z0..z1 at y = 0. Its
/// rim runs all four sides of it.
fn curtain(x0: f64, x1: f64, z0: f64, z1: f64) -> Vec<Triangle> {
    vec![
        Triangle::new(p3(x0, 0.0, z0), p3(x1, 0.0, z0), p3(x1, 0.0, z1)),
        Triangle::new(p3(x0, 0.0, z0), p3(x1, 0.0, z1), p3(x0, 0.0, z1)),
    ]
}

/// Extrude a CCW (y, z) polygon along +x into a closed solid: side
/// quads carry the cross-section's outward normals, caps close it.
/// The C-channel fixture's home.
fn prism(poly: &[(f64, f64)], x_len: f64) -> Vec<Triangle> {
    let v = |side: f64, (y, z): (f64, f64)| p3(side, y, z);
    let mut t = Vec::new();
    for i in 0..poly.len() {
        let j = (i + 1) % poly.len();
        let (a, b) = (v(0.0, poly[i]), v(0.0, poly[j]));
        let (c, d) = (v(x_len, poly[j]), v(x_len, poly[i]));
        t.push(Triangle::new(a, b, c));
        t.push(Triangle::new(a, c, d));
    }
    for side in [0.0, x_len] {
        for i in 1..poly.len() - 1 {
            let (a, b, c) = (v(side, poly[0]), v(side, poly[i]), v(side, poly[i + 1]));
            t.push(if side > 0.0 {
                Triangle::new(a, b, c)
            } else {
                Triangle::new(a, c, b)
            });
        }
    }
    t
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
    .unwrap()])
    .unwrap()
}

/// The cut points of a path: where the tip actually stood (rapids
/// carry no cut; the plunge lands the tip, the feeds walk it).
fn cut_points(t: &camcraft_motion::cl::Toolpath) -> Vec<Point3> {
    t.moves.iter().filter(|m| m.kind != MoveKind::Rapid).map(|m| m.to).collect()
}

/// Quantize to microns so two kernels' last-bit differences (a
/// `w * tan` and a `w / tan` at 45 degrees) cannot masquerade as
/// geometric disagreement - and cannot hide real ones.
fn keys(pts: &[Point3]) -> Vec<(i64, i64, i64)> {
    let mut k: Vec<(i64, i64, i64)> = pts
        .iter()
        .map(|p| {
            (
                (p.x * 1e6).round() as i64,
                (p.y * 1e6).round() as i64,
                (p.z * 1e6).round() as i64,
            )
        })
        .collect();
    k.sort_unstable();
    k
}

fn err<T>(r: Result<T, GenError>) -> String {
    match r {
        Ok(_) => panic!("a door was meant to refuse"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn rim_of_an_open_mesh_rides_the_whole_loop() {
    // Two coplanar quads: the seam and the diagonals are face
    // meetings, not edges - the 6 boundary chords are the mesh's own
    // rim. One closed circuit, every tip standing DROP below.
    let t = deburr(&split_patch(), &params(0.0, -4.0)).expect("a rim is an edge");
    assert!(t.notes.is_empty(), "nothing was skipped in silence: {:?}", t.notes);
    assert!(t.is_chained());
    let cut = cut_points(&t);
    assert_eq!(cut.len(), 7, "6 chords, start repeated to close: {cut:?}");
    assert_eq!(cut.first().unwrap().x, cut.last().unwrap().x, "the run closed on its start");
    for c in &cut {
        assert!((c.z + DROP).abs() < 1e-9, "a rim tip off the drop plane: {c:?}");
    }
    assert_eq!(t.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count(), 1);
    assert_eq!(t.moves.last().unwrap().kind, MoveKind::Rapid, "the ride lifts to clearance");
    assert_eq!(t.moves.last().unwrap().to.z, 20.0);
}

#[test]
fn convex_creases_all_ride_a_closed_box_breaks_every_level_edge() {
    // Every dihedral of a box is convex: the level creases ride at the
    // two level lines z - DROP, around the corners. The four vertical
    // corner creases climb steeper than the cone's flank - a vertical
    // cone riding one would countersink the corner down to the window
    // floor (#303) - so they are skipped, and the path says so.
    let f = box_mesh(0.0, 0.0, 40.0, 20.0, -10.0, 0.0);
    let t = deburr(&f, &params(2.0, -12.0)).expect("a box is all burr");
    assert_eq!(t.notes.len(), 1, "only the steep skip speaks: {:?}", t.notes);
    assert!(t.notes[0].contains("4 edges climb steeper"), "{:?}", t.notes);
    let corners = [(0.0, 0.0), (40.0, 0.0), (40.0, 20.0), (0.0, 20.0)];
    for c in cut_points(&t) {
        assert!(
            corners.iter().any(|(x, y)| (c.x - x).abs() < 1e-9 && (c.y - y).abs() < 1e-9),
            "a tip stood off the corner lines: {c:?}"
        );
        assert!(
            (c.z - (0.0 - DROP)).abs() < 1e-9 || (c.z - (-10.0 - DROP)).abs() < 1e-9,
            "a crease tip off the level lines: {c:?}"
        );
    }
    // Both level lines and all four corners made it into the path.
    for (x, y) in corners {
        for z in [-DROP, -10.0 - DROP] {
            assert!(
                cut_points(&t).iter().any(|c| (c.x - x).abs() < 1e-9 && (c.y - y).abs() < 1e-9 && (c.z - z).abs() < 1e-9),
                "the corner ({x},{y}) level {z} never rode"
            );
        }
    }
    // No cut move runs up a corner line: every feed is level.
    for m in t.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
        assert!((m.from.z - m.to.z).abs() < 1e-9, "a feed climbed a vertical corner: {m:?}");
    }
    // The declared breaks are the two level loops at the edges' own Z.
    assert_eq!(t.breaks.len(), 2, "{:?}", t.breaks);
    for b in &t.breaks {
        assert!((b.depth - DROP).abs() < 1e-9 && (b.slope - 1.0).abs() < 1e-9, "{b:?}");
        let z = b.edge[0].z;
        assert!(z == 0.0 || z == -10.0, "a break off the edge levels: {b:?}");
        assert!(b.edge.iter().all(|p| p.z == z), "a level loop that climbs: {b:?}");
    }
}

#[test]
fn concave_creases_are_pencil_s_corner_not_a_burr() {
    // The C-channel: two reflex edges (inner wall meets floor and
    // ceiling at y = 1). Nothing cut INTERIOR to x stands on either
    // reflex line; the convex channel-bottom crease does cut.
    let poly = [(0.0, 0.0), (4.0, 0.0), (4.0, 1.0), (1.0, 1.0), (1.0, 3.0), (4.0, 3.0), (4.0, 4.0), (0.0, 4.0)];
    let t = deburr(&prism(&poly, 6.0), &params(10.0, -4.0)).expect("convex edges exist");
    for c in cut_points(&t) {
        if c.x > 1e-9 && c.x < 6.0 - 1e-9 {
            let on_reflex = (c.y - 1.0).abs() < 1e-9
                && ((c.z - (1.0 - DROP)).abs() < 1e-9 || (c.z - (3.0 - DROP)).abs() < 1e-9);
            assert!(!on_reflex, "a concave crease rode: {c:?}");
        }
    }
    assert!(
        cut_points(&t).iter().any(|c| c.y.abs() < 1e-9 && (c.z + DROP).abs() < 1e-9),
        "the channel's convex bottom crease never stood in the path"
    );
}

#[test]
fn the_window_clips_along_the_edges_own_line() {
    // A trapezoid curtain in y = 0: the top rim level at z 0, its two
    // sides falling 1 in 2 to z -10 (shallower than the 90-deg cone's
    // flank, so they ride). With the tip window [-4, -2] the top rim
    // (tip -2) rides whole and each side is cut short - its kept end
    // stands exactly ON the window floor, the rim's own slope's doing,
    // not a drop of the whole segment.
    let f = vec![
        Triangle::new(p3(-20.0, 0.0, -10.0), p3(30.0, 0.0, -10.0), p3(10.0, 0.0, 0.0)),
        Triangle::new(p3(-20.0, 0.0, -10.0), p3(10.0, 0.0, 0.0), p3(0.0, 0.0, 0.0)),
    ];
    let t = deburr(&f, &params(-2.0, -4.0)).expect("top rim is in reach");
    assert!(t.notes.is_empty(), "nothing steep here: {:?}", t.notes);
    let cut = cut_points(&t);
    let want = keys(&[p3(-4.0, 0.0, -4.0), p3(0.0, 0.0, -2.0), p3(10.0, 0.0, -2.0), p3(14.0, 0.0, -4.0)]);
    let mut got = keys(&cut);
    got.sort_unstable();
    got.dedup();
    assert_eq!(got, want, "one clipped circuit: the rim plus two clipped sides");
    assert_eq!(t.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count(), 1);
    for c in &cut {
        assert!(c.z >= -4.0 - 1e-9 && c.z <= -2.0 + 1e-9, "a tip outside the programmed window: {c:?}");
    }
}

#[test]
fn a_vertical_edge_is_skipped_out_loud_never_countersunk() {
    // #303: the plain curtain's side rims are vertical. A vertical cone
    // riding one, apex DROP under each point, would cut the top of the
    // rim down to the window floor - so only the top rim rides, the
    // sides are skipped, and the note names them.
    let t = deburr(&curtain(0.0, 10.0, -10.0, 0.0), &params(-2.0, -4.0)).expect("top rim is in reach");
    assert!(t.notes.iter().any(|n| n.contains("2 edges climb steeper")), "{:?}", t.notes);
    let mut got = keys(&cut_points(&t));
    got.dedup();
    assert_eq!(got, keys(&[p3(0.0, 0.0, -2.0), p3(10.0, 0.0, -2.0)]), "only the level rim rides");
}

#[test]
fn a_weld_gone_wrong_is_skipped_out_loud() {
    // Flips: two triangles crossing the same edge in the SAME
    // direction - one winding is wrong. The shared edge is skipped
    // and the path says so; the rims still ride.
    let flip = vec![
        Triangle::new(p3(0.0, 0.0, 0.0), p3(1.0, 0.0, 0.0), p3(0.5, -1.0, 0.0)),
        Triangle::new(p3(0.0, 0.0, 0.0), p3(1.0, 0.0, 0.0), p3(0.5, 1.0, 1.0)),
    ];
    let t = deburr(&flip, &params(1.0, -3.0)).expect("the rims are edges");
    assert_eq!(t.notes.len(), 1, "the flip is a spoken skip: {:?}", t.notes);
    assert!(t.notes[0].contains("inconsistent winding"));
    assert!(
        !cut_points(&t).iter().any(|c| (c.x - 0.5).abs() < 1e-9 && c.y.abs() < 1e-9 && (c.z + DROP).abs() < 1e-9),
        "the flipped edge rode anyway"
    );

    // Manifold: three facets welded on one edge - a weld gone wrong
    // is no edge to break; skipped with its own note.
    let pages = vec![
        Triangle::new(p3(0.0, 0.0, 0.0), p3(0.0, 0.0, 1.0), p3(1.0, 0.0, 0.5)),
        Triangle::new(p3(0.0, 0.0, 0.0), p3(0.0, 0.0, 1.0), p3(-0.5, 0.87, 0.5)),
        Triangle::new(p3(0.0, 0.0, 0.0), p3(0.0, 0.0, 1.0), p3(-0.5, -0.87, 0.5)),
    ];
    let t = deburr(&pages, &params(1.0, -3.0)).expect("the page rims are edges");
    assert!(t.notes.iter().any(|n| n.contains("more than two facets")), "{:?}", t.notes);
}

#[test]
fn an_edge_nothing_exposes_refuses_by_name() {
    // A closed box with the whole window off the model (still under
    // the rapids plane, so it is the reach that speaks, not a door):
    // no tip can stand in it, and the answer names that, never a path.
    let f = box_mesh(0.0, 0.0, 4.0, 4.0, -10.0, 0.0);
    let e = err(deburr(&f, &params(15.0, 10.0)));
    assert!(e.contains("no exposed edge inside the programmed window"), "{e}");
    // And the input doors refuse first: no facets at all.
    let e = err(deburr(&[], &params(1.0, 0.0)));
    assert!(e.contains("model has no facets"), "{e}");
    // A clearance plane under the window top would start a plunge
    // inside the cut.
    let e = err(deburr(&split_patch(), &DeburrParams { clearance_z: -1.0, ..params(0.0, -4.0) }));
    assert!(e.contains("plunge would start inside the cut"), "{e}");
    // 0 or 180 degrees of cone rules no chamfer; no width breaks no edge.
    let e = err(deburr(&split_patch(), &DeburrParams { tip_angle_deg: 180.0, ..params(0.0, -4.0) }));
    assert!(e.contains("cone tool"), "{e}");
    let e = err(deburr(&split_patch(), &DeburrParams { width: 0.0, ..params(0.0, -4.0) }));
    assert!(e.contains("width must be positive"), "{e}");
}

#[test]
fn deburr_at_the_wall_corner_is_chamfer_byte_for_byte() {
    // The congruence law. A 90-deg included cone (chamfer's 45-deg
    // face) breaking the convex crease where a box's flat top meets
    // its wall stands OVER the wall loop, at corner_z: the same XY
    // circuit chamfer.rs posts and the same single Z. Byte law, up
    // to the last-bit difference between w * tan(45) and w / tan(45)
    // - quantized to microns, which no real disagreement survives.
    let (w, top_z) = (2.0, 0.0);
    let box_ = box_mesh(0.0, 0.0, 40.0, 20.0, -10.0, top_z);
    // The window at exactly the crease tip's LEVEL isolates the top
    // circuit: the vertical corner creases clip to zero length (no
    // segment), the bottom rims stand far below. The level comes from
    // drop(), not the DROP constant - tan(45 deg) is one ulp shy of
    // 1, and an exact-boundary window must be exact.
    let mut w3 = params(0.0, 0.0);
    let level = -w3.drop();
    w3.top_z = level;
    w3.bottom_z = level;
    let t = deburr(&box_, &w3).expect("the top crease is in the window");
    let cp = ChamferParams {
        width: w,
        angle_deg: 45.0,
        clearance_z: 20.0,
        retract_z: None,
        top_z,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        inside: false,
    };
    let c = chamfer(&rect(0.0, 0.0, 40.0, 20.0), &cp).expect("the chamfer twin");
    let (mut ka, mut kb) = (keys(&cut_points(&t)), keys(&cut_points(&c)));
    ka.dedup();
    kb.dedup();
    assert_eq!(ka, kb, "the cone over the crease and the ride on the wall disagree");
    // Both are one closed circuit of the same four chords; which
    // corner carries the repeated start is entry law, not geometry.
    let pa = cut_points(&t);
    let pb = cut_points(&c);
    assert_eq!(pa.len(), pb.len(), "same number of chords");
    let same = |a: &Point3, b: &Point3| {
        (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6 && (a.z - b.z).abs() < 1e-6
    };
    assert!(same(&pa[0], &pa[pa.len() - 1]), "deburr's circuit closes on its start");
    assert!(same(&pb[0], &pb[pb.len() - 1]), "the chamfer circuit closes too");
    // And the depth laws state the same number within a micron.
    let za = t.moves.iter().find(|m| m.kind == MoveKind::Plunge).unwrap().to.z;
    assert!((za - cp.corner_z()).abs() < 1e-6, "corner_z {} vs deburr tip {za}", cp.corner_z());
}

#[test]
fn every_run_leaves_its_cut_straight_up_before_the_next_traverse() {
    // #304: a closed box rides as several runs (the rims, the corner
    // tip lines), and the cone ends each one still in the edge it
    // broke. Every rapid that starts below the clearance plane must be
    // the vertical retract out of that break - a diagonal one would
    // drag the cone up and across the part.
    let f = box_mesh(0.0, 0.0, 40.0, 20.0, -10.0, 0.0);
    let p = params(2.0, -12.0);
    let t = deburr(&f, &p).expect("a box is all burr");
    let plunges = t.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count();
    assert!(plunges >= 2, "the box must ride as several runs to test the links: {plunges}");
    for (i, m) in t.moves.iter().enumerate() {
        if m.kind != MoveKind::Rapid || m.from.z >= p.clearance_z - 1e-9 {
            continue;
        }
        assert!(
            (m.from.x - m.to.x).abs() < 1e-9 && (m.from.y - m.to.y).abs() < 1e-9,
            "rapid {i} leaves cutting depth diagonally: {:?} -> {:?}",
            m.from,
            m.to
        );
        assert!((m.to.z - p.clearance_z).abs() < 1e-9, "rapid {i} stops short of clearance: {:?}", m.to);
    }
}
