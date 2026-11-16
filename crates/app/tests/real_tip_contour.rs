//! The real tip at the app seam (M13.7 A, owed item): a Contour with a
//! non-square tip, generated through the app's own operation path along
//! a wall with a fillet at its foot, is judged by the verifier with the
//! op's real tip (`VerifyContext.cutter`, set in `operation::generate`),
//! and the same path judged with the square stack (`cutter: None`) is
//! convicted. Pushed into the floor, the same op's path still convicts
//! under the real tip. The app's own doors never let the op get there:
//! a negative stock to leave is refused at creation, and a Bottom below
//! the floor is held at the wall's foot by the depth ladder - so the
//! floor case drives the generated path down by hand.
//!
//! Why a bull nose and not a ball: the tool law refuses a ball-end
//! Contour at creation (`tool_law::FLAT_MILLS` - a 2D kind promises a
//! flat floor, which a ball would scallop), so no ball-end Contour can
//! exist in a document to be generated. The first test pins that door;
//! the rest use the corner-radius tip the law does admit (BB-1000-R1),
//! which exercises the same real-tip path (`Cutter::of`, the section
//! law's `tip_height` lift, the solid law's real tip).
//!
//! Why the square case is hand-placed (M13.7, ladder vs verifier): the
//! Contour generator keeps the tool off the fillet by itself. Its
//! per-ring depth ladder (`machining2d::contour_depth`) reads the part
//! solid with a SQUARE section - a ring ends at the deepest vertex
//! plane whose material above stands no nearer than `r + leave -
//! 0.008` - which is stricter than the verifier's tangency law
//! (`gouge_tol`, 0.01 mm), so no generated level is one the square
//! verdict convicts. The fillet here is a plain 16-facet R3 quarter
//! round: its first facet below the wall leans out 0.0145 mm. Before
//! the ladder was tightened (its slack was 0.02) the ring ended on that
//! facet's lower vertex plane (the "seam"), inside the generator's
//! slack and outside the square verdict's tolerance - so a FLAT end
//! mill's generated Contour was convicted of a gouge that is not real,
//! blocking its post. Now the ladder stops at the wall's foot (Z = R,
//! the fillet's top), where a flat and a bull nose are both clean. The
//! real-tip-vs-square difference is therefore proven on the generated
//! path with its floor level moved down to the seam by hand: there the
//! real tip, which reaches full radius only `corner_r` above its tip
//! point, is clear, and the square stack, full radius at the tip, bites
//! the leaning facet by more than the tolerance.
//!
//! Provenance: our own fixtures, specced from the Prime Directive and
//! the handbook end-mill side views; no vendor material.
//!
//! Gate map (M13.7 A): covers app::operation (generate, the verify
//! context's cutter, tool_law), motion::machining2d (contour_on_part,
//! contour_depth), sim::verify (section law b, solid law b2,
//! Cutter::judged).

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geo2d::Region;
use camcraft_app::geometry::{Mesh, Point3, Triangle, slice};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::verify::{Cutter, Mode, VerifyContext, Warning, verify};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

const BALL: &str = "BN-1000";
const BULL: &str = "BB-1000-R1";
const FLAT: &str = "EM-1000-4FL";
/// Fillet radius at the wall's foot, and its facet count.
const FILLET_R: f64 = 3.0;
const FILLET_N: usize = 16;
/// The wall the ring rides (x = WALL), the boss top, the floor.
const WALL: f64 = 10.0;
const TOP: f64 = 10.0;
const FLOOR: f64 = 0.0;
/// The wall's foot, where the fillet meets it: the ladder's floor.
const FOOT: f64 = FLOOR + FILLET_R;

/// The fillet's profile in (x, z), from the floor (WALL + R, 0) up to
/// the wall (WALL, R): a quarter round about (WALL + R, R), endpoints
/// exact.
fn fillet() -> Vec<(f64, f64)> {
    (0..=FILLET_N)
        .map(|k| match k {
            0 => (WALL + FILLET_R, FLOOR),
            k if k == FILLET_N => (WALL, FLOOR + FILLET_R),
            k => {
                let phi = (270.0 - 90.0 * k as f64 / FILLET_N as f64).to_radians();
                (WALL + FILLET_R + FILLET_R * phi.cos(), FLOOR + FILLET_R + FILLET_R * phi.sin())
            }
        })
        .collect()
}

