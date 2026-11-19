//! The 0.8.0 hand test on 2D Adaptive over the user's "Milling-test"
//! (testdata STEP: a 60 x 50 block, Z -5..20, notched sides, a round
//! recess r 12.5 down to Z 0 in its top). The face picked at Z -5
//! (~2500 mm^2) is the block's underside: its floor is its own Z, and
//! Containment Outside clears the band the tool reaches past that
//! outline - never the recess the band does not touch, and never a hole
//! of the selection.
//!
//! Gate map: covers app::operation (heights_sel, boundary, selection,
//! generate), sim::verify.
//! Provenance: our own regression of the user's report; no vendor
//! material consulted.

mod area_support;
mod common;
mod pocket_support;

use area_support::*;
use camcraft_app::geo2d::Point2;
use camcraft_app::operation::HeightReference;

fn milling_test() -> (World, String) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    let world = world_with(Some(ImportModel::new(&path).expect("the user's STEP imports")), -7.0, 27.0);
    let name = model::named_entities(&world)
        .into_iter()
        .map(|(n, _)| n)
        .find(|n| n != "Stock" && n != TOOL)
        .expect("the imported part");
    (world, name)
}

/// The recess in the part's top: (centre, radius) of the top face's hole.
fn recess(world: &World, name: &str) -> ((f64, f64), f64) {
    let FeatureSel::Face(id) = top_face(world, name, 20.0) else { panic!("a face") };
    let e = model::find_by_name(world, name).expect("model");
    let f = model::features_of(world, &e).expect("features");
    let ff = f.faces.iter().find(|x| x.id == id).expect("the face");
    let l = ff.loops.iter().find(|l| l.kind != feature::LoopKind::Outer).expect("the recess is the top's hole");
    let pts: Vec<[f64; 3]> = l.verts.iter().filter_map(|&v| f.verts.get(v as usize).copied()).collect();
    let n = pts.len() as f64;
    let c = (pts.iter().map(|p| p[0]).sum::<f64>() / n, pts.iter().map(|p| p[1]).sum::<f64>() / n);
    let r = pts.iter().map(|p| (p[0] - c.0).hypot(p[1] - c.1)).fold(f64::INFINITY, f64::min);
    (c, r)
}

fn adaptive(bottom: Height) -> OpParams {
    let mut p = operation::retype_params(&pocket(20.0, -5.0), OpType::Adaptive2D.ordinal());
    if let OpParams::Adaptive2D { top_z, bottom_z, stepdown, .. } = &mut p {
        *top_z = Height::fixed(20.0);
        *bottom_z = bottom;
        *stepdown = 25.0; // one level, on the face: the debug lane stays fast
    }
    assert_eq!(p.kind(), OpType::Adaptive2D);
    p
}

const FLOOR: Height = Height { reference: HeightReference::BottomOfSelection, offset: 0.0 };

#[test]
fn a_face_selection_floor_is_the_faces_own_z() {
    let (world, name) = milling_test();
    let src = sel(&name, top_face(&world, &name, -5.0));
    let z = operation::resolve_height(&world, None, Some(&src), FLOOR).expect("resolves");
    assert!((z + 5.0).abs() < 1e-6, "the picked face's floor is Z -5, got {z}");
    let top = sel(&name, top_face(&world, &name, 20.0));
    let zt = operation::resolve_height(&world, None, Some(&top), FLOOR).expect("resolves");
    assert!((zt - 20.0).abs() < 1e-6, "the top face resolves to its own Z, got {zt}");
    let p = adaptive(FLOOR);
    assert_eq!(operation::selection_heights_sane(&world, None, &src, &p), None, "no height error");
    assert!(operation::depth_sane(&world, None, Some(&src), &p));
}

