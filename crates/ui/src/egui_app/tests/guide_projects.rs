//! Deterministic clean-room demo projects for the User Guide (#1902).
//!
//! Provenance: original parametric meshes and CamCraft app commands seeded
//! with zero third-party or vendor geometry.

use std::path::{Path, PathBuf};

use camcraft_app::accessory::Accessory;
use camcraft_app::commands::{
    AccessoryPlacement, AddTool, Command, CreateOperation, CreateSetup, GenerateOperation, History,
    ImportModel, InsertAccessory, InsertVise, LoadMachine, SetStock, SetWcsOrigin, VisePlacement,
};
use camcraft_app::finishing3d::FinishPattern;
use camcraft_app::geometry::{IDENTITY_Q, Mesh, Point3, Triangle, cube};
use camcraft_app::links::Links;
use camcraft_app::machining2d::{Lead, PocketStrategy, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef, Tilt};
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_fit::{self, FitKind};
use camcraft_app::tools::{self, Tool};
use camcraft_app::vise::{ViseSize, size as vise_size};
use flecs_ecs::prelude::World;

pub(super) fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

/// The demo projects' output directory: `CAMCRAFT_GUIDE_PROJECTS_OUT` when
/// set (the `cargo xtask docs-images` and catalog-media runs), else a
/// directory of this test process only (#2543). A fixed name under the
/// shared temp dir is owned by whichever account made it first; on a
/// runner where another account made it, the save fails with
/// "Permission denied", and two concurrent runs write the same files.
pub(super) fn projects_dir() -> PathBuf {
    std::env::var_os("CAMCRAFT_GUIDE_PROJECTS_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(own_projects_dir)
}

fn own_projects_dir() -> PathBuf {
    std::env::temp_dir().join(format!("camcraft-guide-projects-{}", std::process::id()))
}

fn run<C: Command + 'static>(world: &World, h: &mut History, cmd: Result<C, camcraft_app::commands::CommandError>, label: &str) {
    let cmd = cmd.unwrap_or_else(|e| panic!("{label}: {e}"));
    h.execute(world, cmd);
}

fn add_catalog_tools(world: &World, h: &mut History, names: &[&str]) {
    for &name in names {
        let t: Tool = tools::catalog()
            .into_iter()
            .find(|(n, _)| *n == name)
            .map(|(_, t)| t)
            .unwrap_or_else(|| panic!("missing catalog tool {name}"));
        run(world, h, AddTool::new(world, name, t), name);
    }
}

fn init_setup(world: &World, h: &mut History, setup: &str, machine: &str, model: &str, mesh: Mesh) {
    run(world, h, CreateSetup::new(world, Some(setup), None), setup);
    run(world, h, LoadMachine::builtin(world, machine), machine);
    h.execute(world, ImportModel::from_mesh(model, mesh));
}

fn set_wcs(world: &World, h: &mut History, p: [f64; 3], label: &str) {
    let loc = Location { pos: Point3::new(p[0], p[1], p[2]), rot: IDENTITY_Q };
    run(world, h, SetWcsOrigin::new(world, loc), label);
}

fn set_box_stock(world: &World, h: &mut History, size: [f64; 3], p: [f64; 3], label: &str) {
    let shape = StockShape::Box { dx: size[0], dy: size[1], dz: size[2] };
    let loc = Location { pos: Point3::new(p[0], p[1], p[2]), rot: IDENTITY_Q };
    run(world, h, SetStock::new(world, shape, loc), label);
}

fn create_and_gen(world: &World, h: &mut History, name: &str, tool: &str, src: SrcRef, params: OpParams) {
    run(world, h, CreateOperation::new(world, name, tool, src, params), name);
    run(world, h, GenerateOperation::new(world, name), name);
    let entity = model::find_by_name(world, name).unwrap_or_else(|| panic!("missing op {name}"));
    let moves = model::toolpath_of(world, &entity).map_or(0, |tp| tp.path.moves.len());
    assert!(moves > 0, "expected generated toolpath for {name}, got 0 moves");
}

