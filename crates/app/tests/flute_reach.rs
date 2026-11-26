//! #3525 (the case of #3319): a 2D Offset on `fig8.step` whose tool has a
//! 10 mm flute, with the maintainer's heights (stock top Z 22, Top = top
//! of stock, Bottom = selection +1.5, stepdown 10). The shank of
//! EM-0800-4FL (10 mm over an 8 mm cutter) enters the cut. The first
//! warning and the first note name that cause - the tool, its flute length
//! and the depth below the stock top - before the shank and rapid
//! findings. The finding is advisory (the #3525 decision): the post stays
//! blocked by the shank findings. A 25 mm flute gets no such finding. A
//! same-diameter shank deeper than its flute meets nothing: it gets the
//! note alone, no warning, and that program posts.
//!
//! Gate map: covers app::operation (generate, flute_law), app::mcp (job
//! finding kinds), app::posting (post_setup_unwritten), sim::verify
//! (flute_reach).
//! Provenance: our own regression of the maintainer's report; no vendor
//! material consulted.

mod area_support;
mod common;

use area_support::*;
use camcraft_app::operation::HeightReference;
use camcraft_app::verify::Warning;

const EM8: &str = "EM-0800-4FL";

fn fig8_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/fig8.step")
}

/// The part's largest horizontal face at `z` (the fig8 floor).
fn floor_face(world: &World, name: &str, z: f64) -> FeatureSel {
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
        if best.as_ref().is_none_or(|(b, _)| ext > *b) {
            best = Some((ext, FeatureSel::Face(face.id)));
        }
    }
    best.expect("a horizontal face").1
}

/// The #3319 job with EM-0800-4FL ground by `grind`: the stock box 5 mm
/// past the part in XY and 2 mm over its top, the 2D Offset generated.
/// Returns the world, the path, its warnings and the depth of the lowest
/// level under the stock top.
fn job(grind: impl Fn(&mut camcraft_app::tools::Tool)) -> (World, Toolpath, Vec<Warning>, f64) {
    let world = new_world();
    let mut h = History::default();
    let imp = ImportModel::new(&fig8_path()).expect("fig8.step imports");
    let name = imp.name.clone();
    h.execute(&world, imp);
    let e = model::find_by_name(&world, &name).expect("part");
    let b = model::entity_world_aabb(&world, &e).expect("part aabb");
    let mut tool = catalog().into_iter().find(|(n, _)| *n == EM8).map(|(_, t)| t).expect("catalog tool");
    grind(&mut tool);
    h.execute(&world, AddTool::new(&world, EM8, tool).expect("tool"));
    h.execute(&world, camcraft_app::commands::CreateSetup::new(&world, None, None).expect("setup"));
    let m = 5.0;
    let (dx, dy, dz) = ((b.max.x - b.min.x) + 2.0 * m, (b.max.y - b.min.y) + 2.0 * m, (b.max.z - b.min.z) + 2.0);
    let loc = Location { pos: Point3::new(0.5 * (b.min.x + b.max.x), 0.5 * (b.min.y + b.max.y), b.min.z), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx, dy, dz }, loc).expect("stock"));
    let src = sel(&name, floor_face(&world, &name, b.min.z));
    let mut p = operation::retype_params(&pocket(b.min.z + 20.0, b.min.z), OpType::Offset2D.ordinal());
    if let OpParams::Offset2D { clearance_z, retract_z, top_z, bottom_z, stepdown, entry, helix_radius, .. } = &mut p {
        *clearance_z = Height { reference: HeightReference::TopOfStock, offset: 10.0 };
        *retract_z = None;
        *top_z = Height { reference: HeightReference::TopOfStock, offset: 0.0 };
        *bottom_z = Height { reference: HeightReference::BottomOfSelection, offset: 1.5 };
        *stepdown = 10.0;
        *entry = Entry3D::Helix;
        *helix_radius = 1.6;
    }
    let area = AreaControls { containment: Some(Containment::Outside), ..Default::default() };
    h.execute(&world, CreateOperation::new(&world, "Off1", EM8, src, p.clone()).expect("create"));
    h.execute(&world, EditOperationParams::with_area(&world, "Off1", p, area).expect("area"));
    h.execute(&world, camcraft_app::commands::GenerateOperation::new(&world, "Off1").expect("generate"));
    let op = model::find_by_name(&world, "Off1").expect("op");
    let cached = model::toolpath_of(&world, &op).expect("cached");
    let (path, warns) = (cached.path.clone(), cached.warnings.clone());
    (world, path, warns, (b.max.z + 2.0) - (b.min.z + 1.5))
}

