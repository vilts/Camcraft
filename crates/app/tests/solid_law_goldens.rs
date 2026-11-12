//! Not noisy: every committed 2D golden program whose kind answers to a
//! depth law (contour, face, pocket - the model law with its solid law
//! b2; drill, peck, tap - containment's solid law) is regenerated, proved
//! byte-identical to its golden, and verified against the finished part
//! it was cut for, modelled as an import would hand it over (round
//! holes as inscribed 32/128-gons). The pinned verdict is what the
//! per-plane verifier gave before the solid law, warning for warning:
//! the solid law adds none - and since #862 that verdict is clean for
//! every one of them, and every posted `.nc` golden's program must be
//! one the gate passes (the guard below); the crossing refusal the two
//! disc-hole goldens used to carry is pinned on its own fixture.
//! (Slot and trace cut their source - no depth law runs on them; since
//! #1000 they, the trochoidal slot and the plasma plate are judged by
//! the gate their kind's post runs, `posted_gates`. The 3D
//! finish goldens verify as `Mode::Envelope`, where the solid law
//! stands down by mode gate - their verdict path is untouched; the
//! arc-planes golden is a hand-built posting fixture with no part.)
//!
//! Provenance: our own harness over our own goldens; no vendor material.
//!
//! Gate map (solid law): covers sim::verify (engine b2, solid_hits, the
//! crossing law), the posted goldens under tests/golden/nc/,
//! motion::machining2d (contour, face, pocket, slot, trochoid, trace),
//! motion::drilling (drill), app::operation (region_of), sim::verify::plasma.

mod common;
mod posted_gates;

use common::gen_to_cltext;
use posted_gates::{narrowed_slot_verdict, plasma_findings, region_verdicts};
use common::solid_part::{cap, circle, mill, plate, plate_cell, rect, region, walls};

use camcraft_app::cl::Toolpath;
use camcraft_app::drilling::{CycleChoice, CycleParams, Hole, drill};
use camcraft_app::geo2d::{Point2, Region};
use camcraft_app::geometry::Triangle;
use camcraft_app::machining2d::{ContourParams, FaceParams, Lead, PocketParams, PocketStrategy, Side, contour, face, pocket};
use camcraft_app::tools::{Tool, ToolKind};
use camcraft_app::verify::{Cutter, HoleAxis, Mode, SolidLaw, ToolGeom, VerifyContext, Warning, solid_hits_walled, verify};

fn golden(name: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name);
    std::fs::read_to_string(&p).expect("golden").replace("\r\n", "\n")
}

fn cx<'a>(region: &'a Region, model: Option<&'a [Triangle]>, tool: &'a ToolGeom, mode: Mode, top: f64, clear: f64) -> VerifyContext<'a> {
    VerifyContext {
        region,
        stock: None,
        model,
        tool,
        cutter: None,
        mode,
        stock_top_z: top,
        clearance_z: clear,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    }
}

/// The model kinds' verdict, as the operation layer reads it.
fn section_law(t: &Toolpath, region: &Region, part: &[Triangle], tool: &ToolGeom, mode: Mode, top: f64, clear: f64) -> Vec<Warning> {
    verify(t, &cx(region, Some(part), tool, mode, top, clear))
}

/// The containment kinds' depth verdict: the solid law with the tool's
/// own tip, the round walls of the holes it drills excused (their region
/// laws read the exact rims, untouched by the solid law and not judged
/// here; #1205 measures each wall about its hole's own axis).
fn contain_law(t: &Toolpath, part: &[Triangle], tool: &Tool, holes: &[Hole]) -> Vec<Warning> {
    let law = SolidLaw { step: 0.25, allow: 0.01, hole_walls: true };
    let walls: Vec<HoleAxis> = holes.iter().map(|h| HoleAxis::vertical(h.xy.x, h.xy.y)).collect();
    solid_hits_walled(t, part, &Cutter::of(tool), &law, &walls)
}

