//! #1164 (part of #290): a wire taper posted as an ANGLE word on a
//! machine that states that dialect and its control's corner rule as
//! data (`edm.taper_angle`: the side a positive angle leans the top,
//! and `mitre` or `cone` at corners). The generator and the wire laws
//! model the stated rule, so a corner the rule gouges blocks the post;
//! a rule left unstated, a machine stating neither U/V nor an angle
//! word, and a 4-axis cut on an angle-word machine are refused by name.
//! The fixture's angle code is invented (D10); not exercised on a real
//! EDM control.
//!
//! Gate map (#1164): covers motion::wire_edm (taper: the cone corner),
//! post::machine::edm_taper, app::operation::wire_op (the corner rule's
//! door), app::posting_wire (WireAngle, the top-plane X Y) and the
//! wire-iso.cps angle words.
//! Provenance: our own tests over our own synthetic fixture.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{CreateOperation, GenerateOperation, History, ImportModel, LoadMachine};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::parse_mch;
use camcraft_app::model;
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, OpParams, SrcRef, WireContourOp, WireFourAxis, WirePairingRule, WireSide};
use camcraft_app::posting;
use camcraft_app::world::new_world;

const UV: &str = include_str!("edm/wire-iso-fixture.mch");
const WIRE: &str = "Generic ISO Wire EDM";
const NC: &str = "tests/golden/nc/wire-iso/wire_taper_angle_punch.nc";
const TAPER_CL: &str = "tests/golden/wire_taper_punch.cl";
const H: f64 = 12.0;

fn cps() -> &'static str {
    camcraft_app::posting::builtin_post_text(WIRE).expect("registered")
}

/// The synthetic fixture turned angle-word: no `uv`, an invented angle
/// template, and the stated `rules` object (JSON members, may be empty).
fn angle_machine(rules: &str) -> String {
    UV.replace("\"uv\": \"U{u} V{v}\"", "\"taper_angle\": \"FX-ANGLE A{angle}\"")
        .replace("\"max_taper_deg\": 15.0,", &format!("\"max_taper_deg\": 15.0,\n    \"taper_angle\": {{ {rules} }},"))
}

fn rules(corner: &str) -> String {
    format!("\"positive\": \"top_left\", \"corner\": \"{corner}\"")
}

/// The ring (x, y) at height z, as a band of quads to the ring above.
fn prism(lo: &[(f64, f64)], hi: &[(f64, f64)]) -> Mesh {
    let p = |(x, y): (f64, f64), z: f64| Point3::new(x, y, z);
    let mut f = Vec::new();
    let n = lo.len();
    for i in 0..n {
        let j = (i + 1) % n;
        f.push(Triangle::new(Point3::new(0.0, 0.0, H), p(hi[i], H), p(hi[j], H)));
        f.push(Triangle::new(Point3::new(0.0, 0.0, 0.0), p(lo[j], 0.0), p(lo[i], 0.0)));
        f.push(Triangle::new(p(lo[i], 0.0), p(lo[j], 0.0), p(hi[j], H)));
        f.push(Triangle::new(p(lo[i], 0.0), p(hi[j], H), p(hi[i], H)));
    }
    Mesh::from_facets(f)
}

fn rect(hx: f64, hy: f64) -> Vec<(f64, f64)> {
    vec![(hx, hy), (-hx, hy), (-hx, -hy), (hx, -hy)]
}

/// #536's sharp 40 x 20 block, vertical walls.
fn block() -> Mesh {
    prism(&rect(20.0, 10.0), &rect(20.0, 10.0))
}

/// A punch drafted 3 degrees: its top the block, its base wider by the
/// lean on every side, the corners sharp at every height.
fn drafted() -> Mesh {
    let d = H * 3.0f64.to_radians().tan();
    prism(&rect(20.0 + d, 10.0 + d), &rect(20.0, 10.0))
}

fn top_face(world: &World) -> GeoSel {
    let e = model::find_by_name(world, "part").expect("part");
    let f = model::features_of(world, &e).expect("features");
    let id = f
        .faces
        .iter()
        .filter(|fa| fa.normal[2].abs() > 0.99)
        .filter(|fa| fa.loops.iter().flat_map(|l| &l.verts).any(|&v| (f.verts[v as usize][2] - H).abs() < 1e-9))
        .max_by(|a, b| a.area.total_cmp(&b.area))
        .map(|fa| fa.id)
        .expect("a top face");
    GeoSel { entity: "part".into(), feature: FeatureSel::Face(id) }
}

