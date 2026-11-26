//! #2772 parts 1 and 2: the stated rotation axis. The rotary kinds
//! take their axis from a STATEMENT first - the op's own `stated_axis`
//! direction, else the mounted machine's stated rotary line
//! (tormach-770mx-microarc4.mch's A about X, millturn.mch's C about
//! Z), each anchored through the setup's work offset origin - and
//! JUDGE the model's faces against it (the fit's own concurrency,
//! coarseness and bearing gates about the stated line, in
//! `camcraft_motion::multiaxis::axis_stated`). What "right" means
//! here:
//!
//! - a revolve ON the stated line generates the very path today's
//!   free fit reads (bit for bit - the two suites' moves agree);
//! - a stated line the model's faces disagree with refuses by name,
//!   never a silent fall back to the fit: a revolve placed off the
//!   line is told its own axis and the acts that fix it, and the
//!   reporter's helical rotor on a stated A keeps the radial kinds
//!   locked (the radial drop, a separate slice of this issue, is the
//!   coming answer for that work);
//! - the anchor is the work offset's own position: a moved offset
//!   moves the stated line with it, and the refusal says so;
//! - nothing stated keeps today's free fit and its own refusal words
//!   ("share no rotation axis") untouched.
//!
//! Provenance: our own tests over our own machine files and commands;
//! nothing of vendor binaries.
//!
//! Gate map (M4.4): covers camcraft_app::rotary_stated,
//! camcraft_app::fixture (rotary_axis_of), camcraft_app::operation
//! (the rotary arms' resolution, the letter door, the radial law's
//! axis), camcraft_app::project (the stated_axis round-trip and the
//! v78 step), camcraft-motion (axis_stated, the *_stated kernels),
//! camcraft_app::commands (LoadMachine, SetWcsOrigin).
//! Lane shape (M4.4): every test here runs in seconds; the stated
//! axis's field parity is the heavy lane (`rotary_field_parity.rs`).

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock, SetWcsOrigin,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::{fixture, multiaxis, project};

const TAU: f64 = std::f64::consts::TAU;
/// The tormach-770mx-microarc4.mch registry entry: an A about X,
/// stated `[1, 0, 0]`, work-turning.
const MICROARC: &str = "Tormach 770MX + microARC 4";
/// The millturn.mch registry entry: a spindle C about Z, stated.
const MILLTURN: &str = "Mill-turn";

/// The turned bar's meridian (rho, s): wall r6, shoulder, neck r5.
const MERIDIAN: [(f64, f64); 6] = [(0.0, -16.0), (6.0, -16.0), (6.0, -2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)];