fn contour_params(d: f64, lead: Lead, side: Side) -> ContourParams {
    ContourParams {
        tool_diameter: d,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side,
        lead,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    }
}

/// The 40 x 20 plate with the R6 bore (128-gon) through it, as a
/// region and as the finished part.
fn disc_hole_plate() -> (Region, Vec<Triangle>) {
    let bore6 = circle((20.0, 10.0), 6.0, 128);
    let mut part = Vec::new();
    plate(&rect(0.0, 0.0, 40.0, 20.0), Some((&bore6, -4.0)), -4.0, 0.0, &mut part);
    (region(&[rect(0.0, 0.0, 40.0, 20.0), bore6]), part)
}

/// (golden, verdict) for every depth-law golden.
fn verdicts() -> Vec<(&'static str, Vec<Warning>)> {
    let mut out = Vec::new();
    let mut check = |name: &'static str, t: &Toolpath, w: Vec<Warning>| {
        assert_eq!(gen_to_cltext(t), golden(name), "{name}: the regenerated program is the golden");
        out.push((name, w));
    };
    // The plate with a round bore (contour_disc_hole, tangential lead),
    // 2 mm leads that stay clear of the R6 bore's r = 3 ring (#862).
    let (reg, part) = disc_hole_plate();
    let g = mill(6.0);
    for (name, lead) in [("contour_disc_hole.cl", Lead::Arc { radius: 2.0 }), ("contour_tangential_lead.cl", Lead::Tangential { radius: 2.0 })] {
        let t = contour(&reg, &contour_params(6.0, lead, Side::Auto)).expect("contour");
        check(name, &t, section_law(&t, &reg, &part, &g, Mode::Boundary, 0.0, 5.0));
    }
    // The plate with a square hole, sharp and blended.
    let sq = rect(16.0, 6.0, 24.0, 14.0);
    let mut part = Vec::new();
    plate(&rect(0.0, 0.0, 40.0, 20.0), Some((&sq, -4.0)), -4.0, 0.0, &mut part);
    let reg = region(&[rect(0.0, 0.0, 40.0, 20.0), sq.clone()]);
    for (name, blend) in [("contour_plate_with_hole.cl", false), ("contour_plate_with_hole_blended.cl", true)] {
        let t = contour(&reg, &ContourParams { blend_turns: blend, ..contour_params(6.0, Lead::Arc { radius: 3.0 }, Side::Auto) }).expect("contour");
        check(name, &t, section_law(&t, &reg, &part, &g, Mode::Boundary, 0.0, 5.0));
    }
    // The R12 bore walked from the inside, arc entry.
    let bore12 = circle((20.0, 10.0), 12.0, 128);
    let mut part = Vec::new();
    plate(&rect(0.0, -10.0, 40.0, 30.0), Some((&bore12, -4.0)), -4.0, 0.0, &mut part);
    let reg = region(std::slice::from_ref(&bore12));
    let t = contour(&reg, &ContourParams { entry: camcraft_app::machining3d::Entry3D::Arc, ..contour_params(6.0, Lead::Arc { radius: 3.0 }, Side::Inside) })
        .expect("contour");
    check("contour_bore_arc_entry.cl", &t, section_law(&t, &reg, &part, &g, Mode::Boundary, 0.0, 5.0));
    // The faced 40 x 40 block, finished at Z -2.
    let sq40 = rect(0.0, 0.0, 40.0, 40.0);
    let mut part = Vec::new();
    plate(&sq40, None, -12.0, -2.0, &mut part);
    let reg = region(std::slice::from_ref(&sq40));
    let g10 = mill(10.0);
    for (name, blend, top, step) in [
        ("face_square_40x40.cl", false, None, None),
        ("face_square_40x40_blended.cl", true, None, None),
        ("face_square_40x40_stepped.cl", false, Some(2.0), Some(1.5)),
    ] {
        let p = FaceParams { holes: Default::default(), tool_diameter: 10.0, stepover: 8.0, clearance_z: 5.0, cut_z: -2.0, top_z: top, stepdown: step, feed: 500.0, plunge_feed: 100.0, climb: true, blend_turns: blend };
        let t = face(&reg, &p).expect("face");
        check(name, &t, section_law(&t, &reg, &part, &g10, Mode::Boundary, top.unwrap_or(-2.0), 5.0));
    }
    // The 30 x 30 pocket to Z -4 around a 10 x 10 island standing to Z 0.
    let (outer, pk, isl) = (rect(-10.0, -10.0, 40.0, 40.0), rect(0.0, 0.0, 30.0, 30.0), rect(10.0, 10.0, 20.0, 20.0));
    let mut part = Vec::new();
    walls(&outer, -10.0, 0.0, true, &mut part);
    cap(&outer, None, -10.0, false, &mut part);
    cap(&outer, Some(&pk), 0.0, true, &mut part);
    walls(&pk, -4.0, 0.0, false, &mut part);
    cap(&pk, Some(&isl), -4.0, true, &mut part);
    walls(&isl, -4.0, 0.0, true, &mut part);
    cap(&isl, None, 0.0, true, &mut part);
    let reg = region(&[pk.clone(), isl.clone()]);
    let g8 = mill(8.0);
    for (name, strategy, blend) in [
        ("pocket_zigzag_plate_with_island.cl", PocketStrategy::Zigzag, false),
        ("pocket_zigzag_plate_with_island_blended.cl", PocketStrategy::Zigzag, true),
        ("pocket_waterline_plate_with_island.cl", PocketStrategy::Waterline, false),
    ] {
        let p = PocketParams {
            links: camcraft_app::links::Links::PLANAR,
            tool_diameter: 8.0,
            stepover: 3.0,
            stock_to_leave: 0.0,
            clearance_z: 5.0,
            retract_z: None,
            top_z: 0.0,
            bottom_z: -4.0,
            stepdown: 2.0,
            feed: 400.0,
            plunge_feed: 80.0,
            climb: true,
            strategy,
            entry: camcraft_app::machining3d::Entry3D::Plunge,
            helix_radius: 1.0,
            optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
            blend_turns: blend,
        };
        let t = pocket(&reg, &p).expect("pocket");
        check(name, &t, section_law(&t, &reg, &part, &g8, Mode::Area, 0.0, 5.0));
    }
    // The drilling goldens: their plates, holes inscribed 32-gons.
    let tool = |kind: ToolKind, d: f64, tip: f64, pitch: f64| Tool {
        kind,
        diameter: d,
        corner_r: 0.0,
        tip_angle: tip,
        pitch,
        nose_radius: 0.0,
        width: 0.0,
        flute_len: 40.0,
        shank_d: d.min(10.0),
        shank_len: 30.0,
        holder_d: 20.0,
        holder_len: 40.0,
        material_group: String::from("steel"),
        rpm: 500.0,
        feed: 200.0,
        tool_material: None,
        flutes: None,
        center_cutting: true,
        orientation: None,
        turning: None,
    };
    let hole = |x: f64, y: f64, r: f64, floor: f64, through: bool| Hole { xy: Point2::new(x, y), radius: r, entry_z: 0.0, floor_z: floor, through, stepped: false };
    let cycle = |holes: &[Hole], tool: &Tool, clear: f64| {
        drill(&CycleParams { clearance_z: clear, r_z: 0.0, hop_z: None, bottom_z: None, cycle: CycleChoice::Auto, peck: 0.0, dwell: 0.0, feed: 200.0, plunge_feed: 60.0, tool, holes })
            .expect("drill")
    };
    let dr6 = tool(ToolKind::Drill, 6.0, 118.0, 0.0);
    let (ha, hb) = (circle((10.0, 10.0), 3.0, 32), circle((30.0, 10.0), 3.0, 32));
    let mut part = Vec::new();
    walls(&rect(0.0, 0.0, 40.0, 20.0), -4.0, 0.0, true, &mut part);
    plate_cell(&rect(0.0, 0.0, 20.0, 20.0), Some((&ha, -4.0)), -4.0, 0.0, &mut part);
    plate_cell(&rect(20.0, 0.0, 40.0, 20.0), Some((&hb, -3.0)), -4.0, 0.0, &mut part);
    let holes = [hole(10.0, 10.0, 3.0, -4.0, true), hole(30.0, 10.0, 3.0, -3.0, false)];
    let t = cycle(&holes, &dr6, 5.0);
    check("drill_plate.cl", &t, contain_law(&t, &part, &dr6, &holes));
    let hp = circle((20.0, 10.0), 3.0, 32);
    let mut part = Vec::new();
    plate(&rect(0.0, 0.0, 40.0, 20.0), Some((&hp, -23.0)), -23.0, 0.0, &mut part);
    let holes = [hole(20.0, 10.0, 3.0, -23.0, true)];
    let t = cycle(&holes, &dr6, 6.0);
    check("drill_peck.cl", &t, contain_law(&t, &part, &dr6, &holes));
    // The tapped hole: the finished part's thread stands at the tap's
    // own diameter (the golden's 2.5 mm hole record is the tap DRILL's).
    let tap = tool(ToolKind::Tap, 10.0, 0.0, 1.5);
    let ht = circle((15.0, 15.0), 5.0, 32);
    let mut part = Vec::new();
    plate(&rect(0.0, 0.0, 30.0, 30.0), Some((&ht, -12.0)), -20.0, 0.0, &mut part);
    let holes = [hole(15.0, 15.0, 2.5, -12.0, false)];
    let t = cycle(&holes, &tap, 5.0);
    check("tap_plate.cl", &t, contain_law(&t, &part, &tap, &holes));
    out
}