/// Closed spherical dome cap + flat bottom disc for 3D roughing and finishing.
fn dome_mesh(radius: f64, polar_max: f64) -> Mesh {
    const TAU: f64 = std::f64::consts::TAU;
    let (n_lat, n_lon) = (7usize, 28usize);
    let mut facets = Vec::new();
    let ring = |k: usize| -> Vec<Point3> {
        let ph = polar_max * k as f64 / n_lat as f64;
        let (z, rr) = (radius * ph.cos(), radius * ph.sin());
        (0..n_lon)
            .map(|j| {
                let th = TAU * j as f64 / n_lon as f64;
                Point3::new(rr * th.cos(), rr * th.sin(), z)
            })
            .collect()
    };
    let apex = Point3::new(0.0, 0.0, radius);
    let first = ring(1);
    for j in 0..n_lon {
        facets.push(Triangle::new(apex, first[j], first[(j + 1) % n_lon]));
    }
    for k in 1..n_lat {
        let ra = ring(k);
        let rb = ring(k + 1);
        for j in 0..n_lon {
            let j2 = (j + 1) % n_lon;
            facets.push(Triangle::new(ra[j], rb[j], rb[j2]));
            facets.push(Triangle::new(ra[j], rb[j2], ra[j2]));
        }
    }
    let rim = ring(n_lat);
    let bottom = Point3::new(0.0, 0.0, radius * polar_max.cos());
    for j in 0..n_lon {
        facets.push(Triangle::new(bottom, rim[(j + 1) % n_lon], rim[j]));
    }
    Mesh::from_facets(facets)
}

/// Closed horizontal cylinder segment aligned on X for multi-axis swarf cuts.
fn blade_mesh(radius: f64, x_len: f64) -> Mesh {
    const TAU: f64 = std::f64::consts::TAU;
    let (n_circ, n_x) = (18usize, 6usize);
    let v = |j: usize, i: usize| {
        let th = TAU * j as f64 / n_circ as f64;
        Point3::new(
            -x_len + 2.0 * x_len * i as f64 / n_x as f64,
            radius * th.cos(),
            radius * th.sin(),
        )
    };
    let mut facets = Vec::new();
    for j in 0..n_circ {
        for i in 0..n_x {
            let (a, b, c, d) = (v(j, i), v(j + 1, i), v(j + 1, i + 1), v(j, i + 1));
            facets.push(Triangle::new(a, b, c));
            facets.push(Triangle::new(a, c, d));
        }
    }
    for j in 0..n_circ {
        facets.push(Triangle::new(Point3::new(x_len, 0.0, 0.0), v(j, n_x), v(j + 1, n_x)));
        facets.push(Triangle::new(Point3::new(-x_len, 0.0, 0.0), v(j + 1, 0), v(j, 0)));
    }
    Mesh::from_facets(facets)
}

fn fit_stock(world: &World, h: &mut History, sides: [f64; 6]) {
    let (stock_cmd, _) = stock_fit::fit_command(world, FitKind::Sides(sides)).expect("fit stock");
    h.execute(world, stock_cmd);
}

pub(super) fn build_workholding_world() -> World {
    let world = World::new();
    let mut h = History::default();
    let part = cube(Point3::new(-45.0, -30.0, 0.0), Point3::new(45.0, 30.0, 22.0));
    init_setup(&world, &mut h, "Setup1", "VMC 3-axis", "Workpiece", part);
    fit_stock(&world, &mut h, [4.0, 4.0, 4.0, 4.0, 4.0, 2.0]);
    set_wcs(&world, &mut h, [-45.0, 30.0, 22.0], "wcs");

    let vise_params = vise_size("6 in").map(ViseSize::half_open).expect("6 in vise");
    let vise_cmd = InsertVise::new(&world, &vise_params, Some("Setup1"), VisePlacement::FitStock)
        .expect("insert vise");
    let vise = vise_cmd.name().to_string();
    h.execute(&world, vise_cmd);
    let parallels = Accessory::default_of("parallels").expect("parallels");
    let placement = AccessoryPlacement::UnderStockInVise { vise, above: 12.0 };
    run(&world, &mut h, InsertAccessory::new(&world, &parallels, Some("Setup1"), placement), "parallels");

    add_catalog_tools(&world, &mut h, &["EM-1000-4FL", "EM-0600-4FL"]);
    let top_clr = Height::of(HeightReference::TopOfStock, 12.0);
    create_and_gen(&world, &mut h, "Face1", "EM-1000-4FL", SrcRef::Stock, OpParams::Face {
        holes: Default::default(), blend_turns: false, tilt: None, stepover: 6.0,
        clearance_z: top_clr, cut_z: Height::of(HeightReference::TopOfModel, 0.0),
        top_z: Some(Height::of(HeightReference::TopOfStock, 0.0)), stepdown: Some(1.0),
        feed: 1400.0, plunge_feed: 350.0, climb: true,
    });
    create_and_gen(&world, &mut h, "2D Contour1", "EM-0600-4FL", SrcRef::Entity("Workpiece".into()), OpParams::Contour {
        stock_to_leave: 0.0, clearance_z: top_clr,
        retract_z: Height::of(HeightReference::TopOfStock, 4.0),
        top_z: Height::of(HeightReference::TopOfModel, 0.0),
        bottom_z: Height::of(HeightReference::TopOfModel, -4.0),
        stepdown: 2.0, feed: 1200.0, plunge_feed: 300.0, climb: true, side: Side::Outside,
        lead: Lead::Arc { radius: 4.0 }, tolerance: 0.01, tab_count: 0, tab_width: 8.0,
        tab_height: 3.0, entry: Entry3D::Plunge, helix_radius: 1.0, blend_turns: false,
        links: Links::PLANAR, tilt: None,
    });
    world
}

