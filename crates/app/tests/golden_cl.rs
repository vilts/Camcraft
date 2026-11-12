//! Committed golden CL dumps for the 2D generators - the AGENTS.md
//! testing rule made concrete. Format, naming and the regeneration
//! workflow: tests/golden/CLAUDE.md.
//!
//! Gate map (M4.4): covers cl::cl, motion::drilling, motion::finishing3d, geometry::geo2d, app::geometry, motion::machining2d, tools::tools.


mod common;

use common::{gen_to_cltext, write_golden};

use camcraft_app::cl::MoveKind;
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::machining2d::{
    contour, face, pocket, slot, trace, ContourParams, FaceParams, Lead, PocketParams,
    PocketStrategy, Side, SlotParams, TraceParams,
};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
    .unwrap()])
    .unwrap()
}

/// 40x20 plate with an 8x8 through hole centered on (20,10).
fn plate_with_hole() -> Region {
    let mut loops = rect(0.0, 0.0, 40.0, 20.0).loops().to_vec();
    loops.extend_from_slice(rect(16.0, 6.0, 24.0, 14.0).loops());
    Region::new(loops).unwrap()
}

/// Two loops through Region::new: outer square with a centered square
/// void (a hole/island, depending on the operation reading it).
fn square_with_center(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let hx0 = (2.0 * x0 + x1) / 3.0;
    let hy0 = (2.0 * y0 + y1) / 3.0;
    let hx1 = (x0 + 2.0 * x1) / 3.0;
    let hy1 = (y0 + 2.0 * y1) / 3.0;
    Region::new(
        vec![
            Loop::new(vec![
                Point2::new(x0, y0),
                Point2::new(x1, y0),
                Point2::new(x1, y1),
                Point2::new(x0, y1),
            ]),
            Loop::new(vec![
                Point2::new(hx0, hy0),
                Point2::new(hx1, hy0),
                Point2::new(hx1, hy1),
                Point2::new(hx0, hy1),
            ]),
        ]
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .unwrap(),
    )
    .unwrap()
}

/// A closed ccw circle about `c`, tessellated into `n` segments - fine
/// enough that the arc fitter sees the circle in it (sagitta
/// `r(1 - cos(pi/n))` well under any sane tolerance), which is exactly
/// what STEP tessellation hands us for a bore.
fn circle(c: Point2, r: f64, n: usize) -> Loop {
    Loop::new(
        (0..n)
            .map(|k| {
                let t = std::f64::consts::TAU * k as f64 / n as f64;
                Point2::new(c.x + r * t.cos(), c.y + r * t.sin())
            })
            .collect(),
    )
    .unwrap()
}

#[test]
fn contour_disc_hole_matches_golden() {
    // A plate with a 128-gon approximation of a round bore: the rim is
    // polygonal only because import tessellates it, so the fitter must
    // replace the chord runs with arc moves - this golden is the v2
    // format's committed witness. The lead is 2 mm (#862): a 3 mm lead
    // on the R6 bore lies on the r = 3 cutter-centre ring itself, so the
    // leads cross the ring and the gate refuses the program - a posted
    // golden must be a program a user can post (`solid_law_goldens`
    // pins both the pass and the R3 refusal).
    let mut loops = rect(0.0, 0.0, 40.0, 20.0).loops().to_vec();
    loops.push(circle(Point2::new(20.0, 10.0), 6.0, 128));
    let region = Region::new(loops).unwrap();
    let p = ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::Arc { radius: 2.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    };
    let t = contour(&region, &p).unwrap();
    assert!(t.has_arcs(), "a fine polygon bore rim must fit to arcs");
    write_golden("contour_disc_hole.cl", &gen_to_cltext(&t));
}

#[test]
fn contour_tangential_lead_matches_golden() {
    // The same plate-and-bore job, tangential entry instead of quarter:
    // this golden is the byte-witness that the half-arc leads post as
    // single S{±1} arc moves with a 2*radius standoff - rerun
    // WRITE_GOLDEN=1 only together with a deliberate law change. 2 mm,
    // not 3, so the leads stay clear of the ring and the gate (#862).
    let mut loops = rect(0.0, 0.0, 40.0, 20.0).loops().to_vec();
    loops.push(circle(Point2::new(20.0, 10.0), 6.0, 128));
    let region = Region::new(loops).unwrap();
    let p = ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::Tangential { radius: 2.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    };
    let t = contour(&region, &p).unwrap();
    assert!(t.has_arcs());
    write_golden("contour_tangential_lead.cl", &gen_to_cltext(&t));
}

