//! #1172: explicit fans generate, verify and post. #3209: stitched runs
//! and a die face's opening are 4-axis profiles; runs that do not close
//! refuse by name.
//! Gate map: covers app::operation::wire_ruled, app::posting_wire.
//! Provenance: synthetic mesh fixtures from elementary ruled-surface geometry.

use camcraft_app::commands::{
    CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::geometry::{feature, Mesh, Point3, Triangle};
use camcraft_app::machine::parse_mch;
use camcraft_app::model;
use camcraft_app::operation::{
    self, FeatureSel, GeoSel, Height, OpParams, SrcRef, WireContourOp, WireFourAxis,
    WirePairingRule, WireSide, WireSyncPair,
};
use camcraft_app::posting;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

const H: f64 = 24.0;
const FIXTURE: &str = include_str!("edm/wire-iso-fixture.mch");
type P2 = (f64, f64);

fn at(p: P2, z: f64) -> Point3 {
    Point3::new(p.0, p.1, z)
}

fn cap(out: &mut Vec<Triangle>, ring: &[P2], z: f64, up: bool) {
    for j in 0..ring.len() {
        let (p, q) = (at(ring[j], z), at(ring[(j + 1) % ring.len()], z));
        if (p.x - q.x).hypot(p.y - q.y) < 1e-9 {
            continue;
        }
        let zero = Point3::new(0.0, 0.0, z);
        out.push(if up {
            Triangle::new(zero, p, q)
        } else {
            Triangle::new(zero, q, p)
        });
    }
}

fn loft(lower: &[P2], upper: &[P2]) -> Mesh {
    let mut facets = Vec::new();
    cap(&mut facets, lower, 0.0, false);
    cap(&mut facets, upper, H, true);
    for j in 0..lower.len() {
        let k = (j + 1) % lower.len();
        let (a, b, c, d) = (
            at(lower[j], 0.0),
            at(lower[k], 0.0),
            at(upper[k], H),
            at(upper[j], H),
        );
        if (a.x - b.x).hypot(a.y - b.y) < 1e-9 {
            facets.push(Triangle::new(a, c, d));
        } else {
            let middle = |p: Point3, q: Point3| ((p.x + q.x) * 0.5).hypot((p.y + q.y) * 0.5);
            if middle(a, c) <= middle(b, d) {
                facets.push(Triangle::new(a, b, c));
                facets.push(Triangle::new(a, c, d));
            } else {
                facets.push(Triangle::new(a, b, d));
                facets.push(Triangle::new(b, c, d));
            }
        }
    }
    Mesh::from_facets(facets)
}

fn corners() -> [P2; 4] {
    [(8.0, 8.0), (-8.0, 8.0), (-8.0, -8.0), (8.0, -8.0)]
}

fn fan_mesh() -> Mesh {
    let mut upper = Vec::new();
    let mut lower = Vec::new();
    let cs = corners();
    for j in 0..64 {
        let degrees = j as f64 * 360.0 / 64.0;
        let angle = degrees.to_radians();
        upper.push((9.6 * angle.cos(), 9.6 * angle.sin()));
        let phase = (degrees - 22.5).rem_euclid(360.0) / 90.0;
        let index = phase.floor() as usize;
        let f = ((phase.fract() - 0.5) * 2.0).max(0.0);
        let (a, b) = (cs[index], cs[(index + 1) % 4]);
        lower.push((a.0 + f * (b.0 - a.0), a.1 + f * (b.1 - a.1)));
    }
    loft(&lower, &upper)
}

fn op(pairing: WirePairingRule, sync: Vec<WireSyncPair>) -> WireContourOp {
    WireContourOp {
        lead_arc_radius: None,
        top_z: Height::fixed(H),
        bottom_z: Height::fixed(0.0),
        condition: "C110".into(),
        side: WireSide::Punch,
        picks: camcraft_app::path_picks::PathPicks { start: vec![camcraft_app::path_picks::PathPick::xy(camcraft_app::geo2d::Point2::new(-30.0, 0.0))], exit: Vec::new() },
        tabs: None,
        capture: true,
        both_pieces_restrained: false,
        stock_centered_in_xy_travel: false,
        taper_deg: 0.0,
        burnout: None,
        clearance: operation::DEFAULT_CLEARANCE,
        skims: Vec::new(),
        sharp_corners: false,
        four_axis: Some(WireFourAxis {
            pairing,
            sync,
            tolerance: 0.5,
            ..WireFourAxis::default()
        }),
    }
}

fn world(mesh: Mesh) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let fixture = FIXTURE.replace(
        "\"min\": -5.0, \"max\": 5.0",
        "\"min\": -50.0, \"max\": 50.0",
    );
    let machine = parse_mch(&fixture).expect("synthetic fixture");
    history.execute(
        &world,
        LoadMachine::new(&world, "wire-iso.mch".into(), Some(machine)).expect("mount"),
    );
    history.execute(
        &world,
        camcraft_app::commands::AddTool::new(&world, "EDM-W025", camcraft_app::wire_tools::wire_tool(0.25))
            .expect("wire"),
    );
    history.execute(&world, ImportModel::from_mesh("part", mesh));
    (world, history)
}

