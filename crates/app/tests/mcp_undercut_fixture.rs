//! #1444: a lollipop's whole ball against the workholding steel. The
//! neck leaves the ball below its pole, so the ball's upper hemisphere
//! stands wider than the neck above the neck start. The post gate's
//! fixture law must sweep that upper cap: a fixture that only the cap
//! reaches holds the post, and the same job posts with the fixture
//! moved clear.
//! Provenance: our own fixture - the T-slot block of `undercut_support`
//! and a box fixture, written as STL by this test.
//!
//! Gate map: covers app::fixture_struck (the undercut fixture envelope),
//! the steel stack of app::operation::generate and
//! camcraft_tools::undercut::fixture_geom through the post gate.

mod undercut_support;

use serde_json::{json, Value};
use std::path::Path;
use undercut_support::{tmp_dir, write_box_stl, write_tslot_stl, Client};

/// The T-slot block, a 6 mm end mill for the neck slot, and `LP-6`: a
/// 6 mm ball on a 3 mm neck that leaves the ball 4 mm above the tip.
fn job(dir: &Path) -> Client {
    let stl = dir.join("tslot.stl");
    write_tslot_stl(&stl);
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": stl.display().to_string() }));
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 40.0, "dz": 20.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0600-4FL" }));
    let job = c.ok("get_job", json!({}));
    let mut def = job["tools"][0]["definition"].clone();
    def["kind"] = json!("lollipop");
    def["diameter"] = json!(8.0);
    def["flute_len"] = json!(6.0);
    def["shank_d"] = json!(3.0);
    def["shank_len"] = json!(30.0);
    def["rpm"] = json!(1500.0);
    def["feed"] = json!(120.0);
    if let Some(o) = def.as_object_mut() {
        o.remove("cutting_data");
    }
    c.ok("add_tool", json!({ "name": "LP-6", "definition": def, "holder": "none" }));
    c.ok("add_operation", json!({
        "kind": "Slot", "tool": "EM-0600-4FL", "source": "tslot", "name": "Neck",
        "params": {
            "a": [-30.0, 0.0], "b": [30.0, 0.0], "width": 9.0,
            "clearance_z": 30.0, "top_z": 20.0, "bottom_z": 14.0, "stepdown": 3.0, "stepover": 2.0
        }
    }));
    c.ok("add_operation", json!({
        "kind": "Slot Undercut", "tool": "LP-6", "source": "tslot", "name": "Undercut",
        "params": {
            "a": [-30.0, 0.0], "b": [30.0, 0.0], "width": 10.0, "run_in": 15.0, "neck_slot": 9.0,
            "clearance_z": 30.0, "retract_z": 25.0, "top_z": 20.0, "bottom_z": 8.0, "stepover": 2.0,
            "feed": 120.0, "plunge_feed": 60.0
        }
    }));
    c
}

fn post(c: &mut Client, dir: &Path, name: &str) -> (bool, Value) {
    c.call("post", json!({ "output": dir.join(name).display().to_string(), "post": "Fanuc" }))
}

/// The steel: a small block beside the run-in (X -45 to -30, tip at
/// Z 8, Y 0). The ball's centre is at Z 12 and the neck (radius 1.5)
/// starts at Z 14. At Z 14.3 the ball's radius is 3.27, so the block
/// 1.8 mm off the path meets the upper cap and nothing else: the head
/// below the neck start ends at Z 14 and the neck is 0.3 mm short.
fn add_steel(c: &mut Client, dir: &Path, name: &str, lift: f64) {
    let stl = dir.join(format!("{name}.stl"));
    write_box_stl(&stl, [-40.0, 1.8, 14.3 + lift], [-38.0, 2.4, 14.8 + lift]);
    c.ok("import_model", json!({ "path": stl.display().to_string(), "name": name }));
    c.ok("add_fixture", json!({ "model": name }));
}

fn refused_by_the_cap(c: &mut Client, dir: &Path, name: &str) {
    let (refused, msg) = post(c, dir, name);
    let msg = msg.as_str().unwrap_or_default().to_owned();
    assert!(refused, "the cap strikes the steel, yet the post went out: {msg}");
    assert!(msg.contains("Undercut") && msg.contains("strikes a fixture"), "{msg}");
    assert!(!dir.join(name).exists(), "nothing is posted");
}

/// Generation judges the steel with the whole ball: the op carries the
/// hit, and the post holds.
#[test]
fn a_fixture_only_the_balls_upper_cap_reaches_holds_the_post() {
    let dir = tmp_dir("lp-cap");
    let mut c = job(&dir);
    add_steel(&mut c, &dir, "steel", 0.0);
    c.ok("generate", json!({}));
    let ver = c.ok("verify", json!({}));
    let warned = ver["operations"].as_array().and_then(|o| o.iter().find(|o| o["key"] == json!("Undercut"))).map(|o| o["warnings"].to_string());
    assert!(warned.is_some_and(|w| w.contains("strikes a fixture")), "generation saw the cap hit: {ver}");
    refused_by_the_cap(&mut c, &dir, "cap.nc");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The live post gate judges the cached path against steel added after
/// generation, with the whole ball.
#[test]
fn steel_added_after_generation_is_judged_with_the_whole_ball_at_the_post() {
    let dir = tmp_dir("lp-live");
    let mut c = job(&dir);
    c.ok("generate", json!({}));
    add_steel(&mut c, &dir, "steel", 0.0);
    refused_by_the_cap(&mut c, &dir, "live.nc");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_same_block_over_the_balls_pole_posts() {
    // The block 2 mm higher stands over the ball's pole (Z 16), beside
    // the neck: real air, and the job posts. The envelope stops at the
    // pole and keeps the real neck.
    let dir = tmp_dir("lp-pole");
    let mut c = job(&dir);
    add_steel(&mut c, &dir, "high", 2.0);
    c.ok("generate", json!({}));
    let (refused, body) = post(&mut c, &dir, "clear.nc");
    assert!(!refused, "the block over the pole is clear: {body}");
    let _ = std::fs::remove_dir_all(&dir);
}