#[test]
fn face_square_40x40_matches_golden() {
    // The same plain 40x40 square the row-count tests use, at the
    // default parameters: five rows, rapids only at the ends.
    let p = FaceParams { holes: Default::default(),
        tool_diameter: 10.0,
        stepover: 8.0,
        clearance_z: 5.0,
        cut_z: -2.0,
        top_z: None,
        stepdown: None,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        blend_turns: false,
    };
    let t = face(&rect(0.0, 0.0, 40.0, 40.0), &p).unwrap();
    write_golden("face_square_40x40.cl", &gen_to_cltext(&t));
}

#[test]
fn face_square_40x40_blended_matches_golden() {
    // M11.1 A's byte witness: the same job with turning-point blends on.
    // Line-for-line the sharp file, except that each of the four row
    // reversals' step feed becomes one `Arc ... S±1` semicircle on the
    // same endpoints (stride/2 radius, bulging into the overrun) and the
    // header steps to v2. The sharp golden above stays byte-identical:
    // blends are opt-in.
    let p = FaceParams { holes: Default::default(),
        tool_diameter: 10.0,
        stepover: 8.0,
        clearance_z: 5.0,
        cut_z: -2.0,
        top_z: None,
        stepdown: None,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        blend_turns: true,
    };
    let t = face(&rect(0.0, 0.0, 40.0, 40.0), &p).unwrap();
    assert_eq!(t.arc_count(), 4);
    assert!(t.notes.is_empty(), "{:?}", t.notes);
    write_golden("face_square_40x40_blended.cl", &gen_to_cltext(&t));
}

#[test]
fn face_square_40x40_stepped_matches_golden() {
    // The same square staged from 2.0 down to -2.0 in 1.5 steps: the row
    // band is identical at every level, and each level change lifts to
    // clearance and re-enters above the first row's start.
    let p = FaceParams { holes: Default::default(),
        tool_diameter: 10.0,
        stepover: 8.0,
        clearance_z: 5.0,
        cut_z: -2.0,
        top_z: Some(2.0),
        stepdown: Some(1.5),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        blend_turns: false,
    };
    let t = face(&rect(0.0, 0.0, 40.0, 40.0), &p).unwrap();
    assert_eq!(
        t.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count(),
        4,
        "levels 2.0, 0.5, -1.0, -2.0"
    );
    write_golden("face_square_40x40_stepped.cl", &gen_to_cltext(&t));
}

#[test]
fn contour_plate_with_hole_matches_golden() {
    // 40x20 plate with a 8x8 through hole: contour offsets material
    // away from every loop at each level, so both the outer and the hole
    // boundary must appear. Lead-in arcs included.
    let p = ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::Arc { radius: 3.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    };
    let t = contour(&plate_with_hole(), &p).unwrap();
    write_golden("contour_plate_with_hole.cl", &gen_to_cltext(&t));
}

#[test]
fn pocket_zigzag_plate_with_island_matches_golden() {
    // 30x30 region with a centered 10x10 island; zigzag rows must stop
    // at the island and step over it between levels.
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
        strategy: PocketStrategy::Zigzag,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    };
    let t = pocket(&square_with_center(0.0, 0.0, 30.0, 30.0), &p).unwrap();
    write_golden("pocket_zigzag_plate_with_island.cl", &gen_to_cltext(&t));
}

#[test]
fn contour_plate_with_hole_blended_matches_golden() {
    // M11.1 B's byte witness: the plate-with-hole job with turning-point
    // blends on. The outer ring's corners are round joins already (no
    // line-to-line corner to fillet); the hole's ring turns toward the
    // hole's air at its corners, and each one but the seam (where the
    // lead joins) rides one tangent `Arc` of the tool radius. The sharp
    // golden above stays byte-identical: blends are opt-in.
    let p = ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::Arc { radius: 3.0 },
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: true,
        links: camcraft_app::links::Links::PLANAR,
    };
    let t = contour(&plate_with_hole(), &p).unwrap();
    let sharp = contour(&plate_with_hole(), &ContourParams { blend_turns: false, ..p }).unwrap();
    assert!(t.arc_count() > sharp.arc_count(), "{:?}", t.notes);
    assert!(t.notes.iter().any(|n| n.contains("rounded")), "{:?}", t.notes);
    write_golden("contour_plate_with_hole_blended.cl", &gen_to_cltext(&t));
}

