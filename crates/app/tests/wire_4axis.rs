//! #537 (part of #290): the 4-axis wire cut, end to end. A square-bottom
//! / round-top punch pairs its two picked profiles into rulings, passes
//! the wire laws through the whole height and posts a golden through
//! the generic wire post on the synthetic fixture (D10); a constant
//! taper stated as two profiles with sync pairs is #536's golden, and
//! arc length pairs the same profiles differently; a crossed, folded or
//! unpaired sync pairing, a ruling past the machine's maximum taper or
//! its U stroke, and profiles off the op's planes are refused by name;
//! a ruling whose kerf burns the part blocks the post. Not exercised on
//! a real EDM control (D10).
//!
//! Gate map (#537): covers motion::wire_edm (ruled, pairing,
//! ruled_laws), app::operation::wire_ruled (the two picked profiles),
//! app::operation::wire_op, app::operation::wire_verify,
//! app::posting_wire (WireFourAxis), app::project::wire_edm_dto and the
//! wire-iso.cps 4-axis words.
//! Provenance: our own tests over our own synthetic fixture.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel, LoadMachine};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::parse_mch;
use camcraft_app::model;
use camcraft_app::operation::{
    self, FeatureSel, GeoSel, Height, OpParams, OpStatus, SrcRef, WireContourOp, WireFourAxis, WirePairingRule, WireSide, WireTabs,
    WireSyncPair,
};
use camcraft_app::posting::{self, BUILTINS};
use camcraft_app::project;
use camcraft_app::world::new_world;

const FIXTURE: &str = include_str!("edm/wire-iso-fixture.mch");
const WIRE: &str = "Generic ISO Wire EDM";
const NC: &str = "tests/golden/nc/wire-iso/wire_4axis_punch.nc";
const CL: &str = "tests/golden/wire_4axis_punch.cl";
const TAPER_NC: &str = "tests/golden/nc/wire-iso/wire_taper_punch.nc";
const TAPER_CL: &str = "tests/golden/wire_taper_punch.cl";
const H: f64 = 12.0;
/// Points round each ring of the lofted fixtures.
const N: usize = 64;

type P2 = (f64, f64);

fn cps() -> &'static str {
    BUILTINS.iter().find(|(n, _)| *n == WIRE).map(|(_, s)| *s).expect("registered")
}

/// A square of half-side `a` as N points, counter-clockwise from the
/// middle of its left side - the fraction j/N of its perimeter each.
fn square(a: f64) -> Vec<P2> {
    (0..N)
        .map(|j| {
            let s = 8.0 * a * j as f64 / N as f64;
            match s {
                s if s <= a => (-a, -s),
                s if s <= 3.0 * a => (s - 2.0 * a, -a),
                s if s <= 5.0 * a => (a, s - 4.0 * a),
                s if s <= 7.0 * a => (6.0 * a - s, a),
                s => (-a, 8.0 * a - s),
            }
        })
        .collect()
}

/// A circle of radius `r` as N points, counter-clockwise from its
/// leftmost point.
fn circle(r: f64) -> Vec<P2> {
    (0..N)
        .map(|j| {
            let t = std::f64::consts::PI + std::f64::consts::TAU * j as f64 / N as f64;
            (r * t.cos(), r * t.sin())
        })
        .collect()
}