pub(super) fn build_3d_world() -> World {
    let world = World::new();
    let mut h = History::default();
    let mold = dome_mesh(18.0, std::f64::consts::FRAC_PI_3);
    init_setup(&world, &mut h, "Setup1", "VMC 3-axis", "Mold", mold);
    fit_stock(&world, &mut h, [3.0, 3.0, 3.0, 3.0, 0.0, 1.5]);
    set_wcs(&world, &mut h, [0.0, 0.0, 19.5], "wcs");

    add_catalog_tools(&world, &mut h, &["EM-0600-4FL", "BN-0600"]);
    let top_clr = Height::of(HeightReference::TopOfStock, 10.0);
    let ret_z = Height::of(HeightReference::TopOfStock, 4.0);
    create_and_gen(&world, &mut h, "3D Pocket1", "EM-0600-4FL", SrcRef::Entity("Mold".into()), OpParams::Pocket3D {
        stepover: 2.5, stock_to_leave: 0.3, clearance_z: top_clr, retract_z: Some(ret_z),
        top_z: Height::of(HeightReference::TopOfStock, 0.0),
        bottom_z: Height::of(HeightReference::BottomOfModel, 0.0),
        stepdown: 2.0, feed: 1200.0, plunge_feed: 300.0, climb: true,
        strategy: PocketStrategy::Waterline, entry: Entry3D::Helix, helix_radius: 1.5,
        links: Links::ROUGH3D, rest_rough: false, optimal_load: 0.25, feed_opt: false, shell: false,
    });
    create_and_gen(&world, &mut h, "3D Finish1", "BN-0600", SrcRef::Entity("Mold".into()), OpParams::Finish3D {
        pattern: FinishPattern::Parallel, stepover: Some(2.0), tolerance: 0.08,
        clearance_z: top_clr, retract_z: Some(ret_z),
        top_z: Height::of(HeightReference::TopOfModel, 0.0),
        bottom_z: Height::of(HeightReference::BottomOfModel, 0.0),
        feed: 900.0, plunge_feed: 250.0, climb: true, ramp: true, min_step: None,
        ramp_angle: 45.0, entry_ramp: false, steep_stepover: None,
        pencil_reference: None, fill_openings: None,
    });
    world
}

pub(super) fn build_multiaxis_world() -> World {
    let world = World::new();
    let mut h = History::default();
    let blade = blade_mesh(12.0, 24.0);
    init_setup(&world, &mut h, "Setup1", "Tilt-head 5-axis", "Blade", blade);
    set_box_stock(&world, &mut h, [52.0, 28.0, 28.0], [0.0, 0.0, -14.0], "stock1");
    set_wcs(&world, &mut h, [0.0, 0.0, 12.0], "wcs1");

    add_catalog_tools(&world, &mut h, &["BN-0600", "EM-0600-4FL"]);
    create_and_gen(&world, &mut h, "Swarf1", "BN-0600", SrcRef::Entity("Blade".into()), OpParams::Swarf {
        tolerance: 0.1, stepover: Some(2.0), clearance_z: Height::fixed(24.0), retract_z: None,
        top_z: Height::fixed(12.0), bottom_z: Height::fixed(3.0), feed: 500.0, plunge_feed: 120.0,
        climb: true, lead_deg: 15.0, lag_deg: 0.0, max_tilt_deg: 60.0,
    });

    run(&world, &mut h, CreateSetup::new(&world, Some("Setup2"), None), "setup2");
    let block = cube(Point3::new(75.0, -18.0, 0.0), Point3::new(125.0, 18.0, 20.0));
    h.execute(&world, ImportModel::from_mesh("IndexedBlock", block));
    set_box_stock(&world, &mut h, [54.0, 40.0, 22.0], [100.0, 0.0, 0.0], "stock2");
    set_wcs(&world, &mut h, [100.0, 0.0, 20.0], "wcs2");
    create_and_gen(&world, &mut h, "Indexed Face1", "EM-0600-4FL", SrcRef::Stock, OpParams::Face {
        holes: Default::default(), blend_turns: false,
        // Preserve the persisted legacy basis of this documented unsafe example.
        tilt: Some(Tilt { basis_version: 0, a_deg: 0.0, b_deg: 20.0, origin: [100.0, 0.0, 20.0] }),
        stepover: 3.5, clearance_z: Height::fixed(30.0), cut_z: Height::fixed(18.0),
        top_z: Some(Height::fixed(21.0)), stepdown: Some(1.5),
        feed: 900.0, plunge_feed: 200.0, climb: true,
    });
    world
}