/// The bar revolved about `axis` (0 = world X, laid; 2 = world Z,
/// standing), 16 facets around, centred on the origin.
fn bar(axis: usize) -> Vec<Triangle> {
    let pt = |rho: f64, s: f64, j: usize| {
        let th = TAU * j as f64 / 16.0;
        match axis {
            0 => Point3::new(s, rho * th.cos(), rho * th.sin()),
            _ => Point3::new(rho * th.cos(), rho * th.sin(), s),
        }
    };
    let mut f = Vec::new();
    for w in MERIDIAN.windows(2) {
        let ((ra, za), (rb, zb)) = (w[0], w[1]);
        for j in 0..16 {
            let k = (j + 1) % 16;
            let (a1, a2, b1, b2) = (pt(ra, za, j), pt(ra, za, k), pt(rb, zb, j), pt(rb, zb, k));
            if ra <= 1e-12 {
                f.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                f.push(Triangle::new(a1, a2, b2));
            } else {
                f.push(Triangle::new(a1, a2, b2));
                f.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    f
}

/// The reporter's kind of part (#2772): a 4-lobe helical sweep about
/// world X - the compressor rotor's lobes turning as x advances -
/// clamped ON the stated A line through the origin. Its normals share
/// no rotation axis (the free fit's own refusal), and the solid slices
/// clean (a closed, manifold shell).
fn screw(turns: f64) -> Vec<Triangle> {
    let rate = turns * TAU / 32.0;
    let (nth, nx) = (96usize, 32usize);
    let rho = |th: f64| 5.0 + 1.5 * (4.0 * th).cos();
    let pt = |th_i: usize, x: f64| {
        let th = TAU * th_i as f64 / nth as f64;
        let (s, c) = (rate * x).sin_cos();
        let r = rho(th);
        let (bs, bc) = th.sin_cos();
        Point3::new(x, r * (bc * c - bs * s), r * (bs * c + bc * s))
    };
    let mut f = Vec::new();
    for thi in 0..nth {
        let tk = (thi + 1) % nth;
        for st in 0..nx {
            let (x0, x1) = (-16.0 + 32.0 * st as f64 / nx as f64, -16.0 + 32.0 * (st + 1) as f64 / nx as f64);
            let (a1, a2, b1, b2) = (pt(thi, x0), pt(tk, x0), pt(thi, x1), pt(tk, x1));
            f.push(Triangle::new(a1, a2, b2));
            f.push(Triangle::new(a1, b2, b1));
        }
    }
    // The flat ends, wound outward: the swept tube is a closed solid.
    let cap = |f: &mut Vec<Triangle>, x: f64, outward: bool| {
        let c = Point3::new(x, 0.0, 0.0);
        for thi in 0..nth {
            let tk = (thi + 1) % nth;
            let (a, b) = (pt(thi, x), pt(tk, x));
            f.push(if outward { Triangle::new(c, a, b) } else { Triangle::new(c, b, a) });
        }
    };
    cap(&mut f, -16.0, false);
    cap(&mut f, 16.0, true);
    f
}

/// The rotary parallel's params, with the op's own stated direction
/// when one is given (#2772).
fn parallel(stated_axis: Option<[f64; 3]>) -> OpParams {
    parallel_at(stated_axis, 25.0)
}

/// [`parallel`] at a stated clearance: the review's probe of PR
/// #3674 ran the moved part at 100.
fn parallel_at(stated_axis: Option<[f64; 3]>, clearance: f64) -> OpParams {
    OpParams::RotaryParallel {
        tolerance: 0.1,
        stepover: Some(2.0),
        clearance_z: Height::fixed(clearance),
        retract_z: None,
        top_z: Height::fixed(6.0),
        bottom_z: Height::fixed(1.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
        stated_axis,
        picks: Default::default(),
    }
}

fn contour(stated_axis: Option<[f64; 3]>) -> OpParams {
    contour_at(stated_axis, 25.0)
}

/// [`contour`] at a stated clearance: the mill-turn's live-tool
/// traverse reads the first rapid against the spindle's index height,
/// so its case stands higher.
fn contour_at(stated_axis: Option<[f64; 3]>, clearance: f64) -> OpParams {
    OpParams::RotaryContour {
        tolerance: 0.1,
        clearance_z: Height::fixed(clearance),
        retract_z: None,
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(-14.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
        stated_axis,
        picks: Default::default(),
    }
}

/// Every vertex of `facets` moved by `off`: a common model or WCS
/// translation (the review of PR #3674).
fn moved(facets: &[Triangle], off: [f64; 3]) -> Vec<Triangle> {
    let at = |p: Point3| Point3::new(p.x + off[0], p.y + off[1], p.z + off[2]);
    facets
        .iter()
        .map(|t| Triangle::new(at(t.a), at(t.b), at(t.c)))
        .collect()
}

/// [`world_on`] with the box stock moved to `stock` as well - a
/// common translation of the whole setup (the review of PR #3674
/// moved the model and the stock together).
fn world_moved(mesh: Vec<Triangle>, params: OpParams, stock: [f64; 3]) -> World {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == "BN-0600").map(|(_, t)| t).expect("catalog ball");
    h.execute(&world, AddTool::new(&world, "BN-0600", tool).expect("tool"));
    let (dx, dz) = if mesh.first().is_some_and(|t| t.a.x.abs() > t.a.z.abs()) { (34.0, 14.0) } else { (14.0, 34.0) };
    h.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box { dx, dy: 14.0, dz },
            Location { pos: Point3::new(stock[0], stock[1], stock[2]), ..Location::default() },
        )
        .expect("stock"),
    );
    h.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(mesh)));
    h.execute(&world, CreateOperation::new(&world, "OP", "BN-0600", SrcRef::Entity("bar".into()), params).expect("create"));
    world
}

