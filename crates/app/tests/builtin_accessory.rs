//! Built-in workholding accessories (#789): every kind builds closed,
//! outward shells of its stated dimensions that the mesh law takes;
//! nonsense parameters are refused by name; placing and resizing are one
//! undo unit each; the parameters survive a save and load, and a file
//! whose accessory key was edited into nonsense keeps plain steel.
//!
//! Gate map (M4.4): covers app::accessory, app::commands::accessory,
//! app::commands::insert_fixture, app::project (fixture accessory key),
//! app::fixture, sim::fixture.

use std::collections::HashMap;

use camcraft_app::accessory::{self, Accessory, mesh};
use camcraft_app::commands::{
    AccessoryPlacement, History, ImportModel, InsertAccessory, SetAccessory, TagFixture,
};
use camcraft_app::fixture;
use camcraft_app::geometry::{IDENTITY_Q, Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::world::new_world;
use camcraft_sim::fixture_mesh::FixtureMesh;

const IN: f64 = 25.4;

fn bounds(tris: &[Triangle]) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for p in tris.iter().flat_map(|t| [t.a, t.b, t.c]) {
        for (k, v) in [p.x, p.y, p.z].into_iter().enumerate() {
            lo[k] = lo[k].min(v);
            hi[k] = hi[k].max(v);
        }
    }
    (lo, hi)
}

fn key(p: Point3) -> [u64; 3] {
    [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]
}

/// Closed and consistently oriented, the mesh law's 2-cycle: every
/// directed edge is met by its reverse exactly as often (touching
/// shells, like a heel block flush under the strap's end, share an edge
/// once per shell).
fn assert_watertight(tris: &[Triangle], what: &str) {
    let mut edges: HashMap<([u64; 3], [u64; 3]), i32> = HashMap::new();
    for t in tris {
        for (a, b) in [(t.a, t.b), (t.b, t.c), (t.c, t.a)] {
            *edges.entry((key(a), key(b))).or_default() += 1;
        }
    }
    for ((a, b), n) in &edges {
        assert_eq!(edges.get(&(*b, *a)), Some(n), "{what}: an edge with no matching reverse (open shell)");
    }
}

fn volume(tris: &[Triangle]) -> f64 {
    tris.iter()
        .map(|t| {
            let (a, b, c) = (t.a, t.b, t.c);
            a.x * (b.y * c.z - b.z * c.y) - a.y * (b.x * c.z - b.z * c.x) + a.z * (b.x * c.y - b.y * c.x)
        })
        .sum::<f64>()
        / 6.0
}

/// The polygon drawn around a stud: its area.
fn stud_area(d: f64) -> f64 {
    let n = mesh::STUD_SIDES as f64;
    let r = mesh::stud_radius(d);
    0.5 * n * r * r * (std::f64::consts::TAU / n).sin()
}

/// The solid's volume by its parts, and its bounds `(lo, hi)`.
fn expected(a: &Accessory) -> (f64, [f64; 3], [f64; 3]) {
    match *a {
        Accessory::Parallels { height, thickness, length, spacing } => {
            let y = 0.5 * spacing + thickness;
            (2.0 * height * thickness * length, [-0.5 * length, -y, 0.0], [0.5 * length, y, height])
        }
        Accessory::Block { length, width, height } => {
            (length * width * height, [-0.5 * length, -0.5 * width, 0.0], [0.5 * length, 0.5 * width, height])
        }
        Accessory::SoftJaw { width, height, thickness } => {
            (width * height * thickness, [-0.5 * width, 0.0, 0.0], [0.5 * width, thickness, height])
        }
        Accessory::StepBlock { steps, rise, run, width } => {
            let n = f64::from(steps);
            (width * run * rise * n * (n + 1.0) / 2.0, [-0.5 * width, 0.0, 0.0], [0.5 * width, n * run, n * rise])
        }
        Accessory::StrapClamp {
            strap_length: l,
            strap_width: w,
            strap_thickness: t,
            stud_diameter: d,
            stud_height: s,
            heel_length: k,
            clamp_height: h,
            ..
        } => {
            let v = l * w * t + k * w * h + stud_area(d) * (h + s);
            let hw = (0.5 * w).max(mesh::stud_radius(d));
            (v, [0.0, -hw, 0.0], [l, hw, h + t + s])
        }
    }
}