#[test]
fn pocket_zigzag_plate_with_island_blended_matches_golden() {
    // M11.1 C's byte witness: the zigzag-around-an-island job with the
    // row-to-link jogs filleted inside the cleared floor (quarter arcs
    // of at most stepover/2); plunges, lifts and pass starts are the
    // sharp golden's to the byte.
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
        strategy: PocketStrategy::Zigzag,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: true,
    };
    let t = pocket(&square_with_center(0.0, 0.0, 30.0, 30.0), &p).unwrap();
    assert!(t.has_arcs(), "{:?}", t.notes);
    write_golden("pocket_zigzag_plate_with_island_blended.cl", &gen_to_cltext(&t));
}

#[test]
fn pocket_waterline_matches_golden() {
    // Same region, waterline rings. Committed as its own golden because
    // ring ordering is a place where a "small" generator change tends to
    // reorder everything.
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
        strategy: PocketStrategy::Waterline,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
    };
    let t = pocket(&square_with_center(0.0, 0.0, 30.0, 30.0), &p).unwrap();
    write_golden("pocket_waterline_plate_with_island.cl", &gen_to_cltext(&t));
}

#[test]
fn slot_fullwidth_centerline_matches_golden() {
    // Tool fills the channel (dia 10 = width 10): one centerline row per
    // level, the everyday slot.
    let p = SlotParams {
        links: camcraft_app::links::Links::PLANAR,
        tool_diameter: 10.0,
        width: 10.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: -6.0,
        stepdown: 3.0,
        stepover: 5.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
    };
    let t = slot(Point2::new(0.0, 0.0), Point2::new(60.0, 0.0), &p).unwrap();
    write_golden("slot_fullwidth_centerline.cl", &gen_to_cltext(&t));
}

#[test]
fn slot_wide_channel_rows_matches_golden() {
    // Width 20 with a 6 mm tool, stepover 4: several serpentine rows,
    // each shortened by the round-end reach sqrt(h^2 - d^2) - the case
    // where the closed-form stadium math must agree with a Clipper offset.
    let p = SlotParams {
        links: camcraft_app::links::Links::PLANAR,
        tool_diameter: 6.0,
        width: 20.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: -4.0,
        stepdown: 4.0,
        stepover: 4.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
    };
    let t = slot(Point2::new(-10.0, 0.0), Point2::new(50.0, 0.0), &p).unwrap();
    write_golden("slot_wide_channel_rows.cl", &gen_to_cltext(&t));
}

#[test]
fn trace_plate_with_hole_matches_golden() {
    // Engraving: cutter center exactly on both loops, at every level.
    let p = TraceParams {
        clearance_z: 5.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: -2.0,
        stepdown: 1.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
    };
    let t = trace(&plate_with_hole(), &p).unwrap();
    write_golden("trace_plate_with_hole.cl", &gen_to_cltext(&t));
}

// ------------------------------------------------------------ drilling

use camcraft_app::drilling::{CycleChoice, CycleParams, Hole, drill};
use camcraft_app::tools::{Tool, ToolKind};

fn cycle_tool(kind: ToolKind, diameter: f64, tip_angle: f64, pitch: f64) -> Tool {
    Tool {
        kind,
        diameter,
        corner_r: 0.0,
        tip_angle,
        pitch,
        nose_radius: 0.0,
        width: 0.0,
        flute_len: 40.0,
        shank_d: diameter.min(10.0),
        shank_len: 30.0,
        holder_d: 20.0,
        holder_len: 40.0,
        material_group: String::from("steel"), center_cutting: true,
        rpm: 500.0,
        feed: 200.0,
        tool_material: None,
        flutes: None,
        orientation: None,
        turning: None,
    }
}

