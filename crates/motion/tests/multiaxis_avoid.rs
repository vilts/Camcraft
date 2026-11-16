//! M5.1 stage F law tests: the spindle dodges the part without moving
//! the path. The kernel (`multiaxis::avoid`) mirrors `verify`'s law
//! (c) enclosure, so what rides here is the COMPOSITION: balls
//! bit-for-bit the flag-off ladder's (tips follow the pose, the
//! pivot law), floors posed only where the
//! shank speaks, the escape inside the cone and calm across
//! neighbors, and a slot no cone angle clears refusing by name.
//!
//! Gate map (M4.4): covers camcraft_motion::cl, camcraft_motion::geometry,
//! camcraft_motion::machining2d, camcraft_motion::machining3d,
//! camcraft_motion::multiaxis.
//! Lane shape (M4.4): all tests here run in seconds; envelope
//! verdicts over these paths ride the app's ignored lane.

use camcraft_motion::cl::MoveKind;
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::PocketStrategy;
use camcraft_motion::machining3d::{Entry3D, Pocket3DParams};
use camcraft_motion::multiaxis::{AxisControl, Shank, multiaxis_rough};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| {
        p(
            [min[0], max[0]][i & 1],
            [min[1], max[1]][(i >> 1) & 1],
            [min[2], max[2]][(i >> 2) & 1],
        )
    };
    let quad = |a: usize, b: usize, c: usize, d: usize| -> [Triangle; 2] {
        [
            Triangle::new(v(a), v(b), v(c)),
            Triangle::new(v(a), v(c), v(d)),
        ]
    };
    let mut t = Vec::with_capacity(12);
    t.extend(quad(4, 5, 7, 6)); // +z top
    t.extend(quad(0, 2, 3, 1)); // -z bottom
    t.extend(quad(0, 1, 5, 4)); // -y
    t.extend(quad(2, 6, 7, 3)); // +y
    t.extend(quad(1, 3, 7, 5)); // +x
    t.extend(quad(2, 0, 4, 6)); // -x
    t
}

/// The E3 fixture: a 10x10 boss standing to z = 0 over a floor at -10.
fn slab_with_boss() -> Vec<Triangle> {
    let mut f = box_facets([-20.0, -20.0, -20.0], [20.0, 20.0, -10.0]);
    f.extend(box_facets([-5.0, -5.0, -10.0], [5.0, 5.0, 0.0]));
    f
}

/// A 9-wide slot between two full-height walls: the 8 ball fits, a
/// 10-wide shank beside either wall never does, and tilting toward
/// one drives it into the other. No cone angle clears - refusal is
/// the honest answer.
fn narrow_slot() -> Vec<Triangle> {
    let mut f = box_facets([-20.0, -20.0, -20.0], [20.0, 20.0, -10.0]);
    f.extend(box_facets([-20.0, -20.0, -10.0], [-4.5, 20.0, 5.0]));
    f.extend(box_facets([4.5, -20.0, -10.0], [20.0, 20.0, 5.0]));
    f
}

fn params() -> Pocket3DParams {
    Pocket3DParams {
        links: camcraft_motion::links::Links::ROUGH3D,
        tool_diameter: 8.0,
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        top_z: 0.0,
        bottom_z: -10.0,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        retract_z: None,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_motion::load::DEFAULT_OPTIMAL_LOAD,
    }
}

fn control(lead: f64, lag: f64, clamp: Option<f64>) -> AxisControl {
    AxisControl {
        lead_deg: lead,
        lag_deg: lag,
        max_tilt_deg: clamp,
    }
}

/// A ball r=4 on a 6-long flute, a 10-wide shank, a 16-wide holder:
/// the stub's bottom rim stands INSIDE the boss wall (z from -4, the
/// wall to 0) when a floor row passes 4 mm off the face - a real
/// graze, and one 15 deg of lean genuinely clears (the leaning rim
/// stays above the pocket floor, law (c)'s own verdict). A shorter
/// flute would sink the rim below the floor at every cone angle, and
/// then the honest answer is the refusal the slot test proves.
fn shank() -> Shank {
    Shank {
        flute_len: 6.0,
        shank_r: 5.0,
        shank_len: 20.0,
        holder_r: 8.0,
        holder_len: 10.0,
    }
}