/// `machine` mounted, `mesh` imported as "part" and a punch around it
/// at 3 degrees (its top face picked when `pick`); generation's answer.
fn punch(machine: &str, mesh: Mesh, pick: bool, four_axis: Option<WireFourAxis>) -> (World, Result<(), String>) {
    let world = new_world();
    let mut h = History::default();
    let m = parse_mch(machine).expect("the fixture parses");
    h.execute(&world, LoadMachine::new(&world, "wire-angle.mch".into(), Some(m)).expect("mount"));
    h.execute(
        &world,
        camcraft_app::commands::AddTool::new(&world, "EDM-W025", camcraft_app::wire_tools::wire_tool(0.25))
            .expect("wire"),
    );
    h.execute(&world, ImportModel::from_mesh("part", mesh));
    let src = if pick { SrcRef::Selection(vec![top_face(&world)]) } else { SrcRef::Entity("part".into()) };
    let op = WireContourOp {
        lead_arc_radius: None,
        top_z: Height::fixed(H),
        bottom_z: Height::fixed(0.0),
        condition: "C110".into(),
        side: WireSide::Punch,
        picks: camcraft_app::path_picks::PathPicks { start: vec![camcraft_app::path_picks::PathPick::xy(camcraft_app::geo2d::Point2::new(-30.0, 0.0))], exit: Vec::new() },
        tabs: None,
        // The punch drops into a stated capture (#539's hold law).
        capture: true,
        both_pieces_restrained: false,
        stock_centered_in_xy_travel: false,
        taper_deg: if four_axis.is_some() { 0.0 } else { 3.0 },
        burnout: None,
        clearance: camcraft_app::operation::DEFAULT_CLEARANCE,
        four_axis,
        skims: Vec::new(),
        sharp_corners: false,
    };
    let c = CreateOperation::new(&world, "Punch1", "EDM-W025", src, OpParams::WireContour(op)).expect("op");
    h.execute(&world, c);
    let made = GenerateOperation::new(&world, "Punch1").map(|g| h.execute(&world, g)).map_err(|e| e.to_string());
    (world, made)
}

fn path(world: &World) -> camcraft_app::cl::Toolpath {
    let e = model::find_by_name(world, "Punch1").expect("op");
    model::toolpath_of(world, &e).expect("generated").path
}

fn post(world: &World, tag: &str) -> Result<String, String> {
    let out = std::env::temp_dir().join(format!("camcraft-wire-angle-{tag}-{}.nc", std::process::id()));
    let posted = posting::post_script(world, &out, cps());
    let wrote = out.exists();
    let _ = std::fs::remove_file(&out);
    assert_eq!(wrote, posted.is_ok(), "{tag}: NC text is written only for a posted program");
    posted.map(|p| p.program)
}

fn golden(file: &str, text: &str) {
    if std::env::var_os("WRITE_GOLDEN").is_some() {
        std::fs::write(file, text).expect("write golden");
    }
    assert_eq!(text, std::fs::read_to_string(file).expect("golden present"), "{file} drifted");
}

fn lean(a: [f64; 3]) -> f64 {
    a[0].hypot(a[1]).atan2(a[2]).to_degrees()
}

/// Done means: a tapered punch on an angle-word machine posts its
/// golden - the angle word, X Y on the top plane, no U V - and under
/// the mitre rule its CL is #536's byte for byte (what the control
/// cuts is what the U/V post wrote); the side rule signs the word.
#[test]
fn an_angle_word_punch_posts_its_golden_with_the_mitre_rule() {
    let (world, made) = punch(&angle_machine(&rules("mitre")), block(), false, None);
    made.expect("a 3 degree punch generates on an angle-word machine");
    let cl = camcraft_app::cl::text::to_text(&path(&world)).expect("cl text");
    assert_eq!(cl, std::fs::read_to_string(TAPER_CL).expect("golden present"), "the mitre rule is #536's path");
    let clause = posting::wire_clause(&world);
    assert!(clause.problems.is_empty(), "{:?}", clause.problems);
    let text = post(&world, "mitre").expect("the angle-word punch posts");
    golden(NC, &text);
    let lines: Vec<&str> = text.lines().collect();
    let at = |w: &str| lines.iter().position(|l| *l == w).unwrap_or_else(|| panic!("no {w} in\n{text}"));
    let (on, ang, off, cut) = (at("FX-TAPER-ON"), at("FX-ANGLE A3."), at("FX-TAPER-OFF"), at("FX-CUT"));
    assert!(on + 1 == ang && ang < off && off < cut, "{text}");
    assert!(!text.contains(" U") && !text.contains(" V"), "no U V words: {text}");
    assert_eq!(lines[off + 1], "G1 X-30. Y0.", "the taper is switched off before the lead-out stands the wire upright: {text}");
    assert!(lines[ang + 1..off].contains(&"G1 X-20.165 Y-10.165"), "X Y on the top plane, at the kerf: {text}");
    // The other side rule writes the other sign.
    let (world, made) = punch(&angle_machine(&rules("mitre")).replace("top_left", "top_right"), block(), false, None);
    made.expect("generates");
    assert!(post(&world, "right").expect("posts").contains("FX-ANGLE A-3."));
}

