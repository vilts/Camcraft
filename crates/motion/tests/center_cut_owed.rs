//! M11.3 owed (RECOMMENDED "M11.3 owed: the centre-cutting law on every
//! plunge" (5)): the law reaches the kinds that carry no approach. A
//! tool that is not centre-cutting never feeds axially into solid on a
//! 3-D finish's descents, a facer's entry, or a drill cycle - it is
//! refused by name instead; the same op is clean with a centre-cutting
//! tool, or where the entry falls through air (off the stock). A facer
//! whose entry lands on the stock runs it on along its first row out to
//! air (M11.3 owed (4)); it is refused only when no air is in reach.
//!
//! Gate map (M11.3): covers camcraft_motion::plunge_law (Solid,
//! first_solid_plunge, audit), camcraft_motion::finishing3d (center_cut),
//! camcraft_motion::machining2d (face, face_entry),camcraft_motion::drilling (cycle,
//! cross).

use camcraft_motion::approach::CENTER_CUT_REFUSAL;
use camcraft_motion::cl::Toolpath;
use camcraft_motion::drilling::{drill, CycleChoice, CycleParams, Hole};
use camcraft_motion::finishing3d::{center_cut_guard, contour, parallel, waterline, Finish3DParams, FinishPattern};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::cl::MoveKind;
use camcraft_motion::machining2d::{air_lead, face, face_led, FaceParams, GenError, AIR_MARGIN};
use camcraft_motion::plunge_law::{audit, first_solid_plunge, Solid};
use camcraft_motion::tools::{Tool, ToolKind};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let l = Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]);
    Region::new(vec![l.expect("loop")]).expect("region")
}

fn refused(what: &str, e: &GenError) {
    assert!(format!("{e:?}").contains(CENTER_CUT_REFUSAL), "{what}: refused, but not by the law: {e:?}");
}

// ------------------------------------------------------------- 3-D finish

/// A 20 x 20 x 10 box, z in 0..10.
fn box_facets() -> Vec<Triangle> {
    let q = |sx: f64, sy: f64, z: f64| Point3::new(sx * 10.0, sy * 10.0, z);
    let (a, b, c, d) = (q(-1.0, -1.0, 0.0), q(1.0, -1.0, 0.0), q(1.0, 1.0, 0.0), q(-1.0, 1.0, 0.0));
    let (e, f, g, h) = (q(-1.0, -1.0, 10.0), q(1.0, -1.0, 10.0), q(1.0, 1.0, 10.0), q(-1.0, 1.0, 10.0));
    let quad = |p: [Point3; 4]| [Triangle::new(p[0], p[1], p[2]), Triangle::new(p[0], p[2], p[3])];
    let mut t = Vec::new();
    for qd in [[e, f, g, h], [a, c, b, d], [a, b, f, e], [b, c, g, f], [c, d, h, g], [d, a, e, h]] {
        t.extend(quad(qd));
    }
    t
}

fn fparams(pattern: FinishPattern) -> Finish3DParams {
    Finish3DParams {
        pattern,
        stepover: None,
        tool_diameter: 6.0,
        tolerance: 0.1,
        clearance_z: 25.0,
        retract_z: None,
        top_z: 10.0,
        bottom_z: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
    }
}

fn run(f: &[Triangle], p: &Finish3DParams) -> Result<Toolpath, GenError> {
    match p.pattern {
        FinishPattern::Waterline => waterline(f, p),
        FinishPattern::Contour => contour(f, p),
        _ => parallel(f, p),
    }
}

#[test]
fn a_finish_never_plunges_a_tool_that_is_not_centre_cutting_into_solid() {
    let f = box_facets();
    // The stock: wider than the part, its top the part's top.
    let stock = rect(-30.0, -30.0, 30.0, 30.0);
    let mut convicted = 0;
    for pattern in [FinishPattern::Waterline, FinishPattern::Parallel, FinishPattern::Contour] {
        let p = fparams(pattern);
        let what = format!("{pattern:?}");
        for solid in [Solid { top: 10.0, footprint: None }, Solid { top: 10.0, footprint: Some(&stock) }] {
            // The centre-cutting tool's path is the generator's own, byte for byte.
            let cc = center_cut_guard(true, &solid, &p, |q| run(&f, q)).expect("a centre-cutting finish");
            assert_eq!(cc, run(&f, &p).expect("finish"), "{what}");
            let plunges = first_solid_plunge(&cc, 3.0, &solid).is_some();
            convicted += usize::from(plunges);
            match center_cut_guard(false, &solid, &p, |q| run(&f, q)) {
                Ok(t) => {
                    assert_eq!(first_solid_plunge(&t, 3.0, &solid), None, "{what}: {:?}", t.notes);
                    if plunges {
                        assert!(t.notes.iter().any(|n| n.contains("ramp instead")), "{what}: {:?}", t.notes);
                    }
                }
                Err(e) => {
                    assert!(plunges, "{what}: refused a path that never plunged: {e:?}");
                    refused(&what, &e);
                }
            }
        }
    }
    assert!(convicted > 0, "the waterline's ring entries plunge into the stock around the box");
}