#[test]
#[ignore = "heavy lane (release): outside containment over Milling-test.step exceeds 60 s in debug (#2453)"]
fn containment_outside_clears_past_the_edge_never_the_recess() {
    let (world, name) = milling_test();
    let (c, recess_r) = recess(&world, &name);
    let src = sel(&name, top_face(&world, &name, -5.0));
    // #2232 Option 1: with a setup stock, Containment Outside clears
    // from the selected face out to the stock outline (+2r air band),
    // entering in the free area and ending at the selected face, never
    // entering the recess.
    let a = AreaControls { containment: Some(Containment::Outside), ..Default::default() };
    let preview = operation::region_of(&world, &op(src.clone(), adaptive(FLOOR), a.clone())).expect("the band previews");
    assert!(!preview.contains(Point2::new(c.0, c.1)), "the recess is not in the machined area");
    let t0 = std::time::Instant::now();
    let (t, warns) = run(&world, &op(src, adaptive(FLOOR), a)).expect("generates with stock");
    assert!(t0.elapsed().as_secs_f64() < 60.0, "the band plans in seconds, not minutes");
    assert_eq!(warns, 0, "the plan verifies clean: {:?}", t.notes);
    let part_outline = entity_outline(&world, &name);
    let first_feed = t.moves.iter().find(|m| m.kind == MoveKind::Feed).expect("first cut");
    assert!(
        min_dist_to_region(&part_outline, first_feed.from) > R + 0.5,
        "first cut enters in the free area away from the face: {:?}",
        first_feed.from
    );
    let last_feed = t.moves.iter().rfind(|m| m.kind == MoveKind::Feed).expect("last cut");
    assert!(
        min_dist_to_region(&part_outline, last_feed.to) <= R + 0.05,
        "last pass finishes at the selected face: {:?}",
        last_feed.to
    );
    let f: Vec<&camcraft_app::cl::Move> = t.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    assert!(!f.is_empty());
    let lo = f.iter().map(|m| m.to.z).fold(f64::INFINITY, f64::min);
    assert!((lo + 5.0).abs() < 1e-6, "the last level sits on the picked face, got {lo}");
    let mut worst = f64::INFINITY;
    for m in &f {
        // An arc move is read along its arc (its chords at 0.01 mm, well
        // inside the 0.05 this asserts to), never as the straight from
        // `from` to `to`: since #689 a closed ring fits as whole arcs,
        // whose end-to-end straight crosses the recess the arc rounds.
        for (a, b) in m.chords(0.01) {
            for i in 0..=8 {
                let s = f64::from(i) / 8.0;
                let x = a.x + (b.x - a.x) * s;
                let y = a.y + (b.y - a.y) * s;
                worst = worst.min((x - c.0).hypot(y - c.1));
            }
        }
    }
    assert!(
        worst >= recess_r + R - 0.05,
        "a feed enters the recess (centre {worst:.3} from its axis, recess r {recess_r:.3})"
    );
}

#[test]
fn containment_outside_without_stock_clears_with_extension_never_the_recess() {
    let world = new_world();
    let mut h = History::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step");
    let imp = ImportModel::new(&path).expect("the user's STEP imports");
    let name = imp.name.clone();
    h.execute(&world, imp);
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    let (c, recess_r) = recess(&world, &name);
    let src = sel(&name, top_face(&world, &name, -5.0));
    let a = AreaControls { containment: Some(Containment::Outside), extension: 4.0, ..Default::default() };
    let band = operation::region_of(&world, &op(src.clone(), adaptive(FLOOR), a.clone())).expect("the band previews");
    assert!(!band.contains(Point2::new(c.0, c.1)), "the recess stays out with an extension too");
    let t0 = std::time::Instant::now();
    let (t, warns) = run(&world, &op(src, adaptive(FLOOR), a)).expect("generates");
    assert!(t0.elapsed().as_secs_f64() < 60.0, "the band plans in seconds, not minutes");
    assert_eq!(warns, 0, "the plan verifies clean: {:?}", t.notes);
    assert!(
        t.notes.iter().any(|n| n.contains("channel") && n.contains("Offset rings cut")),
        "the refusal is named: {:?}",
        t.notes
    );
    let mut worst = f64::INFINITY;
    for m in t.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
        for (a, b) in m.chords(0.01) {
            for i in 0..=8 {
                let s = f64::from(i) / 8.0;
                worst = worst.min((a.x + (b.x - a.x) * s - c.0).hypot(a.y + (b.y - a.y) * s - c.1));
            }
        }
    }
    assert!(worst >= recess_r + R - 0.05, "a feed enters the recess ({worst:.3})");
}