fn texts(warns: &[Warning]) -> Vec<String> {
    warns.iter().map(ToString::to_string).collect()
}

/// The #3319 case: flute 10 mm, shank 10 mm over an 8 mm cutter. The
/// search on #3319 read three rapid findings beside the shank runs.
/// #3811: the count follows the ring plan (3 at v0.26.0, 12 after #3790,
/// 9 after #3818, 3 after #3824), so the test pins the cause and an upper
/// bound. Each rapid
/// finding is a straight lift or drop along the tool axis whose low end
/// puts the shank under the part top: the shank of the short flute, not a
/// traverse through the part. The 25 mm flute twin below has none.
#[test]
#[ignore = "heavy lane (#3525 fig8 flute 10): one generation and its verify run past the 60 s debug budget (about 65 s) - run with --release -- --ignored"]
fn a_ten_mm_flute_names_its_cause_before_the_rapid_findings() {
    let (path, warns) = named_first(10.0);
    let all = texts(&warns);
    let Some(Warning::FluteShort { depth_mm, .. }) = warns.first().copied() else {
        panic!("the first warning names the flute: {all:?}");
    };
    // The lowest level stands `depth_mm` under the stock top; the part
    // top is 2 mm under the stock top.
    let floor = path.moves.iter().filter(|m| m.kind != MoveKind::Rapid).map(|m| m.to.z).fold(f64::INFINITY, f64::min);
    let part_top = floor + depth_mm - 2.0;
    let rapids: Vec<usize> = warns.iter().filter_map(|w| if let Warning::RapidInPart { at } = *w { Some(at) } else { None }).collect();
    assert!(!rapids.is_empty(), "the shank meets the part on a rapid too: {all:?}");
    eprintln!("flute 10: {} rapid finding(s): {rapids:?}", rapids.len());
    // #3824: the part wall reads as one wall, so the pass links to its
    // slivers at X about +-46.5, Y about 123.2 no longer lift and drop
    // there: 9 findings before, 3 after.
    assert!(rapids.len() <= 3, "#3824: {} rapid findings, at most 3: {rapids:?}", rapids.len());
    for at in rapids {
        let m = &path.moves[at];
        let vertical = (m.to.x - m.from.x).abs() < 1e-9 && (m.to.y - m.from.y).abs() < 1e-9;
        assert!(vertical, "rapid {at} is a lift or a drop, not a traverse through the part: {m:?}");
        let low = m.from.z.min(m.to.z);
        assert!(low + 10.0 < part_top, "rapid {at} puts the shank under the part top {part_top}: {m:?}");
    }
    assert!(!warns.iter().any(|w| matches!(w, Warning::Gouge { .. } | Warning::RapidInStock { .. })), "{all:?}");
}

/// The catalog flute (20 mm) is short of the 20.5 mm cut too: the same
/// finding first, then the shank findings of the last level. The debug
/// twin of the heavy #3319 case.
#[test]
fn the_catalog_flute_names_its_cause_before_the_shank_findings() {
    let _ = named_first(20.0);
}

/// The note the flute finding gives on this job.
const NOTE: &str = "EM-0800-4FL has a flute length of {F} mm; this operation cuts 20.500 mm below the stock top; above the \
     flutes, the shank or the holder goes into the cut and can rub or collide - use a tool with a flute length of at \
     least 20.500 mm, or cut less deep";