fn silhouette(facets: &[Triangle]) -> camcraft_motion::geo2d::Region {
    camcraft_motion::geometry::slice::project(facets, 64).unwrap()
}

fn tilt_from_up(a: Point3) -> f64 {
    let l = (a.x * a.x + a.y * a.y + a.z * a.z).sqrt();
    (a.z / l).clamp(-1.0, 1.0).acos().to_degrees()
}

fn angle_deg(a: [f64; 3], b: [f64; 3]) -> f64 {
    let la = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    let lb = (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();
    let d = ((a[0] * b[0] + a[1] * b[1] + a[2] * b[2]) / (la * lb)).clamp(-1.0, 1.0);
    d.acos().to_degrees()
}

/// The ball radius of `params()`.
const BALL_R: f64 = 4.0;

/// Every move as (kind, BALL CENTRE, held pose): the stream's tip
/// lifted `BALL_R` along the pose the machine holds there (the last
/// stated - the post keeps it modal; upright before any).
fn balls(path: &camcraft_motion::cl::Toolpath) -> Vec<(MoveKind, Point3, [f64; 3])> {
    let mut held = [0.0, 0.0, 1.0];
    path.moves
        .iter()
        .map(|m| {
            if let Some(a) = m.axis {
                held = a;
            }
            let c = p(m.to.x + BALL_R * held[0], m.to.y + BALL_R * held[1], m.to.z + BALL_R * held[2]);
            (m.kind, c, held)
        })
        .collect()
}

fn leaning(a: [f64; 3]) -> bool {
    !camcraft_motion::cl::is_vertical(&a)
}

#[test]
fn dodging_never_moves_the_ball() {
    // Law (f) is the whole contract: a sphere's envelope is
    // rotation-invariant about its center, so the dodge may spend
    // the free axis and NOTHING else. Every BALL and every move kind
    // stands the flag-off ladder's (the tips move with the pose: the
    // pivot law backs each one off its centre along the axis).
    let f = slab_with_boss();
    let region = silhouette(&f);
    let off = multiaxis_rough(&f, &region, &params(), &control(0.0, 0.0, Some(60.0)), None).unwrap();
    let on =
        multiaxis_rough(&f, &region, &params(), &control(0.0, 0.0, Some(60.0)), Some(&shank()))
            .unwrap();
    assert_eq!(off.moves.len(), on.moves.len(), "the dodge added a move");
    let (bo, bn) = (balls(&off), balls(&on));
    for (i, (a, b)) in bo.iter().zip(&bn).enumerate() {
        assert_eq!(a.0, b.0, "kind changed at move {i}");
        let d = a.1 - b.1;
        assert!(d.dot(d).sqrt() < 1e-9, "the ball moved at {i}: {:?} vs {:?}", a.1, b.1);
    }
    let mut held: Option<[f64; 3]> = None;
    for m in &on.moves {
        if m.kind == MoveKind::Rapid {
            assert_eq!(m.axis, held, "a rapid changed the pose it inherited");
        }
        held = m.axis.or(held);
    }
    // Where F re-answers a pose, it answers inside the cone. (Before
    // the pivot law F kept every wall lean E3 stated: it measured the
    // shank from the UPRIGHT tip, r up the wrong column. On the real
    // tip - the contact point, for a lean dead on a wall's normal -
    // the 5-radius shank of a horizontal lean dips a millimetre under
    // the 4-radius ball into the pocket floor, and the escape is the
    // honest answer there.)
    for (a, b) in off.moves.iter().zip(&on.moves) {
        if a.kind != MoveKind::Rapid && a.axis != b.axis {
            // Zero lead: E3's pose IS the cone's axis (the contact
            // normal, or upright on a floor).
            let x = b.axis.unwrap_or([0.0, 0.0, 1.0]);
            let n = a.axis.unwrap_or([0.0, 0.0, 1.0]);
            assert!(angle_deg(x, n) <= 60.0 + 1e-6, "an escape left the cone: {x:?} about {n:?}");
        }
    }
    // And the dodge actually spoke somewhere, or this test proves
    // nothing but the opt-out.
    assert!(
        on.moves.iter().zip(&off.moves).any(|(a, b)| a.axis != b.axis),
        "the shank never spoke on a stub cutting beside a 10-wide boss"
    );
}

#[test]
fn floors_are_posed_only_where_the_shank_speaks() {
    // Stage E3 left every floor unposed (the contact law has nothing
    // to say on a level cut). F adds the second opinion: a floor
    // whose vertical shank rubs the boss wall - the rows standing 4
    // off a wall face - gains a pose, and the rows the shank clears
    // by a mile stay None exactly as E3 had them.
    let f = slab_with_boss();
    let region = silhouette(&f);
    let off = multiaxis_rough(&f, &region, &params(), &control(0.0, 0.0, Some(60.0)), None).unwrap();
    let on =
        multiaxis_rough(&f, &region, &params(), &control(0.0, 0.0, Some(60.0)), Some(&shank()))
            .unwrap();
    let mut grazed = 0;
    let upright = |a: Option<[f64; 3]>| a.is_none_or(|a| !leaning(a));
    for (i, (a, b)) in off.moves.iter().zip(&on.moves).enumerate() {
        // The flag-off ladder's floors stand upright, so their tips
        // are the plan's: the floor rows are read off them.
        if a.kind == MoveKind::Rapid || a.to.z != -10.0 {
            continue;
        }
        let near_wall = 8.0 < a.to.x.abs() && a.to.x.abs() < 10.0;
        if a.to.x.abs() >= 12.0 {
            assert!(upright(b.axis), "move {i} posed a floor the shank clears: {:?}", a.to);
        }
        if near_wall {
            assert!(upright(a.axis), "E3's law posed a floor: {i}");
            let x = b
                .axis
                .filter(|x| leaning(*x))
                .unwrap_or_else(|| panic!("move {i} let the shank rub the wall: {:?}", b.to));
            let tilt = tilt_from_up(p(x[0], x[1], x[2]));
            assert!(
                tilt > 1e-6 && tilt <= 60.0001,
                "a graze floor stood vertical or past the cone at {i}: {x:?}"
            );
            grazed += 1;
        }
    }
    assert!(grazed >= 4, "the boss-adjacent floors never appeared: {grazed}");
}

#[test]
fn the_escape_is_inside_the_cone_and_calm_down_the_wall() {
    // Shortest-travel law, shown two ways: every floor escape obeys
    // the 60 deg cone, and successive rows on the SAME wall arrive
    // at the SAME pose bit-for-bit - the A/C head does not wiggle
    // from row to row when nothing about the wall changed.
    let f = slab_with_boss();
    let region = silhouette(&f);
    let on =
        multiaxis_rough(&f, &region, &params(), &control(0.0, 0.0, Some(60.0)), Some(&shank()))
            .unwrap();
    let mut plus: Vec<[f64; 3]> = Vec::new();
    let mut minus: Vec<[f64; 3]> = Vec::new();
    for (m, (_, c, _)) in on.moves.iter().zip(balls(&on)) {
        // Read at the BALL CENTRE: a floor row's centre stands r over
        // the floor whatever the pose (the tip follows the lean).
        let Some(a) = m.axis.filter(|a| leaning(*a)) else { continue };
        if m.kind == MoveKind::Rapid || (c.z - (-10.0 + BALL_R)).abs() > 1e-9 {
            continue;
        }
        if c.x > 8.4 && c.y.abs() <= 4.0 {
            plus.push(a);
        } else if c.x < -8.4 && c.y.abs() <= 4.0 {
            minus.push(a);
        }
        assert!(
            angle_deg(a, [0.0, 0.0, 1.0]) <= 60.0 + 1e-6,
            "an escape leaned past the cone: {c:?}"
        );
        // The lean points AWAY from the boss: dodging INTO the wall
        // it dodges is not a dodge.
        assert!(
            a[0] * c.x + a[1] * c.y > 0.0,
            "an escape leaned toward the material: {a:?} at {c:?}"
        );
    }
    for (name, set, outward) in [("+x wall", &plus, 1.0), ("-x wall", &minus, -1.0)] {
        assert!(set.len() >= 2, "no repeat escapes on the {name}: {}", set.len());
        for a in set.iter().skip(1) {
            assert!(
                angle_deg(*a, set[0]) < 1e-6,
                "the {name} escaped two ways: {a:?} vs {:?}",
                set[0]
            );
        }
        // Same flat wall, same seed, same held pose before every row
        // (the far feed that ends each row is unposed, and an
        // unposed cut resets the held state to home): the answer
        // must be the SAME lean, not a lottery over the grid.
        assert!(
            outward * set[0][0] > 0.15 && set[0][2] > 0.3 && set[0][1].abs() < 1e-9,
            "the {name} pose is not a clean outward tilt: {:?}",
            set[0]
        );
    }
}

#[test]
fn a_slot_no_angle_clears_refuses_by_name() {
    // The other half of the contract: where no pose inside the cone
    // clears, F refuses at generation time and says so - never
    // posts a path whose shank is already known to touch, and never
    // leaves it to post-time warnings. Flag off stays E3's behavior
    // (the path generates; the verifier warns), so the escape is
    // the refusal and not a new inability to cut.
    let f = narrow_slot();
    let region = silhouette(&f);
    let e = multiaxis_rough(&f, &region, &params(), &control(0.0, 0.0, Some(60.0)), Some(&shank()));
    match e {
        Err(e) => {
            let msg = format!("{e:?}");
            assert!(msg.contains("shank"), "the refusal does not name the shank: {msg}");
            assert!(msg.contains("cone"), "the refusal does not name the cone: {msg}");
        }
        Ok(path) => panic!(
            "a 10-wide shank slipped into a 9-wide slot: {:?}",
            path.moves
                .iter()
                .filter(|m| m.kind != MoveKind::Rapid)
                .map(|m| (m.to.x, m.to.y, m.to.z, m.axis))
                .collect::<Vec<_>>()
        ),
    }
    let ok = multiaxis_rough(&f, &region, &params(), &control(0.0, 0.0, Some(60.0)), None)
        .expect("flag-off must still cut the slot as E3 did");
    assert!(!ok.moves.is_empty(), "the opt-out lost the ladder");
}

#[test]
fn the_shank_refusal_hands_back_the_world_plan_before_its_move() {
    // #1015: the stamp's refusal carries the plan it was posing, up to
    // the refused move - world, as the stamp's input is (the 3-D
    // ladder over the same part), never a frame of its own.
    let f = narrow_slot();
    let region = silhouette(&f);
    let e = multiaxis_rough(&f, &region, &params(), &control(0.0, 0.0, Some(60.0)), Some(&shank()))
        .expect_err("no cone angle clears the slot");
    let plan = camcraft_motion::machining3d::pocket3d(&f, &region, &params()).expect("the unposed ladder");
    let g = e.ghost().expect("moves were planned before the refused one");
    assert!(g.len() < plan.moves.len(), "only the plan before the refusal");
    assert_eq!(g.moves(), &plan.moves[..g.len()], "the ghost is the world plan itself");
}

#[test]
fn a_shank_that_never_speaks_leaves_the_path_alone() {
    // The opt-in has to be exactly opt-in: a long flute keeps every
    // band's column floor above the model (the box gate skips all
    // of it), and the posed ladder must then stand bit-identical to
    // the unposed one - axes included, not just positions.
    let f = slab_with_boss();
    let region = silhouette(&f);
    let tall = Shank {
        flute_len: 50.0,
        ..shank()
    };
    let off = multiaxis_rough(&f, &region, &params(), &control(0.0, 0.0, Some(60.0)), None).unwrap();
    let on =
        multiaxis_rough(&f, &region, &params(), &control(0.0, 0.0, Some(60.0)), Some(&tall))
            .unwrap();
    for (i, (a, b)) in off.moves.iter().zip(&on.moves).enumerate() {
        assert_eq!(a.axis, b.axis, "a shank that reaches nothing moved a pose at {i}");
    }
    let _ = tilt_from_up(p(0.0, 0.0, 1.0));
}