/// A part extruded along Y over [-10, 10]: a 20 x 20 boss Z [0, 10]
/// standing on a plate Z [-5, 0] that runs out to x 30 on the +x side,
/// with an R3 fillet in the corner between the boss's +x wall and the
/// plate top. One closed, outward-wound shell.
fn part() -> Mesh {
    // The profile, counter-clockwise in (x, z); every cap piece's corner
    // is one of these vertices or an interior point.
    let mut prof: Vec<(f64, f64)> = vec![(-10.0, -5.0), (WALL, -5.0), (WALL + FILLET_R, -5.0), (30.0, -5.0), (30.0, FLOOR)];
    prof.extend(fillet());
    prof.extend([(WALL, TOP), (-10.0, TOP), (-10.0, FLOOR + FILLET_R), (-10.0, FLOOR)]);
    let (y0, y1) = (-10.0, 10.0);
    let mut f: Vec<Triangle> = Vec::new();
    for k in 0..prof.len() {
        let (a, b) = (prof[k], prof[(k + 1) % prof.len()]);
        let (a0, b0) = (Point3::new(a.0, y0, a.1), Point3::new(b.0, y0, b.1));
        let (a1, b1) = (Point3::new(a.0, y1, a.1), Point3::new(b.0, y1, b.1));
        f.push(Triangle::new(a0, b1, b0));
        f.push(Triangle::new(a0, a1, b1));
    }
    // Cap triangles, counter-clockwise in (x, z): the y0 cap as is (its
    // normal is -y), the y1 cap reversed.
    let mut tris: Vec<[(f64, f64); 3]> = Vec::new();
    let rect = |x0: f64, z0: f64, x1: f64, z1: f64, t: &mut Vec<[(f64, f64); 3]>| {
        t.push([(x0, z0), (x1, z0), (x1, z1)]);
        t.push([(x0, z0), (x1, z1), (x0, z1)]);
    };
    rect(-10.0, -5.0, WALL, FLOOR, &mut tris);
    rect(-10.0, FLOOR, WALL, FLOOR + FILLET_R, &mut tris);
    rect(-10.0, FLOOR + FILLET_R, WALL, TOP, &mut tris);
    rect(WALL, -5.0, WALL + FILLET_R, FLOOR, &mut tris);
    rect(WALL + FILLET_R, -5.0, 30.0, FLOOR, &mut tris);
    // The fillet's corner material, fanned from the corner (WALL, FLOOR):
    // the arc between the two tangent points is all visible from it.
    for w in fillet().windows(2) {
        tris.push([(WALL, FLOOR), w[0], w[1]]);
    }
    for [p, q, r] in tris {
        let at = |c: (f64, f64), y: f64| Point3::new(c.0, y, c.1);
        f.push(Triangle::new(at(p, y0), at(q, y0), at(r, y0)));
        f.push(Triangle::new(at(p, y1), at(r, y1), at(q, y1)));
    }
    Mesh::from_facets(f)
}

fn tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog tool")
}

fn contour_op(leave: f64) -> OpParams {
    OpParams::Contour {
        stock_to_leave: leave,
        clearance_z: Height::fixed(20.0),
        retract_z: Height::fixed(15.0),
        top_z: Height::fixed(TOP),
        bottom_z: Height::fixed(FLOOR),
        stepdown: 2.5,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        tilt: None,
        links: camcraft_app::links::Links::PLANAR,
    }
}

/// The part, its stock and `tool_name` in a fresh document.
fn world_with(tool_name: &str) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", part()));
    let loc = Location { pos: Point3::new(10.0, 0.0, -5.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 70.0, dy: 50.0, dz: 15.0 }, loc).expect("stock"));
    h.execute(&world, AddTool::new(&world, tool_name, tool(tool_name)).expect("tool"));
    (world, h)
}

/// Create + generate a bull-nose Contour on the part through the app's
/// own path: the cached path and the warnings the app attached.
fn generate(leave: f64) -> (Toolpath, Vec<Warning>) {
    generate_with(BULL, leave)
}

/// The same, with `tool_name` from the catalog.
fn generate_with(tool_name: &str, leave: f64) -> (Toolpath, Vec<Warning>) {
    let (world, mut h) = world_with(tool_name);
    let op = CreateOperation::new(&world, "C1", tool_name, SrcRef::Entity("part".into()), contour_op(leave));
    h.execute(&world, op.expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "C1").expect("generate"));
    let e = model::find_by_name(&world, "C1").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cached path");
    (tp.path.clone(), tp.warnings.clone())
}