#[test]
fn every_depth_law_golden_verifies_exactly_as_before() {
    let got = verdicts();
    for (name, w) in &got {
        eprintln!("{name}: {w:?}");
    }
    assert_eq!(got.len(), 14, "every depth-law golden is judged");
    // The verifier's verdict on these programs, recorded at the base
    // before the solid law - and unchanged by it: every golden passes
    // the gate (#862 moved the disc-hole leads off the ring; the
    // refusal they used to carry is pinned below on its own fixture).
    for (name, w) in got {
        assert_eq!(w, Vec::new(), "{name}");
    }
}

#[test]
fn a_lead_on_the_rings_own_circle_is_refused() {
    // Law (a), the XY crossing law the solid law never touches. The R6
    // bore's r = 3 cutter-centre ring and 3 mm leads share one circle,
    // so the leads cross the ring and the program is refused. Since
    // #689 the ring closes on arcs and the lead-out leaves on its true
    // tangent, so the leads meet the ring's arcs as well as each other.
    // These were the contour_disc_hole / contour_tangential_lead
    // goldens' programs until #862; no golden is kept for them, since
    // no user can post them.
    let (reg, part) = disc_hole_plate();
    let g = mill(6.0);
    let crossing = |pairs: &[[usize; 2]]| pairs.iter().map(|&moves| Warning::Crossing { moves }).collect::<Vec<_>>();
    for (lead, want) in [
        (Lead::Arc { radius: 3.0 }, crossing(&[[17, 20], [18, 21], [40, 43], [41, 44], [63, 66], [64, 67]])),
        (
            Lead::Tangential { radius: 3.0 },
            crossing(&[
                [17, 19], [17, 20], [18, 21], [19, 21],
                [40, 42], [40, 43], [41, 44], [42, 44],
                [63, 65], [63, 66], [64, 67], [65, 67],
            ]),
        ),
    ] {
        let t = contour(&reg, &contour_params(6.0, lead, Side::Auto)).expect("contour");
        assert_eq!(section_law(&t, &reg, &part, &g, Mode::Boundary, 0.0, 5.0), want, "{lead:?}");
    }
}