fn check_solid(a: &Accessory, what: &str) {
    let tris = a.mesh().unwrap_or_else(|e| panic!("{what}: {e}"));
    assert_watertight(&tris, what);
    let (want, lo_w, hi_w) = expected(a);
    let v = volume(&tris);
    assert!((v - want).abs() < 1e-9 * want.max(1.0), "{what}: outward volume {v} vs {want}");
    let (lo, hi) = bounds(&tris);
    for k in 0..3 {
        assert!((lo[k] - lo_w[k]).abs() < 1e-9 && (hi[k] - hi_w[k]).abs() < 1e-9, "{what}: bounds {lo:?}..{hi:?}");
    }
    assert!(FixtureMesh::build(&tris).is_ok(), "{what}: the mesh law takes it");
}

#[test]
fn every_kind_builds_closed_shells_of_its_dimensions() {
    assert_eq!(accessory::KINDS, ["parallels", "block", "step_block", "strap_clamp", "soft_jaw"]);
    for kind in accessory::KINDS {
        let a = Accessory::default_of(kind).expect(kind);
        assert_eq!(a.kind(), kind);
        check_solid(&a, kind);
    }
    // The 1-2-3 block is 1 x 2 x 3 in, its holes drawn solid.
    let block = Accessory::default_of("block").expect("block");
    let (lo, hi) = bounds(&block.mesh().expect("mesh"));
    let mut sides = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    sides.sort_by(f64::total_cmp);
    for (s, want) in sides.iter().zip([IN, 2.0 * IN, 3.0 * IN]) {
        assert!((s - want).abs() < 1e-9, "{sides:?}");
    }
    for steps in 1..=8 {
        let mut a = Accessory::default_of("step_block").expect("steps");
        *a.steps_mut().expect("a step block") = steps;
        check_solid(&a, &format!("{steps} steps"));
    }
    // #812: a stud wider than the strap is refused, never drawn narrower
    // than typed (fixture_keys_lenient pins the refusal by name).
    let wide = Accessory::default_of("strap_clamp").expect("clamp").with_dim("stud_diameter", 40.0).expect("dim");
    assert!(wide.mesh().is_err_and(|e| e.contains("must be less than the strap width")));
}

#[test]
fn nonsense_parameters_are_refused_by_name() {
    let refused = |a: Accessory, word: &str| {
        let e = a.mesh().expect_err(word);
        assert!(e.contains(word), "'{e}' names {word}");
    };
    let par = Accessory::default_of("parallels").expect("parallels");
    refused(par.clone().with_dim("height", 0.0).expect("dim"), "Parallels height must be more than zero");
    refused(par.clone().with_dim("spacing", -1.0).expect("dim"), "spacing");
    refused(par.clone().with_dim("thickness", f64::NAN).expect("dim"), "thickness must be a finite length");
    refused(par.clone().with_dim("length", 1e9).expect("dim"), "is over");
    let e = par.with_dim("stud_height", 3.0).err().unwrap_or_default();
    assert!(e.contains("no dimension \"stud_height\""), "{e}");
    let mut steps = Accessory::default_of("step_block").expect("steps");
    *steps.steps_mut().expect("steps") = 0;
    refused(steps.clone(), "step count");
    *steps.steps_mut().expect("steps") = 500;
    refused(steps, "step count");
    let clamp = Accessory::default_of("strap_clamp").expect("clamp");
    refused(clamp.clone().with_dim("stud_offset", 2.0).expect("dim"), "past the toe");
    refused(clamp.clone().with_dim("stud_offset", 140.0).expect("dim"), "into the heel block");
    refused(clamp.clone().with_dim("heel_length", 200.0).expect("dim"), "heel block length");
    refused(Accessory::default_of("soft_jaw").expect("jaw").with_dim("width", f64::INFINITY).expect("dim"), "Soft jaw width");
    let world = new_world();
    let bad = clamp.with_dim("clamp_height", -5.0).expect("dim");
    let at = AccessoryPlacement::At(Location::default());
    assert!(InsertAccessory::new(&world, &bad, None, at.clone()).is_err(), "the command refuses too");
    let nan = AccessoryPlacement::At(Location { pos: Point3::new(f64::NAN, 0.0, 0.0), rot: IDENTITY_Q });
    let good = Accessory::default_of("block").expect("block");
    assert!(InsertAccessory::new(&world, &good, None, nan).is_err(), "a NaN pose is refused");
    let fit = AccessoryPlacement::UnderStockInVise { vise: "Vise 6 in".into(), above: 5.0 };
    let e = InsertAccessory::new(&world, &good, None, fit).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(e.contains("only parallels"), "{e}");
}

