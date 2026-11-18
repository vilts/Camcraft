//! #1917: "Your first router part" guide page, Sienci AltMill 4x4 + grblHAL
//! demo project (`examples/first-router-part.camproj`), RichAuto DSP A11
//! shop-router notes, and Getting started navigation links.
//!
//! Gate map: covers app::commands, app::project, app::posting, app::machine,
//! app::model, app::operation, app::pick, app::recognize, app::schema,
//! app::setup, app::world, app::geometry, geometry::geometry::stock,
//! motion::drilling, motion::machining2d, motion::machining3d, motion::links,
//! motion::load, tools::tools, post::post, sim::verify, assets/posts/grblhal.cps.
//!
//! Provenance: clean-room test suite over public CamCraft commands, shipped
//! Sienci AltMill 4x4 / grblHAL assets, and Zola guide sources.

use std::path::{Path, PathBuf};

use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel,
    LoadMachine, SelectPost, SetStock, SetWcsOrigin,
};
use camcraft_app::drilling::CycleChoice;
use camcraft_app::geometry::{IDENTITY_Q, Mesh, Point3, Triangle};
use camcraft_app::links::Links;
use camcraft_app::machining2d::{Lead, PocketStrategy, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{self, CoolantMode, FeatureSel, GeoSel, Height, HeightReference, OpParams, SrcRef};
use camcraft_app::pick::PickBody;
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, ToolKind, catalog};
use camcraft_app::world::new_world;
use camcraft_app::{machine, model, posting, project, recognize, setup};

const SHEET_DX: f64 = 200.0;
const SHEET_DY: f64 = 160.0;
const SHEET_DZ: f64 = 12.0;
const PART_HALF_X: f64 = 70.0;
const PART_HALF_Y: f64 = 50.0;
const POCKET_FLOOR_Z: f64 = 8.0;
const HOLE_R: f64 = 3.1; // apothem 3.1 * cos(pi/24) > 3.0 mm DR-0600 drill radius
const HOLE_SEGS: usize = 24;

fn repo(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(rel)
}

fn push_tri(out: &mut Vec<Triangle>, a: Point3, b: Point3, c: Point3, n: [f64; 3]) {
    let cross = (b - a).cross(c - a);
    let dot = cross.x * n[0] + cross.y * n[1] + cross.z * n[2];
    if dot.abs() >= 1e-12 {
        out.push(if dot > 0.0 { Triangle::new(a, b, c) } else { Triangle::new(a, c, b) });
    }
}

fn push_quad(out: &mut Vec<Triangle>, q: [Point3; 4], n: [f64; 3]) {
    push_tri(out, q[0], q[1], q[2], n);
    push_tri(out, q[0], q[2], q[3], n);
}

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// Cell `[cx-hx, cx+hx] x [cy-hy, cy+hy]` around one Ø6.2 mm through-hole without T-junctions.
fn add_hole_cell(out: &mut Vec<Triangle>, cx: f64, cy: f64, hx: f64, hy: f64) {
    let ring: Vec<(f64, f64)> = (0..HOLE_SEGS)
        .map(|k| {
            let a = std::f64::consts::TAU * (k as f64) / (HOLE_SEGS as f64);
            (cx + HOLE_R * a.cos(), cy + HOLE_R * a.sin())
        })
        .collect();
    let corners = [(cx + hx, cy + hy), (cx - hx, cy + hy), (cx - hx, cy - hy), (cx + hx, cy - hy)];
    let (q, hq) = (HOLE_SEGS / 4, HOLE_SEGS / 8);
    for side in 0..4 {
        let (ca, cb, mid) = (corners[(side + 3) % 4], corners[side], (side * q) % HOLE_SEGS);
        let i0 = (mid + HOLE_SEGS - hq) % HOLE_SEGS;
        for (z, nz) in [(SHEET_DZ, 1.0), (0.0, -1.0)] {
            let n = [0.0, 0.0, nz];
            push_tri(out, p3(ca.0, ca.1, z), p3(cb.0, cb.1, z), p3(ring[mid].0, ring[mid].1, z), n);
            for step in 0..hq {
                let (a, b) = (ring[(mid + step) % HOLE_SEGS], ring[(mid + step + 1) % HOLE_SEGS]);
                push_tri(out, p3(cb.0, cb.1, z), p3(b.0, b.1, z), p3(a.0, a.1, z), n);
                let idx = (i0 + step) % HOLE_SEGS;
                let (c, d) = (ring[idx], ring[(idx + 1) % HOLE_SEGS]);
                push_tri(out, p3(ca.0, ca.1, z), p3(d.0, d.1, z), p3(c.0, c.1, z), n);
            }
        }
    }
    for k in 0..HOLE_SEGS {
        let ((ax, ay), (bx, by)) = (ring[k], ring[(k + 1) % HOLE_SEGS]);
        push_quad(
            out,
            [p3(ax, ay, 0.0), p3(bx, by, 0.0), p3(bx, by, SHEET_DZ), p3(ax, ay, SHEET_DZ)],
            [cx - (ax + bx) * 0.5, cy - (ay + by) * 0.5, 0.0],
        );
    }
}