/// The first line a posted golden of a gate-refused program must carry
/// (#862 option (c)); none does today.
const REFUSED_HEADER: &str = "refused program, posted for syntax conformance";

/// Posted goldens whose program no gate judges, each with why: the
/// guard below makes every other posted golden's program pass the gate
/// its kind's post runs, so a new one is either judged or named here.
/// The slot, trochoid, trace and plasma goldens are judged since #1000
/// (`posted_gates`).
const UNJUDGED: &[(&str, &str)] = &[(
    "arc_planes",
    "a hand-built posting fixture (arc_planes.rs): no operation kind, part, region or tool record makes it, \
     so no gate a user's post runs can judge it - #1093",
)];

#[test]
fn the_slot_trace_and_plasma_goldens_pass_their_gates() {
    // #1000: the posted goldens no depth law judges, each through the
    // gate its kind's post runs - the region laws for the slots and the
    // trace, the plasma laws for the plate: every one verifies clean.
    let got = region_verdicts();
    assert_eq!(got.len(), 4, "every region-law golden is judged");
    for (name, w) in got {
        assert_eq!(w, Vec::new(), "{name}");
    }
    assert_eq!(plasma_findings(), Vec::new(), "plasma_plate_with_holes.cl: the plasma laws find nothing");
}

#[test]
fn the_slot_gate_bites() {
    // The harness is not vacuous: the wide-channel slot's program judged
    // against a stadium 2 mm narrower than the one it was cut for gouges.
    let got = narrowed_slot_verdict();
    assert!(got.iter().any(|w| matches!(w, Warning::Gouge { .. })), "{got:?}");
}