#[test]
fn drill_plate_matches_golden() {
    // Auto law, shallow side: both holes are within one diameter deep,
    // so both post as plain G81s. The through hole's floor carries the
    // 1 mm exit allowance; the `; CYCLE` lines are the v3 witness that
    // the intent rides alongside the chain.
    let tool = cycle_tool(ToolKind::Drill, 6.0, 118.0, 0.0);
    let holes = [
        Hole {
            xy: Point2::new(10.0, 10.0),
            radius: 3.0,
            entry_z: 0.0,
            floor_z: -4.0,
            through: true,
            stepped: false,
        },
        Hole {
            xy: Point2::new(30.0, 10.0),
            radius: 3.0,
            entry_z: 0.0,
            floor_z: -3.0,
            through: false,
            stepped: false,
        },
    ];
    let p = CycleParams {
        clearance_z: 5.0,
        r_z: 0.0,
        hop_z: None,
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 200.0,
        plunge_feed: 60.0,
        tool: &tool,
        holes: &holes,
    };
    let t = drill(&p).unwrap();
    assert_eq!(t.cycles.len(), 2);
    assert!(
        t.cycles.iter().all(|c| c.kind == camcraft_app::cl::CycleKind::Drill),
        "one-diameter depths must Auto to plain Drill"
    );
    write_golden("drill_plate.cl", &gen_to_cltext(&t));
}

#[test]
fn drill_peck_matches_golden() {
    // Auto law, deep side: 24 mm on a 6 mm drill is exactly 4
    // diameters - the chip-break Peck band (G73 shape), auto step 3d.
    // The record says the same: one hop, two plunges, one path.
    let tool = cycle_tool(ToolKind::Drill, 6.0, 118.0, 0.0);
    let holes = [Hole {
        xy: Point2::new(20.0, 10.0),
        radius: 3.0,
        entry_z: 0.0,
        floor_z: -23.0,
        through: true,
        stepped: false,
    }];
    let p = CycleParams {
        clearance_z: 6.0,
        r_z: 0.0,
        hop_z: None,
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 200.0,
        plunge_feed: 60.0,
        tool: &tool,
        holes: &holes,
    };
    let t = drill(&p).unwrap();
    assert_eq!(t.cycles.len(), 1);
    assert_eq!(t.cycles[0].kind, camcraft_app::cl::CycleKind::Peck);
    assert!((t.cycles[0].peck - 18.0).abs() < 1e-9, "auto chip-break step = 3d");
    write_golden("drill_peck.cl", &gen_to_cltext(&t));
}

#[test]
fn tap_plate_matches_golden() {
    // Rigid tapping: Auto sees the Tap tool and taps - feed down and
    // back at rpm x pitch, no rapids inside the hole. The chain is the
    // truth for verify; the cycle line is what the post emits as G84.
    let tool = cycle_tool(ToolKind::Tap, 10.0, 0.0, 1.5);
    let holes = [Hole {
        xy: Point2::new(15.0, 15.0),
        radius: 2.5,
        entry_z: 0.0,
        floor_z: -12.0,
        through: false,
        stepped: false,
    }];
    let p = CycleParams {
        clearance_z: 5.0,
        r_z: 0.0,
        hop_z: None,
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 200.0,
        plunge_feed: 60.0,
        tool: &tool,
        holes: &holes,
    };
    let t = drill(&p).unwrap();
    assert_eq!(t.cycles.len(), 1);
    assert_eq!(t.cycles[0].kind, camcraft_app::cl::CycleKind::Tap);
    write_golden("tap_plate.cl", &gen_to_cltext(&t));
}

// ---------------------------------------------------------------------------
// M2.2a - the 3D waterline. The same rule as the 2D generators: the exact
// ring set, their arc fits, and the level order must not move unnoticed.
// ---------------------------------------------------------------------------