/// Watertight 140 x 100 x 12 mm bracket plate: central 50 x 30 x 4 mm pocket + 4 corner through-holes.
fn router_plate() -> Mesh {
    let xs = [-PART_HALF_X, -57.0, -37.0, -25.0, 25.0, 37.0, 57.0, PART_HALF_X];
    let ys = [-PART_HALF_Y, -40.0, -20.0, -15.0, 15.0, 20.0, 40.0, PART_HALF_Y];
    let is_hole = |i: usize, j: usize| (i == 1 || i == 5) && (j == 1 || j == 5);
    let top_z = |i: isize, j: isize| {
        if !(0..7).contains(&i) || !(0..7).contains(&j) { 0.0 }
        else if i == 3 && j == 3 { POCKET_FLOOR_Z }
        else { SHEET_DZ }
    };
    let mut f = Vec::new();
    for i in 0..7 {
        for j in 0..7 {
            let (x0, x1, y0, y1) = (xs[i], xs[i + 1], ys[j], ys[j + 1]);
            if is_hole(i, j) {
                add_hole_cell(&mut f, (x0 + x1) * 0.5, (y0 + y1) * 0.5, 10.0, 10.0);
            } else {
                let zt = top_z(i as isize, j as isize);
                push_quad(&mut f, [p3(x0, y0, zt), p3(x1, y0, zt), p3(x1, y1, zt), p3(x0, y1, zt)], [0.0, 0.0, 1.0]);
                push_quad(&mut f, [p3(x0, y0, 0.0), p3(x1, y0, 0.0), p3(x1, y1, 0.0), p3(x0, y1, 0.0)], [0.0, 0.0, -1.0]);
            }
        }
    }
    for (i, &x) in xs.iter().enumerate() {
        for j in 0..7 {
            let (l, r) = (top_z(i as isize - 1, j as isize), top_z(i as isize, j as isize));
            if (l - r).abs() > 1e-9 {
                let (lo, hi) = (l.min(r), l.max(r));
                let n = if l > r { [1.0, 0.0, 0.0] } else { [-1.0, 0.0, 0.0] };
                push_quad(&mut f, [p3(x, ys[j], lo), p3(x, ys[j + 1], lo), p3(x, ys[j + 1], hi), p3(x, ys[j], hi)], n);
            }
        }
    }
    for (j, &y) in ys.iter().enumerate() {
        for i in 0..7 {
            let (d, u) = (top_z(i as isize, j as isize - 1), top_z(i as isize, j as isize));
            if (d - u).abs() > 1e-9 {
                let (lo, hi) = (d.min(u), d.max(u));
                let n = if d > u { [0.0, 1.0, 0.0] } else { [0.0, -1.0, 0.0] };
                push_quad(&mut f, [p3(xs[i], y, lo), p3(xs[i + 1], y, lo), p3(xs[i + 1], y, hi), p3(xs[i], y, hi)], n);
            }
        }
    }
    Mesh::from_facets(f)
}

fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).unwrap_or_else(|| panic!("catalog {name}"))
}

fn outer_bottom_chains(facets: std::sync::Arc<[Triangle]>) -> Vec<GeoSel> {
    let body = PickBody::new("router-part", facets, Location::default());
    let on_rim = |p: [f64; 3]| p[2].abs() < 1e-4 && ((p[0].abs() - PART_HALF_X).abs() < 1e-4 || (p[1].abs() - PART_HALF_Y).abs() < 1e-4);
    body.graph.chains().iter().filter(|c| {
        !c.edges.is_empty() && c.edges.iter().all(|e| body.graph.edge_ends(*e).is_some_and(|(a, b)| on_rim(a) && on_rim(b)))
    }).map(|c| GeoSel { entity: "router-part".into(), feature: FeatureSel::Chain(c.id) }).collect()
}