#[test]
fn the_open_run_refusal_says_how_to_close_it() {
    let world = block_world();
    let rim = rim_edge(&world, |q| (q[1] - 20.0).abs() < 1e-9);
    let p = operation::retype_params(&pocket(10.0, 5.0), OpType::Adaptive2D.ordinal());
    let err = run(&world, &op(sel("part", rim), p, AreaControls::default())).expect_err("an open run refuses");
    assert!(
        err.contains("cannot end mid-edge")
            && err.ends_with("- Ctrl+click the remaining edges, or Shift+click to take the whole loop"),
        "{err}"
    );
}

fn horizontal_face(world: &World, name: &str, z: f64) -> FeatureSel {
    let e = model::find_by_name(world, name).expect("model");
    let f = model::features_of(world, &e).expect("features");
    let mut best: Option<(f64, FeatureSel)> = None;
    for face in f.faces.iter().filter(|face| face.normal[2].abs() > 0.99) {
        let Some(l) = face.loops.iter().find(|l| l.kind == feature::LoopKind::Outer) else { continue };
        let pts: Vec<[f64; 3]> = l.verts.iter().filter_map(|&v| f.verts.get(v as usize).copied()).collect();
        if pts.is_empty() || pts.iter().any(|p| (p[2] - z).abs() > 1e-6) {
            continue;
        }
        let ext = pts.iter().map(|p| p[0].abs().max(p[1].abs())).fold(0.0, f64::max);
        if best.is_none_or(|(b, _)| ext > b) {
            best = Some((ext, FeatureSel::Face(face.id)));
        }
    }
    best.expect("a horizontal face").1
}

fn fig8_world(stock_margin: Option<f64>, tool_name: &str) -> (World, History, String, (Point3, Point3)) {
    let world = new_world();
    let mut h = History::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/fig8.step");
    let imp = ImportModel::new(&path).expect("fig8.step imports");
    let name = imp.name.clone();
    h.execute(&world, imp);
    let e = model::find_by_name(&world, &name).expect("part");
    let b = model::entity_world_aabb(&world, &e).expect("part aabb");
    let tool = catalog().into_iter().find(|(n, _)| *n == tool_name).map(|(_, t)| t).expect("catalog tool");
    h.execute(&world, AddTool::new(&world, tool_name, tool).expect("tool"));
    h.execute(&world, camcraft_app::commands::CreateSetup::new(&world, None, None).expect("setup"));
    if let Some(m) = stock_margin {
        let dx = (b.max.x - b.min.x) + 2.0 * m;
        let dy = (b.max.y - b.min.y) + 2.0 * m;
        let dz = b.max.z - b.min.z;
        let loc = Location {
            pos: Point3::new(0.5 * (b.min.x + b.max.x), 0.5 * (b.min.y + b.max.y), b.min.z),
            ..Default::default()
        };
        h.execute(&world, SetStock::new(&world, StockShape::Box { dx, dy, dz }, loc).expect("stock"));
    }
    (world, h, name, (b.min, b.max))
}

fn entity_outline(world: &World, name: &str) -> camcraft_app::geo2d::Region {
    let e = model::find_by_name(world, name).expect("part");
    let facets = model::world_facets(world, &e).expect("facets");
    camcraft_app::geometry::slice::project(&facets, 64).expect("part outline")
}

