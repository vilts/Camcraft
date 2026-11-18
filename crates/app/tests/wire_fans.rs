//! #1172: explicit fans generate, verify and post; stitched runs refuse by name.
//! Gate map: covers app::operation::wire_ruled, app::posting_wire.
//! Provenance: synthetic mesh fixtures from elementary ruled-surface geometry.

use camcraft_app::commands::{
    CreateOperation, GenerateOperation, History, ImportModel, LoadMachine,
};
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
        top_z: Height::fixed(H),
        bottom_z: Height::fixed(0.0),
        condition: "C110".into(),
        side: WireSide::Punch,
        start: Some([-30.0, 0.0]),
        tabs: None,
        capture: true,
        taper_deg: 0.0,
        burnout: None,
        clearance: operation::DEFAULT_CLEARANCE,
        skims: Vec::new(),
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

#[test]
fn stitched_planar_open_runs_are_refused_by_name() {
    let cs = corners();
    let (world, mut history) = world(loft(&cs, &cs));
    let entity = model::find_by_name(&world, "part").expect("part");
    let features = model::features_of(&world, &entity).expect("features");
    let mut picks = vec![face_at(&world, 0.0)];
    for chain in feature::chains(&features).into_iter().filter(|c| !c.closed) {
        let pts = feature::chain_points(&features, &chain);
        if pts.len() >= 2 && pts.iter().all(|p| (p[2] - H).abs() < 1e-9) {
            picks.push(GeoSel {
                entity: "part".into(),
                feature: FeatureSel::Chain(chain.id),
            });
        }
    }
    assert_eq!(
        picks.len(),
        5,
        "the square top is selected as four open edge runs"
    );
    let why = generate(
        &world,
        &mut history,
        picks,
        op(WirePairingRule::ArcLength, Vec::new()),
    )
    .expect_err("stitched runs refuse");
    assert!(
        why.contains("profile stitched from 4 open runs at Z24.000")
            && why.contains("not supported")
            && why.contains("closed edge chain"),
        "{why}"
    );
}