/// The job with a `flute` mm flute: the flute finding first and once,
/// its note first with the tool's name, the shank findings after it, and
/// the post held by those shank findings. Returns the path and its
/// warnings.
fn named_first(flute: f64) -> (Toolpath, Vec<Warning>) {
    let (world, path, warns, want) = job(|t| t.flute_len = flute);
    let all = texts(&warns);
    let Some(Warning::FluteShort { at, flute_mm, depth_mm }) = warns.first().copied() else {
        panic!("the first warning names the flute: {all:?}");
    };
    assert_eq!(flute_mm, flute, "{all:?}");
    // The lowest level is the selection +1.5 over the floor: 2 mm of stock
    // over the part's 20 mm, so 20.5 mm under the stock top.
    assert!((depth_mm - want).abs() < 0.01 && (want - 20.5).abs() < 0.01, "depth {depth_mm}: {all:?}");
    assert!(!warns[0].blocks_post(), "advisory: {}", all[0]);
    assert_eq!(
        all[0],
        format!(
            "the tool's flute length is {flute:.3} mm, but the cut goes 20.500 mm below the stock top from move {at}: above \
             the flutes, the shank or the holder goes into the cut; the findings after this one name the moves where it \
             meets material - use a tool with a longer flute, or cut less deep (this finding alone does not hold the post)"
        )
    );
    // The note names the tool, its flute and the depth, first.
    assert_eq!(path.notes.first().cloned().unwrap_or_default(), NOTE.replace("{F}", &format!("{flute:.3}")));
    // Every rapid and shank finding comes after it; none is lost.
    let first_rapid = warns.iter().position(|w| matches!(w, Warning::RapidInPart { .. }));
    let first_shank = warns.iter().position(|w| matches!(w, Warning::ShankHit { .. }));
    assert!(first_shank.is_some(), "the shank still collides: {all:?}");
    assert!(first_rapid.is_none_or(|i| i > 0) && first_shank.is_some_and(|i| i > 0), "{all:?}");
    assert_eq!(warns.iter().filter(|w| matches!(w, Warning::FluteShort { .. })).count(), 1, "{all:?}");
    // The post stays blocked, by the shank findings.
    let err = camcraft_app::posting::post_setup_unwritten(&world).expect_err("the post must be held");
    assert!(err.contains("shank/holder at move"), "the shank findings hold the post: {err}");
    (path, warns)
}

/// A 25 mm flute reaches the 20.5 mm cut: no flute finding. #3811: the
/// same ring plan has no shank, rapid or gouge finding either, so each
/// contact of the 10 mm case comes from the short flute.
#[test]
fn a_twenty_five_mm_flute_gets_no_flute_finding() {
    let (_, path, warns, _) = job(|t| t.flute_len = 25.0);
    assert!(!warns.iter().any(|w| matches!(w, Warning::FluteShort { .. })), "{:?}", texts(&warns));
    let contact =
        |w: &Warning| matches!(w, Warning::ShankHit { .. } | Warning::RapidInPart { .. } | Warning::RapidInStock { .. } | Warning::Gouge { .. });
    assert!(!warns.iter().any(contact), "a tool that reaches meets nothing: {:?}", texts(&warns));
    assert!(!path.notes.iter().any(|n| n.contains("has a flute length of")), "{:?}", path.notes);
}

/// A same-diameter shank deeper than its 10 mm flute: nothing collides,
/// so the cause is the advisory note alone (no warning), and the program
/// posts.
#[test]
fn a_same_diameter_shank_past_the_flute_is_an_advisory_note() {
    let (world, path, warns, _) = job(|t| {
        t.flute_len = 10.0;
        t.shank_d = t.diameter;
    });
    assert!(!warns.iter().any(|w| matches!(w, Warning::FluteShort { .. })), "{:?}", texts(&warns));
    assert!(!warns.iter().any(Warning::blocks_post), "nothing collides: {:?}", texts(&warns));
    assert_eq!(path.notes.first().cloned().unwrap_or_default(), NOTE.replace("{F}", "10.000"));
    let posted = camcraft_app::posting::post_setup_unwritten(&world);
    assert!(posted.is_ok(), "nothing collides, so the program posts: {posted:?}");
}