fn min_dist_to_region(reg: &camcraft_app::geo2d::Region, pt: Point3) -> f64 {
    let mut best = f64::INFINITY;
    for l in reg.loops() {
        let pts = l.points();
        for i in 0..pts.len() {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let dd = dx * dx + dy * dy;
            let t = if dd > 0.0 { (((pt.x - a.x) * dx + (pt.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
            best = best.min((pt.x - a.x - dx * t).hypot(pt.y - a.y - dy * t));
        }
    }
    best
}

#[test]
fn fig8_bottom_face_outside_containment_generates_and_stops_at_face() {
    let (world, mut h, name, (bmin, bmax)) = fig8_world(Some(5.0), TOOL);
    let face = horizontal_face(&world, &name, bmin.z);
    let src = sel(&name, face);
    let mut p = operation::retype_params(&pocket(bmax.z, bmin.z), OpType::Adaptive2D.ordinal());
    if let OpParams::Adaptive2D { top_z, bottom_z, stepdown, .. } = &mut p {
        *top_z = Height::fixed(bmax.z);
        *bottom_z = FLOOR;
        *stepdown = (bmax.z - bmin.z) + 1.0;
    }
    let area = AreaControls { containment: Some(Containment::Outside), ..Default::default() };
    h.execute(&world, CreateOperation::new(&world, "Adp1", TOOL, src, p.clone()).expect("create"));
    h.execute(&world, EditOperationParams::with_area(&world, "Adp1", p, area).expect("area"));
    h.execute(&world, camcraft_app::commands::GenerateOperation::new(&world, "Adp1").expect("generate"));
    let e = model::find_by_name(&world, "Adp1").expect("op");
    let cached = model::toolpath_of(&world, &e).expect("cached");
    assert!(cached.warnings.is_empty(), "verifies clean: {:?}", cached.warnings);
    assert!(!cached.path.moves.is_empty());
    let posted = camcraft_app::posting::post_setup_unwritten(&world).expect("posts clean");
    assert!(!posted.program.is_empty());

    let part_outline = entity_outline(&world, &name);
    let op_val = model::operation_of(&world, &e).expect("op");
    let region = operation::region_of(&world, &op_val).expect("region previews");
    let (lo, hi) = region.bounds().expect("bounds");
    assert!(lo.x < bmin.x - 5.0 - R && hi.x > bmax.x + 5.0 + R, "region reaches air past stock: {lo:?} {hi:?}");
    let first_feed = cached.path.moves.iter().find(|m| m.kind == MoveKind::Feed).expect("first cut");
    assert!(
        min_dist_to_region(&part_outline, first_feed.from) > R + 1.0,
        "first cut enters inside free stock area away from face: {:?}",
        first_feed.from
    );
    let last_feed = cached.path.moves.iter().rfind(|m| m.kind == MoveKind::Feed).expect("last cut");
    let d_last = min_dist_to_region(&part_outline, last_feed.to);
    assert!(
        (d_last - R).abs() < 0.05,
        "last pass ends at the selected face (dist {d_last:.3}, R {R})"
    );
}

#[test]
fn fig8_outside_containment_with_em0800_and_clearing_kinds_works() {
    // EM-0800-4FL (D 8, flute 20, shank 10) within flute length clears cleanly and posts.
    let (world, mut h, name, (bmin, bmax)) = fig8_world(Some(5.0), "EM-0800-4FL");
    let r8 = 4.0;
    let face = horizontal_face(&world, &name, bmin.z);
    let src = sel(&name, face);
    let mut p = operation::retype_params(&pocket(bmax.z, bmin.z + 5.0), OpType::Adaptive2D.ordinal());
    if let OpParams::Adaptive2D { top_z, bottom_z, stepdown, entry, helix_radius, .. } = &mut p {
        *top_z = Height::fixed(bmax.z);
        *bottom_z = Height::fixed(bmin.z + 5.0);
        *stepdown = 20.0;
        *entry = Entry3D::Helix;
        *helix_radius = 1.0;
    }
    let area = AreaControls { containment: Some(Containment::Outside), ..Default::default() };
    h.execute(&world, CreateOperation::new(&world, "Adp8", "EM-0800-4FL", src, p.clone()).expect("create"));
    h.execute(&world, EditOperationParams::with_area(&world, "Adp8", p, area).expect("area"));
    h.execute(&world, camcraft_app::commands::GenerateOperation::new(&world, "Adp8").expect("generate"));
    let e = model::find_by_name(&world, "Adp8").expect("op");
    let cached = model::toolpath_of(&world, &e).expect("cached");
    assert!(cached.warnings.is_empty(), "verifies clean: {:?}", cached.warnings);
    assert!(!camcraft_app::posting::post_setup_unwritten(&world).expect("posts clean").program.is_empty());
    let part_outline = entity_outline(&world, &name);
    let first_feed = cached.path.moves.iter().find(|m| m.kind == MoveKind::Feed).expect("first cut");
    assert!(min_dist_to_region(&part_outline, first_feed.from) > r8 + 1.0, "EM-0800-4FL enters away from face");
    let last_feed = cached.path.moves.iter().rfind(|m| m.kind == MoveKind::Feed).expect("last cut");
    let d_last = min_dist_to_region(&part_outline, last_feed.to);
    assert!((d_last - r8).abs() < 0.05, "EM-0800-4FL ends at selected face (dist {d_last:.3}, r {r8})");
}

#[test]
fn outside_containment_clearing_kinds_and_enclosed_pockets() {
    let bw = block_world();
    let bsrc = sel("part", horizontal_face(&bw, "part", 0.0));
    let out_area = AreaControls { containment: Some(Containment::Outside), ..Default::default() };
    for kind in [OpType::Adaptive2D, OpType::Offset2D, OpType::Pocket] {
        let mut kp = operation::retype_params(&pocket(10.0, 0.0), kind.ordinal());
        match &mut kp {
            OpParams::Adaptive2D { top_z, bottom_z, stepdown, .. }
            | OpParams::Offset2D { top_z, bottom_z, stepdown, .. }
            | OpParams::Pocket { top_z, bottom_z, stepdown, .. } => {
                *top_z = Height::fixed(10.0);
                *bottom_z = Height::fixed(0.0);
                *stepdown = 12.0;
            }
            _ => {}
        }
        let (t, warns) = run(&bw, &op(bsrc.clone(), kp, out_area.clone())).unwrap_or_else(|e| panic!("{kind}: {e}"));
        assert_eq!(warns, 0, "{kind} verifies clean");
        let reach = max_by(&feeds(&t), |q| q.x.abs().max(q.y.abs()));
        assert!((reach - (35.0 + R)).abs() < 0.1, "{kind} reaches stock + R: {reach}");
    }

    // Enclosed pocket inside a block inside larger stock must stay inside the pocket walls.
    let pocket_mesh = pocket_support::block_with_pocket(25.0, 25.0, &pocket_support::rounded_rect(12.0, 12.0, 1.0, 1), 10.0, 20.0);
    let ew = world_with(Some(ImportModel::from_mesh("part", pocket_mesh)), 0.0, 20.0);
    let floor = horizontal_face(&ew, "part", 10.0);
    let mut ep = operation::retype_params(&pocket(20.0, 10.0), OpType::Adaptive2D.ordinal());
    if let OpParams::Adaptive2D { top_z, bottom_z, stepdown, .. } = &mut ep {
        (*top_z, *bottom_z, *stepdown) = (Height::fixed(20.0), Height::fixed(10.0), 12.0);
    }
    let (et, ewarns) = run(&ew, &op(sel("part", floor), ep, out_area.clone())).expect("enclosed pocket cuts");
    assert_eq!(ewarns, 0);
    assert!(max_by(&feeds(&et), |q| q.x.abs().max(q.y.abs())) <= 12.0 - R + 0.05, "enclosed pocket stays inside walls");

    // Enclosed circular recess on STEP model (Milling-test.step at Z 0) also stays inside its walls.
    let (mw, mname) = milling_test();
    let (rc, rr) = recess(&mw, &mname);
    let rfloor = top_face(&mw, &mname, 0.0);
    let (rt, rwarns) = run(&mw, &op(sel(&mname, rfloor), adaptive(Height::fixed(0.0)), out_area)).expect("STEP recess cuts");
    assert_eq!(rwarns, 0, "STEP recess verifies clean");
    assert!(
        feeds(&rt).into_iter().all(|q| (q.x - rc.0).hypot(q.y - rc.1) <= rr - R + 0.05),
        "STEP circular recess stays inside walls"
    );
}

#[test]
fn outside_containment_without_stock_refuses_naming_extension() {
    let (world, _h, name, (bmin, bmax)) = fig8_world(None, "EM-0800-4FL");
    let src = sel(&name, horizontal_face(&world, &name, bmin.z));
    let mut p = operation::retype_params(&pocket(bmax.z, bmin.z), OpType::Adaptive2D.ordinal());
    if let OpParams::Adaptive2D { top_z, bottom_z, stepdown, .. } = &mut p {
        (*top_z, *bottom_z, *stepdown) = (Height::fixed(bmax.z), FLOOR, (bmax.z - bmin.z) + 1.0);
    }
    let area = AreaControls { containment: Some(Containment::Outside), ..Default::default() };
    let mut o = op(src, p, area.clone());
    o.tool = "EM-0800-4FL".into();
    let err = run(&world, &o).expect_err("without stock a one-tool band has no entry room");
    assert!(err.contains("Extension"), "refusal must name the Extension control: {err}");

    let fw = new_world();
    let mut fh = History::default();
    fh.execute(&fw, ImportModel::from_mesh("part", cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 10.0))));
    let floc = Location { pos: Point3::new(0.0, 0.0, 0.0), ..Default::default() };
    fh.execute(&fw, SetStock::new(&fw, StockShape::Box { dx: 40.0, dy: 40.0, dz: 10.0 }, floc).expect("flush stock"));
    let t = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    fh.execute(&fw, AddTool::new(&fw, TOOL, t).expect("tool"));
    let fface = horizontal_face(&fw, "part", 0.0);
    let mut fp = operation::retype_params(&pocket(10.0, 0.0), OpType::Adaptive2D.ordinal());
    if let OpParams::Adaptive2D { top_z, bottom_z, stepdown, .. } = &mut fp {
        (*top_z, *bottom_z, *stepdown) = (Height::fixed(10.0), FLOOR, 12.0);
    }
    let ferr = run(&fw, &op(sel("part", fface), fp.clone(), area.clone())).expect_err("flush stock leaves no room past selection");
    assert!(ferr.contains("Extension") && ferr.contains("stock"), "refusal must name Extension and stock: {ferr}");

    // Without stock on a square block, Clipper round corners must not emit an empty toolpath.
    let nw = new_world();
    let mut nh = History::default();
    nh.execute(&nw, ImportModel::from_mesh("part", cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 10.0))));
    let nt = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    nh.execute(&nw, AddTool::new(&nw, TOOL, nt).expect("tool"));
    let nface = horizontal_face(&nw, "part", 0.0);
    let nerr = run(&nw, &op(sel("part", nface), fp, area)).expect_err("no stock on square block must refuse");
    assert!(nerr.contains("Extension") && nerr.contains("stock"), "refusal must name Extension and stock: {nerr}");
}