fn params_of(world: &flecs_ecs::prelude::World, name: &str) -> Option<Accessory> {
    model::find_by_name(world, name).and_then(|e| fixture::def_of(world, &e)).and_then(|d| d.accessory)
}

#[test]
fn placing_and_resizing_are_one_undo_each_and_survive_a_save() {
    let world = new_world();
    let mut history = History::default();
    let mut names = Vec::new();
    for (k, kind) in accessory::KINDS.iter().enumerate() {
        let a = Accessory::default_of(kind).expect(kind);
        let at = Location { pos: Point3::new(200.0 * k as f64, 0.0, 0.0), rot: IDENTITY_Q };
        let cmd = InsertAccessory::new(&world, &a, None, AccessoryPlacement::At(at)).expect(kind);
        names.push(cmd.name().to_owned());
        history.execute(&world, cmd);
    }
    assert_eq!(names, ["Parallels", "1-2-3 block", "Step block", "Strap clamp", "Soft jaw"]);
    let clamp = "Strap clamp";
    let e = model::find_by_name(&world, clamp).expect("placed");
    let rev = model::rev_of(&world, &e);
    let before = model::facets_of(&world, &e).expect("facets").to_vec();
    let taller = params_of(&world, clamp).expect("clamp").with_dim("clamp_height", 50.0).expect("dim");
    history.execute(&world, SetAccessory::new(&world, clamp, taller.clone()).expect("resize"));
    assert_eq!(params_of(&world, clamp), Some(taller.clone()));
    assert!(model::rev_of(&world, &e) > rev, "the geometry revision moves");
    let b = model::entity_world_aabb(&world, &e).expect("box");
    assert!((b.max.z - (50.0 + 0.625 * IN + 1.5 * IN)).abs() < 1e-9, "the stud top rose with the clamp: {}", b.max.z);

    let path = std::env::temp_dir().join(format!("camcraft-accessory-{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains("\"accessory\"") && text.contains("\"strap_clamp\""), "the marker carries the parameters");
    let back = new_world();
    project::load(&back, &path).expect("load");
    for n in &names {
        assert_eq!(params_of(&back, n), params_of(&world, n), "{n} reloads");
    }
    assert!(SetAccessory::new(&back, clamp, taller.clone().with_dim("clamp_height", 30.0).expect("dim")).is_ok(), "and stays editable");

    // A hand-edited nonsense key reads as plain steel, not a crash.
    let broken = text.replace("\"clamp_height\": 50.0", "\"clamp_height\": -1.0");
    assert_ne!(broken, text, "the key is in the file");
    std::fs::write(&path, broken).expect("write");
    let plain = new_world();
    project::load(&plain, &path).expect("loads");
    let _ = std::fs::remove_file(&path);
    let pe = model::find_by_name(&plain, clamp).expect("the clamp's steel stays");
    assert!(fixture::def_of(&plain, &pe).is_some_and(|d| d.accessory.is_none()), "kept as plain steel");

    // One undo takes the resize back, facets and all; one more the clamp.
    assert!(history.undo(&world));
    assert_eq!(params_of(&world, clamp).map(|a| a.dims()), Accessory::default_of("strap_clamp").map(|a| a.dims()));
    assert_eq!(model::rev_of(&world, &e), rev);
    assert_eq!(model::facets_of(&world, &e).expect("facets").to_vec(), before, "the steel comes back");
    assert!(history.undo(&world));
    assert!(model::find_by_name(&world, "Soft jaw").is_none(), "one undo removes the last placement");
    assert!(model::find_by_name(&world, clamp).is_some());
    assert!(history.redo(&world));
    assert!(history.redo(&world));
    assert_eq!(params_of(&world, clamp), Some(taller), "redo lands both again");

    // A resize keeps its kind; an imported fixture has no parameters.
    let e = SetAccessory::new(&world, clamp, Accessory::default_of("block").expect("block")).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(e.contains("cannot become a 1-2-3 block"), "{e}");
    history.execute(&world, ImportModel::from_mesh("Plate", Mesh::from_facets(mesh::cuboid(Point3::ZERO, Point3::new(1.0, 1.0, 1.0)))));
    history.execute(&world, TagFixture::new(&world, "Plate", None).expect("tag"));
    let e = SetAccessory::new(&world, "Plate", Accessory::default_of("block").expect("block")).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(e.contains("not a built-in accessory"), "{e}");
}