#[test]
fn finish3d_waterline_cone() {
    use camcraft_app::finishing3d::{waterline, Finish3DParams, FinishPattern};
    use camcraft_app::geometry::{Point3, Triangle};
    let p3 = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    // 45-degree cone, apex (0,0,20), R20 base ring at z 0, closed bottom.
    let n = 96;
    let mut facets = Vec::new();
    for j in 0..n {
        let a = std::f64::consts::TAU * j as f64 / n as f64;
        let b = std::f64::consts::TAU * (j + 1) as f64 / n as f64;
        facets.push(Triangle::new(
            p3(0.0, 0.0, 20.0),
            p3(20.0 * a.cos(), 20.0 * a.sin(), 0.0),
            p3(20.0 * b.cos(), 20.0 * b.sin(), 0.0),
        ));
        facets.push(Triangle::new(
            p3(0.0, 0.0, 0.0),
            p3(20.0 * b.cos(), 20.0 * b.sin(), 0.0),
            p3(20.0 * a.cos(), 20.0 * a.sin(), 0.0),
        ));
    }
    let t = waterline(
        &facets,
        &Finish3DParams {
            pattern: FinishPattern::Waterline,
            stepover: None,
            tool_diameter: 6.0,
            tolerance: 0.1,
            clearance_z: 25.0,
            retract_z: None,
            top_z: 16.0,
            bottom_z: 0.5,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            ramp: false,
            min_step: None,
            ramp_angle: 45.0,
            entry_ramp: false,
            steep_stepover: None,
            pencil_reference: None,
        },
    )
    .unwrap();
    write_golden("finish3d_waterline_cone.cl", &gen_to_cltext(&t));
}

// M2.2c - the scallop. The plateau, not the cone: its shelf (m_z = 1,
// firmly shallow) and walls (m_z = 0, firmly steep) sit nowhere near
// the 45-deg jurisdiction razor, so the diagonal crossing, the break
// law and the ring handover are all pinned by this dump without a
// single facet balanced on the split line.

#[test]
fn finish3d_scallop_plateau() {
    use camcraft_app::finishing3d::{parallel, Finish3DParams, FinishPattern};
    use camcraft_app::geometry::{Point3, Triangle};
    let p3 = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    // 20x20x10 block (the finish3d.rs plateau fixture's proportions):
    // top at z 10, four walls, no bottom - a finish model, not a
    // solid; the raster's laws need only the surfaces it rides.
    let q = |sx: f64, sy: f64, z: f64| p3(sx * 10.0, sy * 10.0, z);
    let (a, b, c, d) = (q(-1.0, -1.0, 0.0), q(1.0, -1.0, 0.0), q(1.0, 1.0, 0.0), q(-1.0, 1.0, 0.0));
    let (e, f, g, h) = (q(-1.0, -1.0, 10.0), q(1.0, -1.0, 10.0), q(1.0, 1.0, 10.0), q(-1.0, 1.0, 10.0));
    let quad = |p: [Point3; 4]| {
        vec![
            Triangle::new(p[0], p[1], p[2]),
            Triangle::new(p[0], p[2], p[3]),
        ]
    };
    let mut facets = Vec::new();
    facets.extend(quad([e, f, g, h])); // top
    facets.extend(quad([a, b, f, e])); // sides
    facets.extend(quad([b, c, g, f]));
    facets.extend(quad([c, d, h, g]));
    facets.extend(quad([d, a, e, h]));
    let t = parallel(
        &facets,
        &Finish3DParams {
            pattern: FinishPattern::Scallop,
            stepover: None,
            tool_diameter: 6.0,
            tolerance: 0.1,
            clearance_z: 25.0,
            retract_z: None,
            top_z: 10.0,
            bottom_z: 0.0,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            ramp: false,
            min_step: None,
            ramp_angle: 45.0,
            entry_ramp: false,
            steep_stepover: None,
            pencil_reference: None,
        },
    )
    .unwrap();
    write_golden("finish3d_scallop_plateau.cl", &gen_to_cltext(&t));
}

// M2.2e stage D - the ramp. The plateau's plain parallel raster is the
// chain's home fixture: flat ground certifies every line-to-line
// diagonal, so this dump pins WHICH links became Feeds and where the
// surviving plunge (first contact) sits - the whole between-lines
// change, on the record.