#[test]
fn outside_containment_air_band_fixture_blocks_post() {
    use camcraft_app::commands::{GenerateOperation, MoveFixture, TagFixture};
    use camcraft_app::verify::Warning;
    let (world, mut h, name, (bmin, bmax)) = fig8_world(Some(5.0), TOOL);
    let src = sel(&name, horizontal_face(&world, &name, bmin.z));
    let mut p = operation::retype_params(&pocket(bmax.z, bmin.z), OpType::Adaptive2D.ordinal());
    if let OpParams::Adaptive2D { top_z, bottom_z, stepdown, .. } = &mut p {
        (*top_z, *bottom_z, *stepdown) = (Height::fixed(bmax.z), FLOOR, (bmax.z - bmin.z) + 1.0);
    }
    let area = AreaControls { containment: Some(Containment::Outside), ..Default::default() };
    h.execute(&world, CreateOperation::new(&world, "Adp1", TOOL, src, p.clone()).expect("create"));
    h.execute(&world, EditOperationParams::with_area(&world, "Adp1", p, area).expect("area"));
    let clamp = cube(Point3::new(bmax.x + 6.0, bmin.y, bmin.z), Point3::new(bmax.x + 25.0, bmax.y, bmax.z));
    h.execute(&world, ImportModel::from_mesh("Clamp", clamp));
    h.execute(&world, TagFixture::new(&world, "Clamp", None).expect("tag fixture"));
    h.execute(&world, GenerateOperation::new(&world, "Adp1").expect("generate"));
    let cached = model::toolpath_of(&world, &model::find_by_name(&world, "Adp1").expect("op")).expect("cached");
    assert!(cached.warnings.iter().any(|w| matches!(w, Warning::FixtureHit { .. })), "fixture hit: {:?}", cached.warnings);
    assert!(camcraft_app::posting::post_setup_unwritten(&world).is_err(), "post gate must block air-band fixture hit");

    // Move the fixture clear and regenerate: verification and post gate both pass.
    h.execute(&world, MoveFixture::new(&world, "Clamp", Location { pos: Point3::new(100.0, 0.0, 0.0), ..Default::default() }).expect("move"));
    assert!(camcraft_app::posting::verification_gate(&world).is_empty(), "moved fixture clears live gate");
    h.execute(&world, GenerateOperation::new(&world, "Adp1").expect("regenerate"));
    assert!(!camcraft_app::posting::post_setup_unwritten(&world).expect("clear fixture posts").program.is_empty());
}