/// The rotary offset's params (the wrap ladder), with the op's own
/// stated direction when one is given (#2772).
fn offset(stated_axis: Option<[f64; 3]>) -> OpParams {
    OpParams::RotaryOffset {
        links: camcraft_app::links::Links::ROUGH3D,
        tolerance: 0.1,
        stepover: 4.0,
        stock_to_leave: 0.5,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(-14.0),
        stepdown: 0.5,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: camcraft_app::machining2d::PocketStrategy::Auto,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
        stated_axis,
        picks: Default::default(),
    }
}

/// A world with the ball, a box stock about the mesh, the model
/// "bar" (the given mesh) and the op "OP" (the given params), on the
/// given mounted machine. The mesh lies along X or stands on Z
/// through the origin, so the stated line through the WCS origin is
/// its own axis.
fn world_on(mesh: Vec<Triangle>, params: OpParams, machine: Option<&str>) -> World {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == "BN-0600").map(|(_, t)| t).expect("catalog ball");
    h.execute(&world, AddTool::new(&world, "BN-0600", tool).expect("tool"));
    if let Some(m) = machine {
        h.execute(&world, LoadMachine::builtin(&world, m).expect("machine"));
    }
    let (dx, dz) = if mesh.first().is_some_and(|t| t.a.x.abs() > t.a.z.abs()) { (34.0, 14.0) } else { (14.0, 34.0) };
    h.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx, dy: 14.0, dz }, Location::default()).expect("stock"),
    );
    h.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(mesh)));
    h.execute(&world, CreateOperation::new(&world, "OP", "BN-0600", SrcRef::Entity("bar".into()), params).expect("create"));
    world
}

/// The op's generation, as the command runs it: the constructor stages
/// and runs the whole generation (its Err is the refusal's own words),
/// and the command is applied so the path lands in the cache.
fn generate(world: &World) -> Result<(), String> {
    let cmd = GenerateOperation::new(world, "OP").map_err(|e| e.to_string())?;
    History::default().execute(world, cmd);
    Ok(())
}

/// The op's cached moves, when a path exists.
fn moves(world: &World) -> Vec<camcraft_app::cl::Move> {
    let e = model::find_by_name(world, "OP").expect("op");
    model::toolpath_of(world, &e).expect("a cached path").path.moves
}

#[test]
fn a_stated_machine_line_generates_the_fitted_path() {
    // The microARC 4's stated A line ([1, 0, 0] through the work
    // offset origin) judged against a revolve ON it: the held line is
    // the very axis the fit reads, and the program is the fitted one
    // move for move. The verify's own read agrees, and the sim's
    // (`rotary_axis_of`) is the stated line by construction.
    let fitted = world_on(bar(0), parallel(None), None);
    generate(&fitted).expect("the fit reads the laid bar");
    let fitted = moves(&fitted);
    let stated = world_on(bar(0), parallel(None), Some(MICROARC));
    generate(&stated).expect("the stated A holds the laid bar");
    let stated_moves = moves(&stated);
    assert_eq!(fitted.len(), stated_moves.len(), "the stated axis generates the same program length");
    for (a, b) in fitted.iter().zip(&stated_moves) {
        assert_eq!((a.from, a.to, a.kind, a.axis, a.feed), (b.from, b.to, b.kind, b.axis, b.feed), "move for move the fitted path");
    }
    // The judge's own reading: the stated line and the free fit agree.
    let e = model::find_by_name(&stated, "OP").expect("op");
    let op = model::operation_of(&stated, &e).expect("operation");
    let read = fixture::rotary_axis_of(&stated, &op).expect("the stated axis answers the verify");
    let fit = multiaxis::axis::fit(&common::rotary_bar::laid()).expect("the bar is a revolve");
    assert_eq!([read.dir[0], read.dir[1], read.dir[2], read.origin.x, read.origin.y, read.origin.z], [fit.dir.x, fit.dir.y, fit.dir.z, fit.origin.x, fit.origin.y, fit.origin.z]);
}