fn build_demo_world() -> flecs_ecs::prelude::World {
    let world = new_world();
    let mut h = History::default();
    let mesh = router_plate();
    let facets = mesh.facets.clone();
    h.execute(&world, ImportModel::from_mesh("router-part", mesh));
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: SHEET_DX, dy: SHEET_DY, dz: SHEET_DZ }, Location::default()).expect("stock"));
    h.execute(&world, LoadMachine::builtin(&world, "Sienci AltMill 4x4").expect("altmill 4x4"));
    h.execute(&world, SelectPost::new(&world, PostRef::Named("grblHAL".into())).expect("grblhal"));
    h.execute(&world, SetWcsOrigin::new(&world, Location { pos: Point3::new(-SHEET_DX * 0.5, -SHEET_DY * 0.5, SHEET_DZ), rot: IDENTITY_Q }).expect("g54"));
    h.execute(&world, AddTool::new(&world, "DR-0600", catalog_tool("DR-0600")).expect("drill"));
    h.execute(&world, AddTool::new(&world, "EM-0600-4FL", catalog_tool("EM-0600-4FL")).expect("end mill"));

    let top_clr = Height::of(HeightReference::TopOfStock, 15.0);
    let top_ret = Height::of(HeightReference::TopOfStock, 5.0);
    let drill_base = OpParams::Drill {
        break_through: None,
        clearance_z: top_clr, retract_z: Some(top_ret), top_z: Height::of(HeightReference::TopOfStock, 0.0), bottom_z: None,
        cycle: CycleChoice::Auto, peck: 4.0, dwell: 0.0, feed: 150.0, plunge_feed: 150.0, holes: Vec::new(), cross: Vec::new(),
    };
    let detected = operation::detect_holes(&world, "Drill1", &drill_base, None, &SrcRef::Entity("router-part".into())).expect("holes");
    h.execute(&world, CreateOperation::new(&world, "Drill1", "DR-0600", SrcRef::Entity("router-part".into()), detected.params).expect("drill op"));
    h.execute(&world, EditOperationParams::with_coolant(&world, "Drill1", CoolantMode::Off).expect("dry drill"));
    h.execute(&world, GenerateOperation::new(&world, "Drill1").expect("generate drill"));

    let pocket = recognize::pockets_of(&world, "router-part").expect("pockets").into_iter().next().expect("pocket");
    let pocket_src = SrcRef::Selection(vec![GeoSel { entity: "router-part".into(), feature: FeatureSel::Face(pocket.floor) }]);
    let pocket_params = OpParams::Pocket {
        rest: None,
        stepover: 3.0, stock_to_leave: 0.0, clearance_z: top_clr, retract_z: Some(top_ret),
        top_z: Height::of(HeightReference::TopOfModel, 0.0), bottom_z: Height::of(HeightReference::TopOfModel, -4.0),
        stepdown: 2.0, feed: 1800.0, plunge_feed: 400.0, climb: true, strategy: PocketStrategy::Waterline,
        entry: Entry3D::Helix, helix_radius: 1.2, links: Links::PLANAR, optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false, blend_radius: None, tilt: None,
    };
    h.execute(&world, CreateOperation::new(&world, "2D Pocket1", "EM-0600-4FL", pocket_src, pocket_params).expect("pocket op"));
    h.execute(&world, EditOperationParams::with_coolant(&world, "2D Pocket1", CoolantMode::Off).expect("dry pocket"));
    h.execute(&world, GenerateOperation::new(&world, "2D Pocket1").expect("generate pocket"));

    let contour_picks = outer_bottom_chains(facets);
    assert!(!contour_picks.is_empty(), "outer bottom chains found");
    let contour_params = OpParams::Contour {
        stock_to_leave: 0.0, clearance_z: top_clr, retract_z: top_ret,
        top_z: Height::of(HeightReference::TopOfStock, 0.0), bottom_z: Height::of(HeightReference::BottomOfStock, 0.0),
        stepdown: 4.0, feed: 1800.0, plunge_feed: 400.0, climb: true, side: Side::Outside,
        lead: Lead::Arc { radius: 3.0 }, tolerance: 0.01, tab_count: 4, tab_width: 8.0, tab_height: 3.0,
        entry: Entry3D::Plunge, helix_radius: 1.0, blend_turns: false, tilt: None, links: Links::PLANAR,
    };
    h.execute(&world, CreateOperation::new(&world, "2D Contour1", "EM-0600-4FL", SrcRef::Selection(contour_picks), contour_params).expect("contour op"));
    h.execute(&world, EditOperationParams::with_coolant(&world, "2D Contour1", CoolantMode::Off).expect("dry contour"));
    h.execute(&world, GenerateOperation::new(&world, "2D Contour1").expect("generate contour"));
    world
}