fn face_at(world: &World, z: f64) -> GeoSel {
    let entity = model::find_by_name(world, "part").expect("part");
    let features = model::features_of(world, &entity).expect("features");
    let face = features
        .faces
        .iter()
        .filter(|f| f.normal[2].abs() > 0.99)
        .filter(|f| {
            f.loops
                .iter()
                .flat_map(|l| &l.verts)
                .any(|v| (features.verts[*v as usize][2] - z).abs() < 1e-9)
        })
        .max_by(|a, b| a.area.total_cmp(&b.area))
        .expect("horizontal face");
    GeoSel {
        entity: "part".into(),
        feature: FeatureSel::Face(face.id),
    }
}

fn generate(
    world: &World,
    history: &mut History,
    picks: Vec<GeoSel>,
    wire: WireContourOp,
) -> Result<(), String> {
    let command = CreateOperation::new(
        world,
        "Fan",
        "EDM-W025",
        SrcRef::Selection(picks),
        OpParams::WireContour(wire),
    )
    .expect("operation");
    history.execute(world, command);
    GenerateOperation::new(world, "Fan")
        .map(|command| history.execute(world, command))
        .map_err(|e| e.to_string())
}

#[test]
fn square_corner_fans_generate_verify_and_post() {
    let (world, mut history) = world(fan_mesh());
    let picks = vec![face_at(&world, H), face_at(&world, 0.0)];
    let mut sync = Vec::new();
    for (j, lower) in corners().iter().enumerate() {
        for angle in [22.5 + j as f64 * 90.0, 67.5 + j as f64 * 90.0] {
            let angle = angle.to_radians();
            sync.push(WireSyncPair {
                upper: Some([9.6 * angle.cos(), 9.6 * angle.sin()]),
                lower: Some([lower.0, lower.1]),
            });
        }
    }
    generate(
        &world,
        &mut history,
        picks,
        op(WirePairingRule::SyncPoints, sync),
    )
    .expect("explicit fans generate");
    let entity = model::find_by_name(&world, "Fan").expect("operation");
    let toolpath = model::toolpath_of(&world, &entity)
        .expect("generated path")
        .path;
    let fans = toolpath
        .moves
        .windows(2)
        .filter(|m| {
            (m[0].to.x - m[1].to.x).hypot(m[0].to.y - m[1].to.y) < 1e-9 && m[0].axis != m[1].axis
        })
        .count();
    assert!(
        fans >= 4,
        "all four lower apices carry changing wire poses: {fans}"
    );
    let clause = posting::wire_clause(&world);
    assert!(clause.problems.is_empty(), "{:?}", clause.problems);
    let script = camcraft_app::posting::shipped_posts().expect("the shipped posts read")
        .iter()
        .find(|(name, _)| *name == "Generic ISO Wire EDM")
        .map(|(_, text)| *text)
        .expect("wire post");
    let output = std::env::temp_dir().join(format!("camcraft-fans-{}.nc", std::process::id()));
    let result = posting::post_script(&world, &output, script);
    let wrote = output.exists();
    let _ = std::fs::remove_file(&output);
    let program = result.expect("verified fans post").program;
    assert!(wrote, "the NC program is written");
    assert!(program.contains("PAIRED BY SYNC POINTS"), "{program}");
    assert!(
        program
            .lines()
            .any(|l| l.starts_with("G1 X") && l.contains(" U") && l.contains(" V")),
        "{program}"
    );
}

/// Every open edge run of the part that lies flat at `z`.
fn open_runs_at(world: &World, z: f64) -> Vec<GeoSel> {
    let entity = model::find_by_name(world, "part").expect("part");
    let features = model::features_of(world, &entity).expect("features");
    feature::chains(&features)
        .into_iter()
        .filter(|c| !c.closed)
        .filter(|c| {
            let pts = feature::chain_points(&features, c);
            pts.len() >= 2 && pts.iter().all(|p| (p[2] - z).abs() < 1e-9)
        })
        .map(|c| GeoSel {
            entity: "part".into(),
            feature: FeatureSel::Chain(c.id),
        })
        .collect()
}

fn verified(world: &World) {
    let clause = posting::wire_clause(world);
    assert!(clause.problems.is_empty(), "{:?}", clause.problems);
}

#[test]
fn stitched_planar_open_runs_generate_and_verify() {
    // #3209: an outline of several open runs on one plane is one ring.
    let cs = corners();
    let (world, mut history) = world(loft(&cs, &cs));
    let top = open_runs_at(&world, H);
    assert_eq!(top.len(), 4, "the square top is selected as four open edge runs");
    let mut picks = vec![face_at(&world, 0.0)];
    picks.extend(top);
    generate(&world, &mut history, picks, op(WirePairingRule::ArcLength, Vec::new()))
        .expect("the stitched top ring and the bottom face generate");
    verified(&world);
}