#[test]
fn the_millturns_stated_c_carries_the_standing_bar() {
    // millturn.mch's spindle C states [0, 0, 1]: the standing bar ON
    // the spindle line holds it, the letter gate answers the machine
    // C without a fit, and the path states its C angle on every move
    // (the spindle-stamp law, issue #33 - unchanged).
    let world = world_on(bar(2), contour_at(None, 40.0), Some(MILLTURN));
    generate(&world).expect("the stated C holds the standing bar");
    let path = moves(&world);
    assert!(path.iter().all(|m| m.rotary.is_some()), "every move states its C");
    let e = model::find_by_name(&world, "OP").expect("op");
    let op = model::operation_of(&world, &e).expect("operation");
    let read = fixture::rotary_axis_of(&world, &op).expect("the stated axis answers the verify");
    assert_eq!(read.dir, [0.0, 0.0, 1.0], "the C's stated line, signed up");
    assert_eq!([read.origin.x, read.origin.y], [0.0, 0.0], "anchored through the work offset origin");
}

#[test]
fn the_screw_rotor_on_a_stated_line_is_refused_by_name() {
    // The reporter's part: clamped ON the stated A line, its lobes
    // helical. The radial raster would gouge on helical lobes (the
    // decision doc's law), so the disagreement refusal keeps the kind
    // locked - the radial drop, a separate slice of this issue, is the
    // coming answer. No path is cached.
    let world = world_on(screw(2.0), parallel(None), Some(MICROARC));
    let err = generate(&world).expect_err("the radial kinds stay locked on helical lobes");
    assert!(err.contains("disagree with the stated rotary line (A)"), "{err}");
    assert!(err.contains("share no rotation axis"), "{err}");
    assert!(err.contains("not supported yet (#2772)"), "{err}");
    let e = model::find_by_name(&world, "OP").expect("op");
    assert!(model::toolpath_of(&world, &e).is_none(), "no path is cached for a refused op");
}

#[test]
fn the_screw_rotor_without_a_stated_line_keeps_the_old_refusal() {
    // Nothing stated (no machine, no op direction): today's free fit
    // and its own words, untouched.
    let world = world_on(screw(2.0), parallel(None), None);
    let err = generate(&world).expect_err("the free fit refuses the helical lobes");
    assert!(err.contains("share no rotation axis"), "{err}");
    assert!(err.contains("not supported yet (#2772)"), "{err}");
}

#[test]
fn a_stated_line_away_from_the_part_names_the_parts_own_axis() {
    // A revolve and a stated line it does not turn about: the
    // disagreement refusal names where the part turns and the two
    // acts that fix it, never a silent fall back to the fit. With no
    // machine mounted the op's own statement is still judged (a
    // stated line is a fact about the work, whatever checks it).
    let world = world_on(bar(0), parallel(Some([0.0, 0.0, 1.0])), None);
    let err = generate(&world).expect_err("the laid bar does not turn about Z");
    assert!(err.contains("disagree with the stated rotary line"), "{err}");
    assert!(err.contains("they turn about their own axis"), "{err}");
    assert!(err.contains("Align the part on the stated line"), "{err}");
    assert!(err.contains("move the work offset"), "{err}");
    // A stated line the part HOLDS but the machine does not: the letter
    // door's own refusal - the microARC states an A about X, not the Z
    // the standing bar turns about.
    let world = world_on(bar(2), contour(Some([0.0, 0.0, 1.0])), Some(MICROARC));
    let err = generate(&world).expect_err("the machine states no rotary about Z");
    assert!(err.contains("states no rotary on that line"), "{err}");
}

#[test]
fn a_stated_line_the_part_holds_on_a_machine_that_has_it_generates() {
    // The op's own statement ON the part's axis, on the machine that
    // states it: the stated X generates the fitted path, exactly as
    // the machine's own line does.
    let fitted = world_on(bar(0), parallel(None), None);
    generate(&fitted).expect("the fit reads the laid bar");
    let fitted = moves(&fitted);
    let stated = world_on(bar(0), parallel(Some([-2.0, 0.0, 0.0])), Some(MICROARC));
    generate(&stated).expect("the op's own stated line holds, either sign");
    let stated_moves = moves(&stated);
    assert_eq!(fitted.len(), stated_moves.len());
    for (a, b) in fitted.iter().zip(&stated_moves) {
        assert_eq!((a.from, a.to), (b.from, b.to), "either sign of the direction is one line");
    }
}