#[test]
fn first_router_part_project_loads_verifies_and_posts_to_grblhal() {
    let demo_path = repo("examples/first-router-part.camproj");
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        // The demo is the command build below, saved (#1917's decision):
        // a generator change regenerates it, as for every golden.
        project::save(&build_demo_world(), &demo_path).expect("write the demo project");
    }
    assert!(demo_path.exists(), "expected committed router demo project at {}", demo_path.display());

    let world = new_world();
    let notes = project::load_noted(&world, &demo_path).expect("load demo project");
    assert!(notes.is_empty(), "demo project should load without migration notes: {notes:?}");

    let (mch_name, mch) = machine::of(&world);
    assert_eq!(mch.as_ref().map(|m| m.model.as_str()), Some("Sienci AltMill 4x4"), "machine {mch_name}");
    assert_eq!(posting::post_of(&world), PostRef::Named("grblHAL".into()));

    let stock_e = setup::stock_entity(&world, None).expect("sheet stock entity");
    assert!(
        matches!(model::stock_def_of(&world, &stock_e), Some(StockShape::Box { dx, dy, dz }) if dx >= 160.0 && dy >= 120.0 && dz > 0.0),
        "demo uses box sheet stock"
    );
    let wcs_e = setup::wcs_entity(&world, None).expect("G54 WCS");
    let wcs = model::transform_of(&world, &wcs_e).expect("WCS transform");
    assert!(
        (wcs.pos.x + SHEET_DX * 0.5).abs() < 1e-6 && (wcs.pos.y + SHEET_DY * 0.5).abs() < 1e-6,
        "corner G54 placed at front-left stock corner, got {:?}",
        wcs.pos
    );

    let ops = setup::operation_order(&world);
    let names: Vec<&str> = ops.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["Drill1", "2D Pocket1", "2D Contour1"]);

    let has_flat_end_mill = model::named_entities(&world).into_iter().any(|(_, e)| {
        model::tool_of(&world, &e).is_some_and(|t| t.kind == ToolKind::EndMill && t.corner_r == 0.0)
    });
    assert!(has_flat_end_mill, "demo selects a flat end mill");

    let contour_e = model::find_by_name(&world, "2D Contour1").expect("2D Contour1");
    let contour = model::operation_of(&world, &contour_e).expect("contour op");
    assert!(
        matches!(contour.params, OpParams::Contour { tab_count, tab_width, tab_height, .. } if tab_count >= 4 && tab_width > 0.0 && tab_height > 0.0),
        "2D Contour1 keeps holding tabs"
    );

    let gate = posting::verification_gate(&world);
    assert!(gate.is_empty(), "verification gate must be silent: {gate:?}");

    let out = std::env::temp_dir().join(format!("camcraft_first_router_{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    let posted = posted.unwrap_or_else(|e| panic!("first router demo must post cleanly: {e}"));
    for expected in ["G21", "G90", "G91.1", "G54", "G53 G0 Z0", "M6", "M30"] {
        assert!(posted.program.contains(expected), "missing {expected} in:\n{}", posted.program);
    }
    for forbidden in ["G43", "G49", "G41", "G42", "G84", "%"] {
        assert!(!posted.program.contains(forbidden), "forbidden grblHAL word {forbidden} in:\n{}", posted.program);
    }
    assert!(
        posted.program.lines().any(|line| line.starts_with("G73 ") && line.split_whitespace().any(|word| word == "Z-13")),
        "demo drill breaks through the 12 mm stock by 1 mm at G54 Z-13"
    );

    let rebuilt = build_demo_world();
    let out2 = std::env::temp_dir().join(format!("camcraft_first_router_rebuilt_{}.nc", std::process::id()));
    let posted2 = posting::post_setup(&rebuilt, &out2);
    let _ = std::fs::remove_file(&out2);
    let posted2 = posted2.unwrap_or_else(|e| panic!("rebuilt router demo must post cleanly: {e}"));
    assert_eq!(posted.program, posted2.program, "committed demo matches command build");
}

#[test]
fn first_router_part_guide_and_navigation_links_exist_and_resolve() {
    let guide_rel = "site/content/guide/getting-started/first-router-part.md";
    let guide_path = repo(guide_rel);
    let guide = std::fs::read_to_string(&guide_path)
        .unwrap_or_else(|e| panic!("missing guide page {}: {e}", guide_path.display()));

    for required in [
        "title =",
        "weight =",
        "Sienci AltMill",
        "grblHAL",
        "grblhal.cps",
        "examples/first-router-part.camproj",
        "G54",
        "2D Pocket",
        "Drill",
        "2D Contour",
        "Tab count",
        "Tab width",
        "Tab height",
        "Simulate",
        "Deviate",
        "G41",
        "G84",
        "RichAuto DSP A11",
        "FAT32",
        "8.3",
        "AbsCntr = Off",
        "C.A.D Thickness",
        "#1902",
    ] {
        assert!(guide.contains(required), "{guide_rel} missing '{required}'");
    }

    let mut rest = guide.as_str();
    while let Some(start) = rest.find("@/") {
        let after = &rest[start + 2..];
        let end = after.find(|c: char| c == ')' || c == '#' || c.is_whitespace()).unwrap_or(after.len());
        let target = &after[..end];
        assert!(repo(&format!("site/content/{target}")).exists(), "broken internal Zola link @/{target} in {guide_rel}");
        rest = &after[end..];
    }

    for nav_rel in [
        "site/content/guide/getting-started/_index.md",
        "site/content/guide/getting-started/workflows.md",
    ] {
        let text = std::fs::read_to_string(repo(nav_rel)).expect(nav_rel);
        assert!(text.contains("@/guide/getting-started/first-router-part.md"), "{nav_rel} must link to first-router-part.md");
    }

    let machines = std::fs::read_to_string(repo("site/content/guide/check-and-post/machines.md")).expect("machines.md");
    let router_sec = machines.split("## Router").nth(1).expect("## Router section in machines.md");
    assert!(router_sec.contains("@/guide/getting-started/first-router-part.md"), "machines.md ## Router must link to first-router-part.md");

    let nav_test = std::fs::read_to_string(repo("site/tests/test_guide_navigation.py")).expect("test_guide_navigation.py");
    assert!(nav_test.contains("first-router-part"), "test_guide_navigation.py must cover guide/getting-started/first-router-part");
}

#[test]
fn first_router_part_richauto_zeroing_uses_xy0_with_oem_source() {
    let guide = std::fs::read_to_string(repo("site/content/guide/getting-started/first-router-part.md")).expect("router guide");
    let richauto = guide.split("## Shop routers with RichAuto DSP A11").nth(1).expect("RichAuto section");
    let calibration = richauto.split("- **Calibrate your Z touch-off puck").nth(1).expect("calibration instructions")
        .split("- **Use a single-tool").next().expect("calibration bullet");
    assert!(calibration.contains("**`XY→0`**"), "set X/Y zero with XY→0");
    assert!(!calibration.contains("press **`ORIGIN`**"), "ORIGIN returns to the existing origin; it does not set X/Y zero");
    for required in [
        "https://www.richautocontroller.com/wp-content/uploads/2023/08/DSP-A11-Manual.pdf",
        "2.3",
        "5.4.1",
    ] {
        assert!(richauto.contains(required), "cite OEM A11 manual sections 2.3 and 5.4.1: missing {required}");
    }
}

#[test]
fn first_router_part_guide_states_drill_breakthrough_and_clearance() {
    let guide = std::fs::read_to_string(repo("site/content/guide/getting-started/first-router-part.md")).expect("router guide");
    let origin = guide.split("4. **Set a corner work offset").nth(1).expect("G54 setup instructions")
        .split("## Tool selection").next().expect("setup step");
    for required in ["-12 mm", "-13 mm", "1 mm", "12 mm"] {
        assert!(origin.contains(required), "G54 depth summary must include drill breakthrough: missing {required}");
    }
    let drill = guide.split("1. **`Drill1`").nth(1).expect("drilling instructions")
        .split("2. **`2D Pocket1`").next().expect("drilling step");
    for required in ["-13 mm", "1 mm", "12 mm"] {
        assert!(drill.contains(required), "drilling instructions must state breakthrough depth: missing {required}");
    }
    let checks = guide.split("## Simulate, check clearances and post to `.nc`").nth(1).expect("clearance checklist")
        .split("Click **Post**").next().expect("clearance instructions");
    let clearance = checks.split("\n\n").flat_map(|paragraph| paragraph.split("\n- "))
        .find(|bullet| bullet.contains("spoilboard")).expect("spoilboard clearance check");
    for required in ["drill", "-13 mm", "-12 mm", "1 mm"] {
        assert!(clearance.contains(required), "spoilboard clearance must include drill breakthrough and contour depth: missing {required}");
    }
    assert!(clearance.contains("fastener") || clearance.contains("screw"), "check fastener clearance to the drill breakthrough depth");
}