/// The section the app takes off a whole part at Top (a micron under
/// it - the slice-retry law): the boss alone, the fillet is below it.
fn top_section() -> Region {
    slice::slice(&part().facets, TOP - 1e-3).expect("section")
}

/// The app's Contour verify, re-run on `path` with the cutter given:
/// the same model law (the part's facets, `DepthLaw::Section`), mode,
/// heights and tolerances; only the tip under judgement varies.
fn judge(path: &Toolpath, cutter: Option<Cutter>, leave: f64) -> Vec<Warning> {
    let mesh = part();
    let region = top_section();
    let geom = tool(BULL).geom();
    verify(
        path,
        &VerifyContext {
            region: &region,
            stock: None,
            model: Some(&mesh.facets),
            tool: &geom,
            cutter,
            mode: Mode::Boundary,
            stock_top_z: TOP,
            clearance_z: 20.0,
            stock_to_leave: leave,
            gouge_tol: 0.01,
            z_tol: 1e-6,
            finish_tol: None,
            fixtures: &[],
            spindle: None,
            ..Default::default()
        },
    )
}

fn gouged_moves(w: &[Warning]) -> Vec<usize> {
    w.iter()
        .filter_map(|w| match w {
            Warning::Gouge { at } => Some(*at),
            _ => None,
        })
        .collect()
}

/// The deepest cutting plane of the path.
fn deepest(t: &Toolpath) -> f64 {
    t.moves.iter().filter(|m| m.kind != MoveKind::Rapid).map(|m| m.to.z).fold(f64::INFINITY, f64::min)
}

/// The fillet's vertex plane just below the wall, and how far that
/// vertex leans out of the wall.
fn seam() -> (f64, f64) {
    let (x, z) = fillet()[FILLET_N - 1];
    (z, x - WALL)
}

/// `path` with every endpoint on plane `from_z` moved to `to_z` (bit
/// exact match: the generator's own level values).
fn relevel(path: &Toolpath, from_z: f64, to_z: f64) -> Toolpath {
    let mut out = path.clone();
    for m in &mut out.moves {
        if m.from.z.to_bits() == from_z.to_bits() {
            m.from.z = to_z;
        }
        if m.to.z.to_bits() == from_z.to_bits() {
            m.to.z = to_z;
        }
    }
    out
}

#[test]
fn a_ball_end_contour_is_refused_at_the_tool_door() {
    let (world, _h) = world_with(BALL);
    let op = CreateOperation::new(&world, "C1", BALL, SrcRef::Entity("part".into()), contour_op(0.0));
    let err = op.err().map(|e| format!("{e:?}")).unwrap_or_default();
    assert!(err.contains("flat end mill or bull-nose"), "a ball-end Contour is refused by name: {err:?}");
}

#[test]
fn a_bull_nose_contour_along_a_filleted_wall_is_clean_with_its_tip_and_convicted_square() {
    let (seam_z, lean) = seam();
    // The fixture sits in the window the module law names: outside the
    // verdict's 0.01 tolerance (so outside the ladder's 0.008 slack
    // too), inside the old 0.02 ladder slack with the 0.005 arc sag.
    assert!(lean > 0.01 && lean + 0.005 < 0.02, "fillet seam leans {lean}");
    let real_tip = Cutter::of(&tool(BULL));
    assert!(real_tip.corner_r > 0.0 && real_tip.tip_height() > 0.0, "{real_tip:?}");

    let (path, warns) = generate(0.0);
    assert!(!path.moves.is_empty(), "{:?}", path.notes);
    // The generator keeps the tip off the fillet: the ring ends at the
    // wall's foot, never on the fillet - the whole foot is the next
    // op's rest.
    let low = deepest(&path);
    assert!((low - FOOT).abs() < 1e-9, "ring floor {low}, foot {FOOT}: {:?}", path.notes);

    // The app's own verdict, with the op's real tip: no gouge.
    assert_eq!(gouged_moves(&warns), Vec::<usize>::new(), "app warnings {warns:?}");
    // The re-run with the same real tip agrees with the app; at the
    // foot the square stack is clean too (the ladder is stricter than
    // the verdict, so nothing generated is convicted square).
    let real = judge(&path, Some(real_tip), 0.0);
    assert_eq!(gouged_moves(&real), Vec::<usize>::new(), "real-tip re-run {real:?}");
    let square = judge(&path, None, 0.0);
    assert_eq!(gouged_moves(&square), Vec::<usize>::new(), "square at the foot {square:?}");

    // Hand-placed (see the module law): the floor level moved down to
    // the fillet's seam. The real tip is still clear there...
    let path = relevel(&path, low, seam_z);
    assert!((deepest(&path) - seam_z).abs() < 1e-12);
    let real = judge(&path, Some(real_tip), 0.0);
    assert_eq!(gouged_moves(&real), Vec::<usize>::new(), "real tip at the seam {real:?}");
    // ...and the square stack on the very same path convicts: its rim
    // at the tip plane bites the fillet's first facet by more than
    // tolerance.
    let square = judge(&path, None, 0.0);
    let convicted = gouged_moves(&square);
    assert!(!convicted.is_empty(), "the square stack must convict: {square:?}");
    for at in &convicted {
        let m = &path.moves[*at];
        assert!(
            m.from.z.min(m.to.z) <= seam_z + 1e-9 && m.to.x.max(m.from.x) > WALL,
            "only the seam level beside the filleted wall is convicted: {m:?}"
        );
    }
}