#[test]
fn stitched_runs_on_both_planes_generate_and_verify() {
    let cs = corners();
    let (world, mut history) = world(loft(&cs, &cs));
    let mut picks = open_runs_at(&world, H);
    let bottom = open_runs_at(&world, 0.0);
    assert_eq!(bottom.len(), 4, "the square bottom is selected as four open edge runs");
    picks.extend(bottom);
    generate(&world, &mut history, picks, op(WirePairingRule::ArcLength, Vec::new()))
        .expect("two stitched rings generate");
    verified(&world);
}

#[test]
fn open_runs_that_do_not_close_are_refused_by_name() {
    let cs = corners();
    let (world, mut history) = world(loft(&cs, &cs));
    let mut picks = vec![face_at(&world, 0.0)];
    picks.extend(open_runs_at(&world, H).into_iter().take(3));
    let why = generate(&world, &mut history, picks, op(WirePairingRule::ArcLength, Vec::new()))
        .expect_err("three sides of a square are no ring");
    assert!(why.contains("3 open runs at Z24.000") && why.contains("do not close"), "{why}");
}

/// A 40 mm round block with a tapered through opening: 16 mm across on
/// top, 12 mm at the bottom (no sharp inside corner for the wire). Its
/// top and bottom faces carry two rings.
fn die_block() -> Mesh {
    const N: usize = 32;
    let round = |r: f64| -> Vec<P2> {
        (0..N)
            .map(|j| {
                let a = (j as f64 * 360.0 / N as f64).to_radians();
                (r * a.cos(), r * a.sin())
            })
            .collect()
    };
    let (outer, top, bottom) = (round(20.0), round(8.0), round(6.0));
    let mut f = Vec::new();
    for j in 0..N {
        let k = (j + 1) % N;
        // The top and bottom caps: one quad from the outer side to the opening.
        for (hole, z, up) in [(&top, H, true), (&bottom, 0.0, false)] {
            let (a, b, c, d) = (at(outer[j], z), at(outer[k], z), at(hole[k], z), at(hole[j], z));
            if up {
                f.push(Triangle::new(a, b, c));
                f.push(Triangle::new(a, c, d));
            } else {
                f.push(Triangle::new(a, c, b));
                f.push(Triangle::new(a, d, c));
            }
        }
        // The outer walls face out; the walls of the opening face in.
        let (a, b, c, d) = (at(outer[j], 0.0), at(outer[k], 0.0), at(outer[k], H), at(outer[j], H));
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
        let (a, b, c, d) = (at(bottom[j], 0.0), at(bottom[k], 0.0), at(top[k], H), at(top[j], H));
        f.push(Triangle::new(a, c, b));
        f.push(Triangle::new(a, d, c));
    }
    Mesh::from_facets(f)
}

#[test]
fn die_face_reads_its_one_hole_ring_and_verifies() {
    // #3209: a die's top and bottom faces carry the block's outer ring and
    // the opening; the die side reads the opening.
    let (world, mut history) = world(die_block());
    let stock = SetStock::new(&world, StockShape::Box { dx: 48.0, dy: 48.0, dz: H }, Location::default())
        .expect("stock");
    history.execute(&world, stock);
    let picks = vec![face_at(&world, H), face_at(&world, 0.0)];
    let wire = WireContourOp {
        side: WireSide::Die,
        picks: camcraft_app::path_picks::PathPicks { start: vec![camcraft_app::path_picks::PathPick::xy(camcraft_app::geo2d::Point2::new(0.0, 0.0))], exit: Vec::new() },
        ..op(WirePairingRule::ArcLength, Vec::new())
    };
    generate(&world, &mut history, picks, wire).expect("the opening of the two faces generates");
    let entity = model::find_by_name(&world, "Fan").expect("operation");
    let path = model::toolpath_of(&world, &entity).expect("generated path").path;
    let reach = path.moves.iter().map(|m| m.to.x.abs().max(m.to.y.abs())).fold(0.0, f64::max);
    assert!(reach < 8.5, "the wire cuts the opening, not the outer ring: it reaches {reach:.3}");
    verified(&world);
}

#[test]
fn punch_face_with_an_opening_is_refused_by_name() {
    // A punch states no rule for which of the two rings it cuts.
    let (world, mut history) = world(die_block());
    let picks = vec![face_at(&world, H), face_at(&world, 0.0)];
    let why = generate(&world, &mut history, picks, op(WirePairingRule::ArcLength, Vec::new()))
        .expect_err("a punch does not pick one of two rings");
    assert!(why.contains("has 2 closed rings (1 openings)") && why.contains("edge chains"), "{why}");
}