#[test]
fn a_work_offset_move_moves_the_stated_line_and_refuses() {
    // The anchor is the work offset's own position: move it off the
    // part and the stated line moves with it - the refusal says the
    // part turns about its own axis and the two acts that fix it.
    let world = world_on(bar(0), parallel(None), Some(MICROARC));
    let mut h = History::default();
    h.execute(
        &world,
        SetWcsOrigin::new(&world, Location { pos: Point3::new(0.0, 30.0, 0.0), ..Location::default() }).expect("place"),
    );
    let err = generate(&world).expect_err("the stated line moved off the part with the offset");
    assert!(err.contains("disagree with the stated rotary line (A)"), "{err}");
    assert!(err.contains("they turn about their own axis"), "{err}");
    assert!(err.contains("move the work offset"), "{err}");
    // And with nothing stated, the same moved offset changes nothing:
    // the fit reads the part where it stands.
    let world = world_on(bar(0), parallel(None), None);
    let mut h = History::default();
    h.execute(
        &world,
        SetWcsOrigin::new(&world, Location { pos: Point3::new(0.0, 30.0, 0.0), ..Location::default() }).expect("place"),
    );
    generate(&world).expect("the free fit ignores the work offset");
}

#[test]
fn a_common_translation_does_not_defeat_the_stated_line_judge() {
    // The review of PR #3674's mutation, pinned: the model and the
    // box stock moved +10000 in Y, the op's own stated X direction
    // anchored through the work offset origin, clearance 100. The
    // judge reads its radii ABOUT THE STATED LINE: an origin ON the
    // moved part generates the fitted path, and 1 mm or 2 mm off in
    // Y refuses by name - the same disagreement the judge makes at
    // home, wherever the part sits.
    let off = [0.0, 10000.0, 0.0];
    let mesh = moved(&bar(0), off);
    let params = parallel_at(Some([1.0, 0.0, 0.0]), 100.0);
    // The GREEN control: the work offset on the moved part generates.
    let world = world_moved(mesh.clone(), params.clone(), off);
    let mut h = History::default();
    h.execute(
        &world,
        SetWcsOrigin::new(&world, Location { pos: Point3::new(off[0], off[1], off[2]), ..Location::default() })
            .expect("place"),
    );
    generate(&world).expect("the stated line runs through the moved part");
    assert!(!moves(&world).is_empty(), "the control generates a program");
    // 1 mm and 2 mm off: the refusal the judge makes at home, not a
    // tolerance that grows with the translation.
    for dy in [1.0, 2.0] {
        let world = world_moved(mesh.clone(), params.clone(), off);
        let mut h = History::default();
        h.execute(
            &world,
            SetWcsOrigin::new(&world, Location { pos: Point3::new(off[0], off[1] + dy, off[2]), ..Location::default() })
                .expect("place"),
        );
        let err = generate(&world).expect_err("the stated line stands off the moved part");
        assert!(err.contains("disagree with the stated rotary line"), "{err}");
        assert!(err.contains("they turn about their own axis"), "{err}");
    }
}

#[test]
fn the_stated_axis_round_trips_the_project_file() {
    // v81 (#2772): the op's stated direction saves, opens and reads
    // back whole; a v77 file (no key) opens as unstated, following the
    // machine, else the fit.
    let world = world_on(bar(0), parallel(Some([0.0, 1.0, 0.0])), None);
    let out = std::env::temp_dir().join(format!("camcraft_stated_axis_{}.camproj", std::process::id()));
    project::save(&world, &out).expect("save");
    let text = std::fs::read_to_string(&out).expect("the saved document");
    let _ = std::fs::remove_file(&out);
    assert!(text.contains("stated_axis"), "the stated direction is written: {text}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text).expect("json")["version"],
        serde_json::json!(project::PROJECT_VERSION),
        "the file states the current version"
    );
    let read_back = |text: &str| {
        let back = camcraft_app::world::new_world();
        project::open(&back, text).expect("open");
        let e = model::find_by_name(&back, "OP").expect("op");
        let op = model::operation_of(&back, &e).expect("operation");
        match op.params {
            OpParams::RotaryParallel { stated_axis, .. } => stated_axis,
            _ => panic!("the op kept its kind"),
        }
    };
    assert_eq!(read_back(&text), Some([0.0, 1.0, 0.0]), "the stated direction survives the round trip");
    // A v77 file (the key pried out of the saved document) opens as
    // unstated: absent means follow the machine, else the fit.
    let mut old: serde_json::Value = serde_json::from_str(&text).expect("json");
    old["version"] = serde_json::json!(77);
    for e in old["entities"].as_array_mut().expect("entities") {
        if e["kind"] == "operation" {
            e["operation"]["params"]["rotary_parallel"]
                .as_object_mut()
                .expect("the op's params")
                .remove("stated_axis");
        }
    }
    assert_eq!(read_back(&serde_json::to_string(&old).expect("json")), None, "absent is unstated");
}