#[test]
fn every_posted_golden_is_a_program_the_gate_passes() {
    // #862: a posted golden reads as "this output is right", so its
    // program must be one a user can post. Every `.nc` under golden/nc/
    // (the Fanuc top and each dialect's directory) whose stem is a `.cl`
    // golden is judged here.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    // Every blocking finding of each judged program: the depth laws',
    // the region laws' (slot, trochoid, trace) and the plasma laws'
    // refusals (its warnings ride the verdict line, never block).
    let blocking = |w: Vec<Warning>| w.iter().map(|w| format!("{w:?}")).collect::<Vec<_>>();
    let mut judged: Vec<(&str, Vec<String>)> = verdicts().into_iter().chain(region_verdicts()).map(|(n, w)| (n, blocking(w))).collect();
    let plasma = plasma_findings().iter().filter(|f| f.refuses()).map(|f| f.describe()).collect();
    judged.push(("plasma_plate_with_holes.cl", plasma));
    let mut dirs = vec![root.join("nc")];
    let mut posted = 0;
    while let Some(dir) = dirs.pop() {
        for e in std::fs::read_dir(&dir).expect("nc dir").map(|e| e.expect("entry").path()) {
            if e.is_dir() {
                dirs.push(e);
                continue;
            }
            let Some(stem) = e.file_stem().and_then(|s| s.to_str()).filter(|_| e.extension().is_some_and(|x| x == "nc")) else { continue };
            if !root.join(format!("{stem}.cl")).is_file() {
                continue; // posted from a document or its own stream, not a .cl golden
            }
            posted += 1;
            let first = std::fs::read_to_string(&e).expect("nc").lines().next().unwrap_or_default().to_owned();
            if first.contains(REFUSED_HEADER) {
                continue;
            }
            let cl = format!("{stem}.cl");
            match judged.iter().find(|(n, _)| *n == cl) {
                Some((_, w)) => assert!(w.is_empty(), "{}: posted, but the gate refuses its program: {w:?}", e.display()),
                None => assert!(
                    UNJUDGED.iter().any(|(n, _)| *n == stem),
                    "{}: posted, but no gate judges its program - judge it in verdicts() / posted_gates or name it in UNJUDGED",
                    e.display()
                ),
            }
        }
    }
    assert!(posted > 50, "the guard saw only {posted} posted goldens");
}