#[test]
fn a_finish_whose_entries_fall_through_air_passes_untouched() {
    // The stock stands somewhere else entirely: every descent is in air.
    let f = box_facets();
    let away = rect(100.0, 100.0, 120.0, 120.0);
    let p = fparams(FinishPattern::Waterline);
    for solid in [Solid { top: 10.0, footprint: Some(&away) }, Solid { top: 1.0, footprint: None }] {
        let t = center_cut_guard(false, &solid, &p, |q| run(&f, q)).expect("air entries pass");
        assert_eq!(t, run(&f, &p).expect("finish"));
    }
}

// ------------------------------------------------------------------- face

fn face_p(top_z: Option<f64>) -> FaceParams {
    FaceParams { holes: Default::default(),
        tool_diameter: 10.0,
        stepover: 6.0,
        clearance_z: 25.0,
        cut_z: 18.0,
        top_z,
        stepdown: top_z.map(|_| 1.0),
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false,
    }
}

#[test]
fn the_facer_enters_from_outside_the_stock_so_its_plunge_is_air() {
    // Facing the stock's own footprint: the entry stands a tool radius
    // off its edge - air, single level and staged, either direction.
    let stock = rect(0.0, 0.0, 60.0, 40.0);
    let solid = Solid { top: 20.0, footprint: Some(&stock) };
    for top in [None, Some(20.0)] {
        for climb in [true, false] {
            let t = face(&stock, &FaceParams { climb, ..face_p(top) }).expect("face");
            assert_eq!(audit(&t, false, 5.0, &solid, "the facing entry"), Ok(()), "{top:?} {climb}");
            assert_eq!(air_lead(&t, 5.0, &solid, &[]), Ok(None), "nothing to run on: {top:?} {climb}");
        }
    }
}

#[test]
fn a_facer_entry_over_the_stock_runs_on_along_its_row_out_to_air() {
    // Facing only an inner patch of a wider stock: the plain entry lands
    // on the stock (refused for a non-centre-cutting tool); the run-on
    // moves the plunge back along the first row until the disc clears
    // the stock by the margin, and the row feeds in from there.
    let patch = rect(10.0, 10.0, 50.0, 30.0);
    let stock = rect(0.0, 0.0, 60.0, 40.0);
    let solid = Solid { top: 20.0, footprint: Some(&stock) };
    for top in [None, Some(20.0)] {
        for climb in [true, false] {
            let p = FaceParams { climb, ..face_p(top) };
            let plain = face(&patch, &p).expect("face");
            refused("face patch", &audit(&plain, false, 5.0, &solid, "the facing entry").expect_err("onto the stock"));
            assert_eq!(audit(&plain, true, 5.0, &solid, "the facing entry"), Ok(()));
            // The centre-cutting path is the facer's own, byte for byte.
            assert_eq!(face_led(&patch, &p, 0.0).expect("face"), plain, "{top:?} {climb}");

            let lead = air_lead(&plain, 5.0, &solid, &[]).expect("air beside the stock").expect("a run-on");
            let led = face_led(&patch, &p, lead).expect("led face");
            assert_eq!(audit(&led, false, 5.0, &solid, "the facing entry"), Ok(()), "{top:?} {climb}");
            assert_eq!(first_solid_plunge(&led, 5.0, &solid), None);
            // Every plunge stands clear of the stock by the margin, on
            // the first row's line, within a sane run-on.
            for m in led.moves.iter().filter(|m| m.kind == MoveKind::Plunge) {
                let gap = if climb { -m.to.x } else { m.to.x - 60.0 };
                assert!(gap >= 5.0 + AIR_MARGIN - 1e-9, "{top:?} {climb}: plunge at {:?}", m.to);
                assert!(gap <= 5.0 + AIR_MARGIN + 5.0, "a sane run-on, not a wander: {:?}", m.to);
                assert!((m.to.y - 15.0).abs() < 1e-9, "on the first row: {:?}", m.to);
            }
            // The run-on rides the first row's own feed: no extra moves.
            assert_eq!(led.moves.len(), plain.moves.len(), "{top:?} {climb}");
        }
    }
}