pub(super) fn write_all_demo_projects(dir: &Path) {
    type ProjectBuilder = fn() -> World;
    std::fs::create_dir_all(dir).expect("create demo project dir");
    let specs: [(&str, ProjectBuilder); 3] = [
        ("guide-workholding.camproj", build_workholding_world),
        ("guide-3d.camproj", build_3d_world),
        ("guide-multiaxis.camproj", build_multiaxis_world),
    ];
    for (file, build) in specs {
        let world = build();
        let path = dir.join(file);
        project::save(&world, &path).unwrap_or_else(|e| panic!("save {}: {e}", path.display()));
    }
}

#[test]
fn demo_projects_build_and_load_with_valid_paths() {
    let dir = projects_dir();
    write_all_demo_projects(&dir);
    for file in ["guide-workholding.camproj", "guide-3d.camproj", "guide-multiaxis.camproj"] {
        let path = dir.join(file);
        assert!(path.is_file(), "missing saved project {}", path.display());
        let world = World::new();
        let notes = project::load_noted(&world, &path).unwrap_or_else(|e| panic!("load {file}: {e}"));
        assert!(notes.is_empty(), "{file} had reload notes: {notes:?}");
    }
    // A directory of this process only: remove it, so that runs leave no files behind.
    if dir == own_projects_dir() {
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn default_projects_dir_belongs_to_this_process() {
    // #2543: a fixed shared temp path failed on a runner where another account owned it.
    let dir = own_projects_dir();
    assert!(dir.starts_with(std::env::temp_dir()));
    let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    assert_eq!(name, format!("camcraft-guide-projects-{}", std::process::id()));
    if std::env::var_os("CAMCRAFT_GUIDE_PROJECTS_OUT").is_none() {
        assert_eq!(projects_dir(), dir);
    }
}

#[test]
fn indexed_guide_preserves_legacy_basis_and_path() {
    let world = build_multiaxis_world();
    let entity = model::find_by_name(&world, "Indexed Face1").unwrap();
    let op = model::operation_of(&world, &entity).unwrap();
    let OpParams::Face { tilt: Some(tilt), .. } = op.params else { panic!("indexed face"); };
    assert_eq!(tilt.basis_version, 0);
    let saved = World::new();
    project::load_noted(&saved, &workspace_root().join("examples/guide-multiaxis.camproj")).unwrap();
    let saved_entity = model::find_by_name(&saved, "Indexed Face1").unwrap();
    let actual = model::toolpath_of(&world, &entity).unwrap();
    let expected = model::toolpath_of(&saved, &saved_entity).unwrap();
    assert_eq!(actual.path.moves.len(), expected.path.moves.len());
    for (actual, expected) in actual.path.moves.iter().zip(&expected.path.moves) {
        assert_eq!(actual.kind, expected.kind);
        assert_eq!(actual.feed, expected.feed);
        assert_eq!(actual.arc, expected.arc);
        assert_eq!(actual.rotary, expected.rotary);
        for (actual, expected) in [actual.from.x, actual.from.y, actual.from.z,
            actual.to.x, actual.to.y, actual.to.z].into_iter().zip([
            expected.from.x, expected.from.y, expected.from.z,
            expected.to.x, expected.to.y, expected.to.z]) {
            assert!((actual - expected).abs() <= 0.000_001, "saved coordinates round to six decimals");
        }
        let actual_axis = actual.axis.unwrap();
        let expected_axis = expected.axis.unwrap();
        for (actual, expected) in actual_axis.into_iter().zip(expected_axis) {
            assert!((actual - expected).abs() <= 0.000_001);
        }
    }
    for warning in &expected.warnings {
        assert!(actual.warnings.contains(warning), "documented warning lost: {warning:?}");
    }
}