#[test]
fn finish3d_ramp_parallel_plateau() {
    use camcraft_app::finishing3d::{parallel, Finish3DParams, FinishPattern};
    use camcraft_app::geometry::{Point3, Triangle};
    let p3 = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    // The scallop golden's block again: 20x20x10, top at z 10, no bottom.
    let q = |sx: f64, sy: f64, z: f64| p3(sx * 10.0, sy * 10.0, z);
    let (a, b, c, d) = (q(-1.0, -1.0, 0.0), q(1.0, -1.0, 0.0), q(1.0, 1.0, 0.0), q(-1.0, 1.0, 0.0));
    let (e, f, g, h) = (q(-1.0, -1.0, 10.0), q(1.0, -1.0, 10.0), q(1.0, 1.0, 10.0), q(-1.0, 1.0, 10.0));
    let quad = |p: [Point3; 4]| {
        vec![
            Triangle::new(p[0], p[1], p[2]),
            Triangle::new(p[0], p[2], p[3]),
        ]
    };
    let mut facets = Vec::new();
    facets.extend(quad([e, f, g, h])); // top
    facets.extend(quad([a, b, f, e])); // sides
    facets.extend(quad([b, c, g, f]));
    facets.extend(quad([c, d, h, g]));
    facets.extend(quad([d, a, e, h]));
    let t = parallel(
        &facets,
        &Finish3DParams {
            pattern: FinishPattern::Parallel,
            stepover: None,
            tool_diameter: 6.0,
            tolerance: 0.1,
            clearance_z: 25.0,
            retract_z: None,
            top_z: 10.0,
            bottom_z: 0.0,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            ramp: true,
            min_step: None,
            ramp_angle: 45.0,
            entry_ramp: false,
            steep_stepover: None,
            pencil_reference: None,
        },
    )
    .unwrap();
    write_golden("finish3d_ramp_parallel_plateau.cl", &gen_to_cltext(&t));
}

// M2.2d - the polar placements. Radial rides the dome (curved ground
// is where chord overlap toward the center actually happens, and the
// flanks' rings fold into the same dump); spiral rides the plateau
// (a flat square is the polar law's hardest test: the march crosses
// it along a single in-winding curve and the walls' rings close).

#[test]
fn finish3d_radial_dome() {
    use camcraft_app::finishing3d::{radial, Finish3DParams, FinishPattern};
    use camcraft_app::geometry::{Point3, Triangle};
    let p3 = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    // Spherical cap, apex (0,0,12), rim at polar 60 deg, closed by a
    // flat base - the dome fixture's shape at golden-sized tessellation.
    let (radius, polar_max, n_lat, n_lon) = (12.0, std::f64::consts::FRAC_PI_3, 6usize, 24usize);
    let mut facets = Vec::new();
    let ring = |k: usize| -> Vec<Point3> {
        let ph = polar_max * k as f64 / n_lat as f64;
        let (z, rr) = (radius * ph.cos(), radius * ph.sin());
        (0..n_lon)
            .map(|j| {
                let th = std::f64::consts::TAU * j as f64 / n_lon as f64;
                p3(rr * th.cos(), rr * th.sin(), z)
            })
            .collect()
    };
    let apex = p3(0.0, 0.0, radius);
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
    let zb = radius * polar_max.cos();
    let center = p3(0.0, 0.0, zb);
    for j in 0..n_lon {
        facets.push(Triangle::new(center, rim[j], rim[(j + 1) % n_lon]));
    }
    let t = radial(
        &facets,
        &Finish3DParams {
            pattern: FinishPattern::Radial,
            stepover: None,
            tool_diameter: 6.0,
            tolerance: 0.1,
            clearance_z: 25.0,
            retract_z: None,
            top_z: 12.0,
            bottom_z: 6.0,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            ramp: false,
            min_step: None,
            ramp_angle: 45.0,
            entry_ramp: false,
            steep_stepover: None,
            pencil_reference: None,
        },
    )
    .unwrap();
    write_golden("finish3d_radial_dome.cl", &gen_to_cltext(&t));
}