fn at((x, y): P2, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// The band between ring `lo` at `z0` and ring `hi` at `z1`, point j to
/// point j, each quad split along its inner diagonal - so the mesh
/// stands inside the ruled surface the same pairing lays.
fn band(f: &mut Vec<Triangle>, lo: &[P2], z0: f64, hi: &[P2], z1: f64) {
    let n = lo.len();
    for j in 0..n {
        let k = (j + 1) % n;
        let (a, b, c, d) = (at(lo[j], z0), at(lo[k], z0), at(hi[k], z1), at(hi[j], z1));
        let mid = |p: Point3, q: Point3| ((p.x + q.x) * 0.5).hypot((p.y + q.y) * 0.5);
        if mid(a, c) <= mid(b, d) {
            f.push(Triangle::new(a, b, c));
            f.push(Triangle::new(a, c, d));
        } else {
            f.push(Triangle::new(a, b, d));
            f.push(Triangle::new(b, c, d));
        }
    }
}

fn cap(f: &mut Vec<Triangle>, ring: &[P2], z: f64, up: bool) {
    let n = ring.len();
    for j in 0..n {
        let (p, q) = (at(ring[j], z), at(ring[(j + 1) % n], z));
        f.push(if up { Triangle::new(Point3::new(0.0, 0.0, z), p, q) } else { Triangle::new(Point3::new(0.0, 0.0, z), q, p) });
    }
}

/// `lower` at z = 0 lofted to `upper` at z = H.
fn loft(lower: &[P2], upper: &[P2]) -> Mesh {
    let mut f = Vec::new();
    cap(&mut f, lower, 0.0, false);
    cap(&mut f, upper, H, true);
    band(&mut f, lower, 0.0, upper, H);
    Mesh::from_facets(f)
}

/// A square post under a round boss: the same two end faces as a loft,
/// and material a ruling between them would cut through.
fn stepped(a: f64, r: f64) -> Mesh {
    let (sq, rd) = (square(a), circle(r));
    let mut f = Vec::new();
    cap(&mut f, &sq, 0.0, false);
    band(&mut f, &sq, 0.0, &sq, H * 0.5);
    band(&mut f, &sq, H * 0.5, &rd, H * 0.5);
    band(&mut f, &rd, H * 0.5, &rd, H);
    cap(&mut f, &rd, H, true);
    Mesh::from_facets(f)
}

fn rect(hx: f64, hy: f64) -> Vec<P2> {
    vec![(-hx, -hy), (hx, -hy), (hx, hy), (-hx, hy)]
}

/// The horizontal face of "part" at height `z`.
fn face_at(world: &World, z: f64) -> GeoSel {
    let e = model::find_by_name(world, "part").expect("part");
    let f = model::features_of(world, &e).expect("features");
    let id = f
        .faces
        .iter()
        .filter(|fa| fa.normal[2].abs() > 0.99)
        .filter(|fa| fa.loops.iter().flat_map(|l| &l.verts).any(|&v| (f.verts[v as usize][2] - z).abs() < 1e-9))
        .max_by(|a, b| a.area.total_cmp(&b.area))
        .map(|fa| fa.id)
        .unwrap_or_else(|| panic!("a horizontal face at Z{z}"));
    GeoSel { entity: "part".into(), feature: FeatureSel::Face(id) }
}

fn four(pairing: WirePairingRule, sync: &[(Option<P2>, Option<P2>)]) -> WireFourAxis {
    let pt = |q: Option<P2>| q.map(|(x, y)| [x, y]);
    WireFourAxis {
        pairing,
        sync: sync.iter().map(|(u, l)| WireSyncPair { upper: pt(*u), lower: pt(*l) }).collect(),
        ..WireFourAxis::default()
    }
}

fn arc() -> WireFourAxis {
    four(WirePairingRule::ArcLength, &[])
}

fn wire_op(four_axis: Option<WireFourAxis>) -> WireContourOp {
    WireContourOp {
        top_z: Height::fixed(H),
        bottom_z: Height::fixed(0.0),
        condition: "C110".into(),
        side: WireSide::Punch,
        start: Some([-30.0, 0.0]),
        tabs: None,
        // The punch drops into a stated capture (#539's hold law).
        capture: true,
        taper_deg: 0.0,
        burnout: None,
        clearance: camcraft_app::operation::DEFAULT_CLEARANCE,
        four_axis,
        skims: Vec::new(),
    }
}

/// The machine `text` mounted, `mesh` imported as "part", and a punch
/// between the faces at the heights `picks` created as `op`; the
/// generator's answer returned.
fn punch(text: &str, mesh: Mesh, picks: &[f64], op: WireContourOp) -> (World, Result<(), String>) {
    let world = new_world();
    let mut h = History::default();
    let m = parse_mch(text).expect("the fixture parses");
    h.execute(&world, LoadMachine::new(&world, "wire-iso.mch".into(), Some(m)).expect("mount"));
    h.execute(&world, ImportModel::from_mesh("part", mesh));
    let src = SrcRef::Selection(picks.iter().map(|z| face_at(&world, *z)).collect());
    let c = CreateOperation::new(&world, "Punch1", "", src, OpParams::WireContour(op)).expect("op");
    h.execute(&world, c);
    let made = GenerateOperation::new(&world, "Punch1").map(|g| h.execute(&world, g)).map_err(|e| e.to_string());
    (world, made)
}

fn path(world: &World) -> camcraft_app::cl::Toolpath {
    let e = model::find_by_name(world, "Punch1").expect("op");
    model::toolpath_of(world, &e).expect("generated").path
}

fn post(world: &World, tag: &str) -> Result<String, String> {
    let out = std::env::temp_dir().join(format!("camcraft-wire-4axis-{tag}-{}.nc", std::process::id()));
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

fn cl(world: &World) -> String {
    camcraft_app::cl::text::to_text(&path(world)).expect("cl text")
}

/// The frustum #536's 3 degree punch cuts: its top the 40 x 20 block,
/// its base wider by the lean.
fn frustum() -> (Mesh, f64) {
    let d = H * 3.0f64.to_radians().tan();
    (loft(&rect(20.0 + d, 10.0 + d), &rect(20.0, 10.0)), d)
}

fn corner_sync(d: f64) -> Vec<(Option<P2>, Option<P2>)> {
    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .iter()
        .map(|(x, y)| (Some((x * 20.0, y * 10.0)), Some((x * (20.0 + d), y * (10.0 + d)))))
        .collect()
}

/// Done means: a square-to-round transition posts and verifies; the CL
/// golden shows the poses and the NC golden the U V words.
#[test]
fn a_square_to_round_punch_posts_its_golden_and_verifies() {
    let (world, made) = punch(FIXTURE, loft(&square(8.0), &circle(9.6)), &[H, 0.0], wire_op(Some(arc())));
    made.expect("a square-to-round punch generates");
    let p = path(&world);
    let lean = |a: [f64; 3]| a[0].hypot(a[1]).atan2(a[2]).to_degrees();
    let poses: Vec<[f64; 3]> = p.moves.iter().map(|m| m.axis.expect("every 4-axis move states its pose")).collect();
    assert!(poses.len() > N, "a ruling at every vertex: {}", poses.len());
    assert!(poses.iter().all(|a| lean(*a) < 9.0), "within the fixture's U/V stroke");
    assert!(poses.last().is_some_and(|a| lean(*a) == 0.0), "upright at the start hole");
    let said = p.notes.join("\n");
    assert!(said.contains("4-axis") && said.contains("paired by normalised arc length"), "{said}");
    assert!(said.contains("stands on the profile planes"), "the twist's widening is said: {said}");
    // The wire laws read the ruled kerf at every judged height.
    let clause = posting::wire_clause(&world);
    assert!(clause.problems.is_empty(), "{:?}", clause.problems);
    golden(CL, &cl(&world));
    let text = post(&world, "ok").expect("the 4-axis punch posts");
    golden(NC, &text);
    let lines: Vec<&str> = text.lines().collect();
    let line = |w: &str| lines.iter().position(|l| *l == w).unwrap_or_else(|| panic!("no {w} in\n{text}"));
    let (on, off, cut) = (line("FX-TAPER-ON"), line("FX-TAPER-OFF"), line("FX-CUT"));
    assert!(on < off && off < cut, "{text}");
    assert!(lines[on + 1..off].iter().all(|l| l.starts_with("G1 X") && l.contains(" U") && l.contains(" V")), "{text}");
    assert!(lines[..on].iter().any(|l| l.contains("4-AXIS") && l.contains("ARC LENGTH")), "{text}");
    assert!(lines[off - 1].ends_with("U0 V0"), "the lead-out stands the wire upright: {text}");
}

/// Done means: a constant taper stated as two profiles with sync pairs
/// at the corners is #536's golden (CL byte for byte; NC but for the
/// line that names the cut), and arc length pairs the same two
/// profiles differently, as stated.
#[test]
fn a_constant_taper_by_sync_points_is_the_taper_golden_and_arc_length_differs() {
    let (mesh, d) = frustum();
    let by_sync = four(WirePairingRule::SyncPoints, &corner_sync(d));
    let (world, made) = punch(FIXTURE, mesh, &[H, 0.0], wire_op(Some(by_sync)));
    made.expect("the frustum generates by sync points");
    let sync_cl = cl(&world);
    assert_eq!(sync_cl, std::fs::read_to_string(TAPER_CL).expect("golden present"), "the taper golden's moves and poses");
    let text = post(&world, "sync").expect("posts");
    let named = |t: &str| t.lines().filter(|l| !l.contains("TAPER 3 DEG") && !l.contains("4-AXIS")).collect::<Vec<_>>().join("\n");
    assert_eq!(named(&text), named(&std::fs::read_to_string(TAPER_NC).expect("golden present")));
    assert!(text.contains("PAIRED BY SYNC POINTS"), "{text}");
    // By arc length the corners no longer meet: more rulings, other poses.
    let (world, made) = punch(FIXTURE, frustum().0, &[H, 0.0], wire_op(Some(arc())));
    made.expect("the frustum generates by arc length");
    let arc_cl = cl(&world);
    assert!(arc_cl != sync_cl && arc_cl.lines().count() > sync_cl.lines().count(), "{arc_cl}");
    assert!(path(&world).notes.iter().any(|n| n.contains("paired by normalised arc length")));
}

/// Done means: a deliberately crossed sync point is refused by name; so
/// are a folding pairing and an unpaired sync point.
#[test]
fn crossed_folded_and_unpaired_sync_points_are_refused_by_name() {
    let refused = |sync: Vec<(Option<P2>, Option<P2>)>| {
        let (mesh, _) = frustum();
        punch(FIXTURE, mesh, &[H, 0.0], wire_op(Some(four(WirePairingRule::SyncPoints, &sync)))).1.expect_err("refused")
    };
    let d = frustum().1;
    let mut crossed = corner_sync(d);
    let (a, b) = (crossed[1].0, crossed[2].0);
    (crossed[1].0, crossed[2].0) = (b, a);
    let err = refused(crossed);
    assert!(err.contains("Punch1") && err.contains("sync pair #3 crosses sync pair #2"), "{err}");
    // One pair, a half turn off: every ruling runs against its profile.
    let err = refused(vec![(Some((20.0, 10.0)), Some((-20.0 - d, -10.0 - d)))]);
    assert!(err.contains("folds near") && err.contains("consecutive rulings cross within the part height"), "{err}");
    let err = refused(vec![(Some((20.0, 10.0)), None)]);
    assert!(err.contains("sync pair #1 is unpaired"), "{err}");
    let err = refused(vec![(Some((0.0, 0.0)), Some((20.0 + d, 10.0 + d)))]);
    assert!(err.contains("sync pair #1's upper point") && err.contains("is not on the upper profile"), "{err}");
}

/// Done means: a ruling past the machine's maximum taper is refused at
/// generation, one past the U stroke blocks the post, and profiles that
/// are not two, or not on the op's planes, are refused - each by name.
#[test]
fn a_pairing_past_the_machine_or_off_the_planes_is_refused_by_name() {
    let part = || loft(&square(8.0), &circle(9.6));
    let (_, made) = punch(&FIXTURE.replace("\"max_taper_deg\": 15.0,", "\"max_taper_deg\": 5.0,"), part(), &[H, 0.0], wire_op(Some(arc())));
    let err = made.expect_err("an 8 degree ruling on a 5 degree machine");
    assert!(err.contains("Punch1") && err.contains("leans the wire") && err.contains("at most 5.000 degrees"), "{err}");
    let (_, made) = punch(&FIXTURE.replace("\"max_taper_deg\": 15.0,", ""), part(), &[H, 0.0], wire_op(Some(arc())));
    assert!(made.expect_err("no stated maximum").contains("states no edm.max_taper_deg"));
    let narrow = FIXTURE.replace("\"min\": -5.0, \"max\": 5.0", "\"min\": -0.5, \"max\": 0.5");
    let (world, made) = punch(&narrow, part(), &[H, 0.0], wire_op(Some(arc())));
    made.expect("the cut itself generates");
    let err = post(&world, "uv").expect_err("past the U stroke");
    assert!(err.contains("Punch1") && err.contains("outside the U stroke -0.500..0.500"), "{err}");
    // One profile picked; a taper stated beside the two profiles; planes
    // that are not the profiles' heights.
    let (_, made) = punch(FIXTURE, part(), &[H], wire_op(Some(arc())));
    assert!(made.expect_err("one profile").contains("pairs two profiles, and 1 are picked"));
    let (_, made) = punch(FIXTURE, part(), &[H, 0.0], WireContourOp { taper_deg: 2.0, ..wire_op(Some(arc())) });
    assert!(made.expect_err("both stated").contains("a 4-axis cut and a 2.000 degree taper are both stated"));
    let (_, made) = punch(FIXTURE, part(), &[H, 0.0], WireContourOp { top_z: Height::fixed(10.0), ..wire_op(Some(arc())) });
    let err = made.expect_err("the top plane under the upper profile");
    assert!(err.contains("the upper profile lies at Z12.000 and the workpiece's top plane at Z10.000"), "{err}");
}

/// M3's review of PR #1181, items 4 and 5: a 4-axis cut asked with a
/// no-core burnout is refused at generation by name, and a 4-axis cut
/// posted on a machine that lacks `taper_on`, `taper_off` or `uv` is
/// refused by the wire post by name - it never invents an EDM code.
#[test]
fn a_burnout_or_a_machine_missing_a_taper_code_refuses_the_4_axis_cut_by_name() {
    let part = || loft(&square(8.0), &circle(9.6));
    let (_, made) = punch(FIXTURE, part(), &[H, 0.0], WireContourOp { burnout: Some(1.0), ..wire_op(Some(arc())) });
    let err = made.expect_err("a 4-axis cut with a burnout");
    assert!(err.contains("a 4-axis cut is asked with a no-core burnout - the burnout is cut vertical only"), "{err}");
    let off = "\"taper_off\": \"FX-TAPER-OFF\"";
    let (on, uv) = (FIXTURE.replace("\"taper_on\": \"FX-TAPER-ON\",", ""), FIXTURE.replace(&format!("{off},"), off));
    for (code, text) in [("taper_on", on), ("taper_off", FIXTURE.replace(&format!("{off},"), "")), ("uv", uv.replace("\"uv\": \"U{u} V{v}\"", ""))] {
        assert_ne!(text, FIXTURE, "{code}: the fixture states it");
        let (world, made) = punch(&text, part(), &[H, 0.0], wire_op(Some(arc())));
        made.expect("the cut generates; the codes are the post's");
        let err = post(&world, code).expect_err("a missing taper code never posts");
        assert!(err.contains("operation Punch1: is a 4-axis cut") && err.contains(&format!("states no edm.codes.{code}")), "{code}: {err}");
    }
}

/// M3's review of PR #1181: a 4-axis cut asked with tabs is refused at
/// generation by name - the ruled generator does not cut tabs.
#[test]
fn tabs_beside_a_4_axis_cut_are_refused_by_name() {
    let tabs = WireTabs { count: Some(1), width: Some(2.0), at: vec![[4.0, 0.0]], cutoff_condition: String::new() };
    let (_, made) = punch(FIXTURE, loft(&square(8.0), &circle(9.6)), &[H, 0.0], WireContourOp { tabs: Some(tabs), ..wire_op(Some(arc())) });
    let err = made.expect_err("a 4-axis cut with tabs");
    assert!(err.contains("a 4-axis cut is asked with tabs - tabs are cut on a one-profile contour only"), "{err}");
}

/// Done means: a ruling whose kerf burns into the part blocks the post -
/// the wire laws read the ruled kerf at every judged height.
#[test]
fn a_ruling_that_gouges_the_part_blocks_the_post() {
    // The two end faces of a square post under a round boss: the ruling
    // from the square's corner to the boss cuts the post's shoulder.
    let (world, made) = punch(FIXTURE, stepped(4.0, 3.9), &[H, 0.0], wire_op(Some(arc())));
    made.expect("the generator reads the two profiles only");
    let err = post(&world, "gouge").expect_err("a gouging ruling never posts");
    assert!(err.starts_with("post blocked") && err.contains("Punch1: wire gouge"), "{err}");
}

/// The 4-axis block rides the project file and the JSON door; a file
/// without it reads as the one-profile cut, and its rule is never
/// defaulted.
#[test]
fn the_four_axis_block_round_trips_and_its_rule_is_never_defaulted() {
    let stated = OpParams::WireContour(wire_op(Some(four(
        WirePairingRule::SyncPoints,
        &[(Some((1.0, 2.0)), Some((3.0, 4.0))), (Some((5.0, 6.0)), None)],
    ))));
    let json = project::params_to_json(&stated);
    assert_eq!(json["wire_contour"]["four_axis"]["pairing"], "sync_points");
    assert_eq!(project::params_from_json(&json).expect("reads back"), stated);
    let plain = project::params_to_json(&OpParams::WireContour(wire_op(None)));
    assert!(plain["wire_contour"].get("four_axis").is_none(), "a one-profile cut writes no 4-axis key: {plain}");
    let mut ruleless = json;
    ruleless["wire_contour"]["four_axis"].as_object_mut().expect("fields").remove("pairing");
    let err = project::params_from_json(&ruleless).expect_err("the rule is stated, never defaulted");
    assert!(err.contains("pairing"), "{err}");
    // The stamp reads the block: a changed tolerance is a stale path,
    // and the edit is a command (undoable).
    let (world, made) = punch(FIXTURE, frustum().0, &[H, 0.0], wire_op(Some(arc())));
    made.expect("generates");
    let status = |w: &World| operation::status_of(w, &model::find_by_name(w, "Punch1").expect("op"));
    assert_eq!(status(&world), OpStatus::Ok);
    let mut h = History::default();
    let looser = WireFourAxis { tolerance: 0.08, ..arc() };
    h.execute(&world, EditOperationParams::new(&world, "Punch1", OpParams::WireContour(wire_op(Some(looser)))).expect("edit"));
    assert_ne!(status(&world), OpStatus::Ok, "the stamp reads the 4-axis block");
    assert!(h.undo(&world));
    assert_eq!(status(&world), OpStatus::Ok);
    let wild = WireFourAxis { tolerance: f64::NAN, ..arc() };
    assert!(EditOperationParams::new(&world, "Punch1", OpParams::WireContour(wire_op(Some(wild)))).is_err(), "refused at the door");
}