#[test]
fn a_facer_entry_that_cannot_reach_air_is_still_refused_by_name() {
    let patch = rect(10.0, 10.0, 50.0, 30.0);
    let stock = rect(0.0, 0.0, 60.0, 40.0);
    let plain = face(&patch, &face_p(None)).expect("face");
    // Unseen stock is solid everywhere: no point beside it is air.
    let blind = Solid { top: 20.0, footprint: None };
    assert!(air_lead(&plain, 5.0, &blind, &[]).expect_err("no air").contains("unknown"));
    // A forbidden zone (an exclude boundary) stands on the run-on line.
    let solid = Solid { top: 20.0, footprint: Some(&stock) };
    let keep_out = rect(-3.0, 12.0, -1.0, 18.0);
    let why = air_lead(&plain, 5.0, &solid, &[("an exclude boundary", &keep_out)]).expect_err("blocked");
    assert!(why.contains("an exclude boundary"), "{why}");
    // A zone well off the line (clear of the swept disc) is no bar.
    let aside = rect(-8.0, 26.0, -1.0, 30.0);
    assert!(air_lead(&plain, 5.0, &solid, &[("an exclude boundary", &aside)]).is_ok_and(|l| l.is_some()));
    // A bad lead never generates.
    for bad in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(face_led(&patch, &face_p(None), bad).is_err(), "{bad}");
    }
}

// --------------------------------------------------------------- drilling

fn tool(kind: ToolKind, center_cutting: bool) -> Tool {
    Tool {
        kind,
        diameter: 6.0,
        corner_r: 0.0,
        tip_angle: 118.0,
        pitch: 1.0,
        nose_radius: 0.0,
        width: 0.0,
        flute_len: 40.0,
        shank_d: 6.0,
        shank_len: 40.0,
        holder_d: 44.0,
        holder_len: 35.0,
        material_group: "aluminum".into(),
        center_cutting,
        rpm: 1000.0,
        feed: 120.0,
        tool_material: None,
        flutes: None,
        orientation: None,
        turning: None,
    }
}

fn holes() -> Vec<Hole> {
    vec![Hole { xy: Point2::new(10.0, 10.0), radius: 3.5, entry_z: 10.0, floor_z: 0.0, through: false, stepped: false }]
}

fn cycle(t: &Tool, cycle: CycleChoice) -> Result<Toolpath, GenError> {
    let hs = holes();
    drill(&CycleParams {
        clearance_z: 25.0,
        r_z: 12.0,
        hop_z: None,
        bottom_z: None,
        cycle,
        peck: 0.0,
        dwell: 0.0,
        feed: 100.0,
        plunge_feed: 80.0,
        tool: t,
        holes: &hs,
    })
}

#[test]
fn a_drill_cycle_refuses_a_tool_that_is_not_centre_cutting() {
    for choice in [CycleChoice::Auto, CycleChoice::Spot, CycleChoice::Drill, CycleChoice::Peck] {
        let e = cycle(&tool(ToolKind::Drill, false), choice).expect_err("a drill plunge into solid");
        refused(&format!("{choice:?}"), &e);
        cycle(&tool(ToolKind::Drill, true), choice).expect("a centre-cutting drill");
    }
}

#[test]
fn tap_and_bore_cycles_read_the_hole_as_pre_drilled() {
    for (kind, choice) in [(ToolKind::Tap, CycleChoice::Tap), (ToolKind::Bore, CycleChoice::Bore), (ToolKind::Bore, CycleChoice::Auto)] {
        let nc = cycle(&tool(kind, false), choice).expect("a pre-drilled hole passes");
        assert_eq!(nc, cycle(&tool(kind, true), choice).expect("cc"), "{kind:?} {choice:?}");
    }
}