#[test]
fn finish3d_spiral_plateau() {
    use camcraft_app::finishing3d::{spiral, Finish3DParams, FinishPattern};
    use camcraft_app::geometry::{Point3, Triangle};
    let p3 = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    // The scallop golden's block: 20x20x10, top at z 10, no bottom.
    let q = |sx: f64, sy: f64, z: f64| p3(sx * 10.0, sy * 10.0, z);
    let (a, b, c, d) = (q(-1.0, -1.0, 0.0), q(1.0, -1.0, 0.0), q(1.0, 1.0, 0.0), q(-1.0, 1.0, 0.0));
    let (e, f, g, h) = (q(-1.0, -1.0, 10.0), q(1.0, -1.0, 10.0), q(1.0, 1.0, 10.0), q(-1.0, 1.0, 10.0));
    let quad = |p: [Point3; 4]| {
        vec![
            Triangle::new(p[0], p[1], p[2]),
            Triangle::new(p[0], p[2], p[3]),
        ]
    };
    let mut facets = Vec::new();
    facets.extend(quad([e, f, g, h])); // top
    facets.extend(quad([a, b, f, e])); // sides
    facets.extend(quad([b, c, g, f]));
    facets.extend(quad([c, d, h, g]));
    facets.extend(quad([d, a, e, h]));
    let t = spiral(
        &facets,
        &Finish3DParams {
            pattern: FinishPattern::Spiral,
            stepover: None,
            tool_diameter: 6.0,
            tolerance: 0.1,
            clearance_z: 25.0,
            retract_z: None,
            top_z: 10.0,
            bottom_z: 0.0,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            ramp: false,
            min_step: None,
            ramp_angle: 45.0,
            entry_ramp: false,
            steep_stepover: None,
            pencil_reference: None,
        },
    )
    .unwrap();
    write_golden("finish3d_spiral_plateau.cl", &gen_to_cltext(&t));
}

#[test]
fn finish3d_contour_plateau() {
    use camcraft_app::finishing3d::{contour, Finish3DParams, FinishPattern};
    use camcraft_app::geometry::{Point3, Triangle};
    let p3 = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    // The scallop golden's block: 20x20x10, top at z 10, no bottom.
    let q = |sx: f64, sy: f64, z: f64| p3(sx * 10.0, sy * 10.0, z);
    let (a, b, c, d) = (q(-1.0, -1.0, 0.0), q(1.0, -1.0, 0.0), q(1.0, 1.0, 0.0), q(-1.0, 1.0, 0.0));
    let (e, f, g, h) = (q(-1.0, -1.0, 10.0), q(1.0, -1.0, 10.0), q(1.0, 1.0, 10.0), q(-1.0, 1.0, 10.0));
    let quad = |p: [Point3; 4]| {
        vec![
            Triangle::new(p[0], p[1], p[2]),
            Triangle::new(p[0], p[2], p[3]),
        ]
    };
    let mut facets = Vec::new();
    facets.extend(quad([e, f, g, h])); // top
    facets.extend(quad([a, b, f, e])); // sides
    facets.extend(quad([b, c, g, f]));
    facets.extend(quad([c, d, h, g]));
    facets.extend(quad([d, a, e, h]));
    let t = contour(
        &facets,
        &Finish3DParams {
            pattern: FinishPattern::Contour,
            stepover: None,
            tool_diameter: 6.0,
            tolerance: 0.1,
            clearance_z: 25.0,
            retract_z: None,
            top_z: 10.0,
            bottom_z: 0.0,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            ramp: false,
            min_step: None,
            ramp_angle: 45.0,
            entry_ramp: false,
            steep_stepover: None,
            pencil_reference: None,
        },
    )
    .unwrap();
    write_golden("finish3d_contour_plateau.cl", &gen_to_cltext(&t));
}

#[test]
fn finish3d_project_dome() {
    use camcraft_app::finishing3d::{project, Finish3DParams, FinishPattern};
    use camcraft_app::geometry::{Point3, Triangle};
    let p3 = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
    // The radial golden's dome, verbatim - so the two polar-family
    // goldens differ only in the kinematic.
    let (radius, polar_max, n_lat, n_lon) = (12.0, std::f64::consts::FRAC_PI_3, 6usize, 24usize);
    let mut facets = Vec::new();
    let ring = |k: usize| -> Vec<Point3> {
        let ph = polar_max * k as f64 / n_lat as f64;
        let (z, rr) = (radius * ph.cos(), radius * ph.sin());
        (0..n_lon)
            .map(|j| {
                let th = std::f64::consts::TAU * j as f64 / n_lon as f64;
                p3(rr * th.cos(), rr * th.sin(), z)
            })
            .collect()
    };
    let apex = p3(0.0, 0.0, radius);
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
    let zb = radius * polar_max.cos();
    let center = p3(0.0, 0.0, zb);
    for j in 0..n_lon {
        facets.push(Triangle::new(center, rim[j], rim[(j + 1) % n_lon]));
    }
    let t = project(
        &facets,
        &Finish3DParams {
            pattern: FinishPattern::Project,
            stepover: None,
            tool_diameter: 6.0,
            tolerance: 0.1,
            clearance_z: 25.0,
            retract_z: None,
            top_z: 12.0,
            bottom_z: 6.0,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            ramp: false,
            min_step: None,
            ramp_angle: 45.0,
            entry_ramp: false,
            steep_stepover: None,
            pencil_reference: None,
        },
    )
    .unwrap();
    write_golden("finish3d_project_dome.cl", &gen_to_cltext(&t));
}