#[test]
fn the_same_contour_pushed_into_the_floor_still_convicts() {
    // (1) The generated path, its floor level driven 0.5 mm into the
    // plate: the tip now buries itself in the fillet and the floor.
    // The real tip must convict it - its corner is no excuse in steel.
    let (path, _) = generate(0.0);
    let sunk_z = FLOOR - 0.5;
    let sunk = relevel(&path, deepest(&path), sunk_z);
    assert!((deepest(&sunk) - sunk_z).abs() < 1e-12);
    let real = judge(&sunk, Some(Cutter::of(&tool(BULL))), 0.0);
    let convicted = gouged_moves(&real);
    assert!(!convicted.is_empty(), "into the floor is a gouge with the real tip: {real:?}");
    for at in &convicted {
        let m = &sunk.moves[*at];
        assert!(m.from.z.min(m.to.z) <= sunk_z + 1e-9, "only the sunk level is convicted: {m:?}");
    }

    // (2) The app never lets the op itself get there, which is why (1)
    // drives the path by hand. A negative stock to leave is refused at
    // creation, before any path exists.
    let (world, _h) = world_with(BULL);
    let op = CreateOperation::new(&world, "C1", BULL, SrcRef::Entity("part".into()), contour_op(-0.5));
    let err = op.err().map(|e| format!("{e:?}")).unwrap_or_default();
    assert!(err.contains("not usable"), "negative leave is refused at creation: {err:?}");
    // A Bottom below the floor is held by the ladder: the ring still
    // ends at the wall's foot, never in the plate, and stays clean.
    let (world, mut h) = world_with(BULL);
    let mut deep = contour_op(0.0);
    if let OpParams::Contour { bottom_z, .. } = &mut deep {
        *bottom_z = Height::fixed(FLOOR - 2.0);
    }
    h.execute(&world, CreateOperation::new(&world, "C2", BULL, SrcRef::Entity("part".into()), deep).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "C2").expect("generate"));
    let e = model::find_by_name(&world, "C2").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cached path");
    assert!((deepest(&tp.path) - FOOT).abs() < 1e-9, "bottom below the floor: {:?}", tp.path.notes);
    assert_eq!(gouged_moves(&tp.warnings), Vec::<usize>::new(), "{:?}", tp.warnings);
}

#[test]
fn a_flat_end_contour_beside_a_leaning_facet_verifies_clean() {
    // The ladder-vs-verifier law (M13.7): the fillet's first facet
    // leans out of the wall 0.0145 mm, more than the verdict's 0.01
    // tolerance. With the old 0.02 ladder slack a flat end mill's ring
    // ended on that facet's seam, where the square verdict (a flat tip
    // IS square) convicted a gouge that is not real and blocked the
    // post. The ladder now stops where its own verifier is clean.
    let (seam_z, lean) = seam();
    assert!(lean > 0.01, "the fixture's facet must lean past tolerance: {lean}");
    let (path, warns) = generate_with(FLAT, 0.0);
    assert!(!path.moves.is_empty(), "{:?}", path.notes);
    let low = deepest(&path);
    assert!(low > seam_z + 1e-6, "the flat ring must stop above the leaning facet: floor {low}, seam {seam_z}");
    assert!((low - FOOT).abs() < 1e-9, "ring floor {low}, foot {FOOT}: {:?}", path.notes);
    assert!(
        !warns.iter().any(|w| matches!(w, Warning::Gouge { .. })),
        "the generated flat-end Contour must verify clean: {warns:?}"
    );
}