#[test]
fn a_v77_file_opens_and_a_newer_file_is_refused() {
    // The v81 rule (#2772, reviewed in PR #3674): an older reader -
    // upstream's v78, and its v79 tip-form build - must refuse this
    // file rather than drop a stated axis and turn the part about one
    // the user never named. A v77 file steps up unchanged.
    const { assert!(project::PROJECT_VERSION >= 81, "the stated rotation axis needs a distinct version older readers refuse") };
    let world = world_on(bar(0), parallel(None), Some(MICROARC));
    let out = std::env::temp_dir().join(format!("camcraft_stated_version_{}.camproj", std::process::id()));
    project::save(&world, &out).expect("save");
    let text = std::fs::read_to_string(&out).expect("the saved document");
    let _ = std::fs::remove_file(&out);
    let saved: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(saved["version"], serde_json::json!(project::PROJECT_VERSION), "{text}");
    let mut future = saved.clone();
    future["version"] = serde_json::json!(project::PROJECT_VERSION + 1);
    let back = camcraft_app::world::new_world();
    let raw = serde_json::to_string(&future).expect("json");
    let err = project::open(&back, &raw).expect_err("a future version is refused");
    assert!(err.to_string().contains("unsupported project version"), "{err}");
}

#[test]
fn a_stated_axis_on_every_rotary_kind_needs_the_new_reader() {
    // The old-reader boundary, one op of each rotary kind (the review
    // of PR #3674): a file whose op states a rotation axis says v81,
    // a version upstream's v78 reader - and its v79 tip-form build -
    // refuses rather than drop the statement and re-save the op about
    // an axis the user never named.
    const { assert!(project::PROJECT_VERSION >= 81, "the stated rotation axis needs a distinct version older readers refuse") };
    for (kind, params) in [
        ("rotary_contour", contour(Some([1.0, 0.0, 0.0]))),
        ("rotary_parallel", parallel(Some([1.0, 0.0, 0.0]))),
        ("rotary_offset", offset(Some([1.0, 0.0, 0.0]))),
    ] {
        let world = world_on(bar(0), params, None);
        let out = std::env::temp_dir().join(format!("camcraft_stated_kind_{}_{}.camproj", kind, std::process::id()));
        project::save(&world, &out).expect("save");
        let text = std::fs::read_to_string(&out).expect("the saved document");
        let _ = std::fs::remove_file(&out);
        assert!(text.contains("stated_axis"), "{kind}: the stated direction is written: {text}");
        let saved: serde_json::Value = serde_json::from_str(&text).expect("json");
        assert_eq!(saved["version"], serde_json::json!(project::PROJECT_VERSION), "{kind}: {text}");
        assert!(
            saved["version"].as_u64().is_some_and(|v| v >= 81),
            "{kind}: a v79 reader must refuse a file that states a rotation axis"
        );
        // This build reopens its own file whole.
        let back = camcraft_app::world::new_world();
        project::open(&back, &text).expect("open");
        let e = model::find_by_name(&back, "OP").expect("op");
        let op = model::operation_of(&back, &e).expect("operation");
        let read = match op.params {
            OpParams::RotaryContour { stated_axis, .. }
            | OpParams::RotaryParallel { stated_axis, .. }
            | OpParams::RotaryOffset { stated_axis, .. } => stated_axis,
            _ => panic!("{kind}: the op kept its kind"),
        };
        assert_eq!(read, Some([1.0, 0.0, 0.0]), "{kind}: the stated direction survives the round trip");
    }
}