/// #670: a plasma plate - a 120x80 outline with a Ø40 hole holding a
/// 12x12 island part, a Ø10 small hole and a 30x10 slot - cut with a
/// 1.5 mm kerf in the computer: the torch records (kerf, spans, roles,
/// parents) pinned with the moves (the `; PLASMA` / `; TORCH` lines,
/// docs/CNC-INTERMEDIATE.md), the island before its hole, every hole
/// before the outline, the small hole by the Primer's rule.
#[test]
fn plasma_plate_with_holes_matches_golden() {
    use camcraft_app::plasma::{plasma_profile, PlasmaParams};
    let loops = vec![
        rect(0.0, 0.0, 120.0, 80.0).loops()[0].clone(),
        circle(Point2::new(35.0, 40.0), 20.0, 64),
        rect(29.0, 34.0, 41.0, 46.0).loops()[0].clone(),
        circle(Point2::new(80.0, 60.0), 5.0, 32),
        rect(70.0, 15.0, 100.0, 25.0).loops()[0].clone(),
    ];
    let part = Region::new(loops.clone()).unwrap();
    let p = PlasmaParams {
        kerf: 1.5,
        control_comp: false,
        reversed: false,
        lead_in: 4.0,
        lead_out: 3.0,
        small_hole_dia: 32.0,
        small_hole_feed: 0.65,
        overburn: 1.5,
        thickness: 6.0,
        feed: Some(2500.0),
        clearance_z: 15.0,
        cut_z: 6.0,
        pierce_height: Some(3.8),
        cut_height: None,
        pierce_delay: Some(0.5),
    };
    let path = plasma_profile(&loops, &[], &part, &p).unwrap();
    let text = gen_to_cltext(&path);
    // The in-app writer and the golden writer agree byte for byte.
    assert_eq!(camcraft_app::cl::text::to_text(&path).unwrap(), text);
    assert_eq!(camcraft_app::cl::text::from_text(&text).unwrap().plasma, path.plasma);
    // The superset law: the post's stream reader takes it - the `; PLASMA`
    // / `; TORCH` lines read as its plasma records (#671, plasma_post.rs).
    let recs = camcraft_app::post::cnc::parse(&text).unwrap();
    assert!(recs.iter().any(|r| matches!(r, camcraft_app::post::cnc::Record::Plasma(_))));
    write_golden("plasma_plate_with_holes.cl", &text);
}

#[test]
fn trochoid_slot_small_matches_golden() {
    // #220: a D6 in a 10 mm channel, 12 mm long, one level - the bore
    // helix, the front arcs with their wall leads and fillets, the
    // microlift retraces and the cap pencil, every arc an arc.
    let p = camcraft_app::machining2d::TrochoidParams {
        tool_diameter: 6.0,
        width: 10.0,
        loop_width: None,
        step: 1.5,
        max_engagement: 90.0,
        ret: camcraft_app::machining2d::TrochoidReturn::Microlift { height: 0.3 },
        smoothing: 0.5,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: Some(1.0),
        top_z: -2.0,
        bottom_z: -2.0,
        stepdown: 2.0,
        feed: 800.0,
        return_feed: 2000.0,
        plunge_feed: 200.0,
        climb: true,
    };
    let t = camcraft_app::machining2d::trochoidal_slot(&[Point2::new(0.0, 0.0), Point2::new(12.0, 0.0)], &p).unwrap();
    assert!(t.has_arcs());
    write_golden("trochoid_slot_small.cl", &gen_to_cltext(&t));
}