/// Done means: under the cone rule the path pivots about each top
/// corner with the lean held at the angle (posed turning moves the
/// control makes itself, so the NC is the mitre's but for its note).
#[test]
fn the_cone_rule_turns_the_wire_about_each_corner() {
    let (world, made) = punch(&angle_machine(&rules("cone")), block(), false, None);
    made.expect("generates under the cone rule");
    let p = path(&world);
    let poses: Vec<[f64; 3]> = p.moves.iter().map(|m| m.axis.expect("posed")).collect();
    let (last, cut) = poses.split_last().expect("moves");
    assert!(lean(*last) == 0.0, "upright at the start hole");
    assert!(cut.iter().all(|a| (lean(*a) - 3.0).abs() < 1e-9), "the cone holds the lean: {poses:?}");
    assert!(poses.len() > 4 * 45, "each quarter turn laid at most 2 degrees a move: {}", poses.len());
    assert!(p.notes.iter().any(|n| n.contains("corner rule is a cone")), "{:?}", p.notes);
    let clause = posting::wire_clause(&world);
    assert!(clause.problems.is_empty(), "{:?}", clause.problems);
    let text = post(&world, "cone").expect("posts");
    let unsaid = |t: &str| t.lines().filter(|l| !l.contains("BY ANGLE WORD")).collect::<Vec<_>>().join("\n");
    assert_eq!(unsaid(&text), unsaid(&std::fs::read_to_string(NC).expect("golden present")));
    assert!(text.contains("CORNERS CONE"), "{text}");
}

/// Done means: the verify judges the lean the stated rule cuts - a
/// drafted punch with sharp corners is cut true under the mitre rule,
/// and the cone rule's rounded lower corners burn it: the post blocks.
#[test]
fn a_corner_the_stated_rule_gouges_blocks_the_post() {
    let (world, made) = punch(&angle_machine(&rules("mitre")), drafted(), true, None);
    made.expect("the drafted punch generates under the mitre rule");
    let clause = posting::wire_clause(&world);
    assert!(clause.problems.is_empty(), "{:?}", clause.problems);
    post(&world, "drafted-mitre").expect("the mitre rule cuts the drafted punch true");
    let (world, made) = punch(&angle_machine(&rules("cone")), drafted(), true, None);
    made.expect("the cone path generates - the verify judges it");
    let err = post(&world, "drafted-cone").expect_err("a gouging corner never posts");
    assert!(err.starts_with("post blocked") && err.contains("Punch1: wire gouge"), "{err}");
}

/// Done means: an angle-word machine with a rule unstated, a machine
/// stating neither U/V nor an angle word, and a 4-axis cut on an
/// angle-word machine are each refused by name.
#[test]
fn an_incomplete_dialect_and_a_four_axis_angle_cut_are_refused_by_name() {
    let (_, made) = punch(&angle_machine("\"positive\": \"top_left\""), block(), false, None);
    assert!(made.expect_err("no corner rule").contains("states no edm.taper_angle.corner (mitre or cone)"));
    let (_, made) = punch(&angle_machine("\"corner\": \"cone\""), block(), false, None);
    assert!(made.expect_err("no side rule").contains("states no edm.taper_angle.positive"));
    let neither = UV.replace(",\n      \"uv\": \"U{u} V{v}\"", "");
    assert!(neither != UV, "the fixture lost its uv code");
    let (world, made) = punch(&neither, block(), false, None);
    made.expect("the cut generates");
    let err = post(&world, "neither").expect_err("no taper dialect");
    assert!(err.contains("states no edm.codes.uv (nor an angle-word taper, edm.taper_angle)"), "{err}");
    let four = WireFourAxis { pairing: WirePairingRule::ArcLength, ..WireFourAxis::default() };
    let (_, made) = punch(&angle_machine(&rules("mitre")), block(), false, Some(four));
    let err = made.expect_err("a 4-axis cut on an angle-word machine");
    assert!(err.contains("tapers by angle word") && err.contains("4-axis"), "{err}");
}

/// #1164 review (PR #1218): the machine's angle-word rules are in the
/// stamp - a corner rule swapped from mitre to cone, or a machine
/// swapped between U/V and the angle word, leaves no path fresh (a
/// mitre path judged as a mitre never posts to a control cutting cones).
#[test]
fn the_corner_rule_and_the_dialect_each_move_the_stamp() {
    let stamp = |machine: &str| {
        let (world, made) = punch(machine, block(), false, None);
        made.expect("generates");
        let e = model::find_by_name(&world, "Punch1").expect("op");
        let op = model::operation_of(&world, &e).expect("op");
        operation::current_stamp(&world, &op.tool, &op.src, &op.params, "", None).expect("a stamp")
    };
    let (uv, mitre, cone) = (stamp(UV), stamp(&angle_machine(&rules("mitre"))), stamp(&angle_machine(&rules("cone"))));
    assert_ne!(mitre, cone, "mitre and cone stamp alike");
    assert_ne!(uv, mitre, "the U/V and angle-word machines stamp alike");
    assert_ne!(uv, cone, "the U/V and cone machines stamp alike");
}
