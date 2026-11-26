//! Arc endpoints and centers in the posted control coordinates.
//! Provenance: original DXF fixtures, rigid transforms and the Fanuc I/J convention.

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{DxfOptions, GenerateOperation, History, MoveSketch, SetWcsOrigin};
use camcraft_app::geometry::{Point3, quat_from_yaw};
use camcraft_app::operation::{OpType, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;

use super::{DXF, OP, PROFILE, TOP, add_op, drawing_job, drawing_job_with, generated, layer, near, params};

const PLACEMENT: [f64; 2] = [16.123, 8.4562];
const WCS: [f64; 2] = [2.375, -3.625];
const ANGLE: f64 = 31.0;

fn arc_drawing(angle: f64) -> String {
    format!(
        "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
         0\nARC\n8\narc\n10\n10\n20\n5\n40\n8\n50\n0\n51\n{angle}\n0\nENDSEC\n0\nEOF\n"
    )
}

fn arc_job(angle: f64, kind: OpType) -> (flecs_ecs::prelude::World, History) {
    // The small join tolerance leaves even the 0.001-degree stroke open
    // (its two ends are 0.000140 mm apart).
    let options = DxfOptions {
        join_tolerance: Some(0.00001),
        ..Default::default()
    };
    let (world, mut history) = drawing_job_with(TOP, &arc_drawing(angle), &options);
    history.execute(
        &world,
        MoveSketch::new(&world, PROFILE, PLACEMENT[0], PLACEMENT[1], ANGLE).expect("place sketch"),
    );
    let loc = Location {
        pos: Point3::new(WCS[0], WCS[1], 0.0),
        rot: quat_from_yaw((-13.0_f64).to_radians()),
    };
    history.execute(&world, SetWcsOrigin::new(&world, loc).expect("place work offset"));
    add_op(&world, &mut history, SrcRef::Entity(PROFILE.into()), params(kind, true, 0.5, 0.5));
    (world, history)
}

fn word(block: &str, address: char) -> Option<f64> {
    block.split_whitespace().find_map(|w| w.strip_prefix(address)?.parse().ok())
}

fn assert_posted_arcs(nc: &str, center: [f64; 2], count: usize) {
    let mut at: Option<[f64; 2]> = None;
    let mut arcs = 0;
    for block in nc.lines() {
        let Some((x, y)) = word(block, 'X').zip(word(block, 'Y')) else {
            continue;
        };
        if block.split_whitespace().any(|w| matches!(w, "G02" | "G03" | "G2" | "G3")) {
            let [sx, sy] = at.expect("preceding machine XY");
            assert_ne!([sx, sy], [x, y], "same posted start and end commands a full circle: {block}");
            let i = word(block, 'I').expect("I increment");
            let j = word(block, 'J').expect("J increment");
            assert!((sx + i - center[0]).abs() < 0.0011, "I is relative to the actual start: {block}");
            assert!((sy + j - center[1]).abs() < 0.0011, "J is relative to the actual start: {block}");
            arcs += 1;
        }
        at = Some([x, y]);
    }
    assert_eq!(arcs, count, "every CL arc posts: {nc}");
}

#[test]
fn short_and_near_full_arcs_keep_their_center_and_sweep_through_a_rotated_work_offset() {
    let (sin, cos) = ANGLE.to_radians().sin_cos();
    let [cx, cy] = [PLACEMENT[0] + 10.0 * cos - 5.0 * sin, PLACEMENT[1] + 10.0 * sin + 5.0 * cos];

    for (kind, degrees) in [(OpType::Engrave, 0.1), (OpType::Trace, 359.999)] {
        let (world, mut history) = arc_job(degrees, kind);
        let mark = generated(&world, &mut history);
        let feeds: Vec<_> = mark.path.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
        let mut sweep: f64 = 0.0;
        let first_ccw = feeds.first().expect("feed move").arc.expect("true arc").ccw;
        for m in &feeds {
            let arc = m.arc.expect("preserved analytic arc");
            assert_eq!(arc.ccw, first_ccw, "a split keeps its direction");
            near(arc.center.x, cx);
            near(arc.center.y, cy);
            let (r, d) = arc.radius_sweep(m.from, m.to);
            near(r, 8.0);
            sweep += d;
        }
        near(sweep, degrees.to_radians());
        assert_eq!(feeds.len() == 1, degrees < 1.0, "near-full arcs need distinct endpoints");
        let nc = posting::post_setup_unwritten(&world).expect("post representable arcs");
        assert_posted_arcs(&nc.program, [cx - WCS[0], cy - WCS[1]], feeds.len());
    }
}

#[test]
fn a_short_arc_with_coincident_posted_endpoints_refuses_by_name() {
    let (world, mut history) = arc_job(0.001, OpType::Engrave);
    let why = match GenerateOperation::new(&world, OP) {
        Err(e) => e.to_string(),
        Ok(cmd) => {
            history.execute(&world, cmd);
            posting::post_setup_unwritten(&world).expect_err("an unrepresentable arc must not post")
        }
    };
    let why = why.to_lowercase();
    assert!(
        why.contains("arc") && (why.contains("short") || why.contains("precision") || why.contains("resolution")),
        "{why}"
    );
}

#[test]
fn a_real_tolerance_joined_gap_gets_its_own_safe_reentry() {
    let dxf = DXF.replace("11\n10\n21\n9\n", "11\n9.995\n21\n9\n");
    let (world, mut history) = drawing_job(TOP, &dxf);
    let src = SrcRef::Selection(vec![layer("open")]);
    add_op(&world, &mut history, src, params(OpType::Trace, true, 0.5, 0.5));
    let cut = generated(&world, &mut history);
    let feeds: Vec<_> = cut
        .path
        .moves
        .iter()
        .enumerate()
        .filter(|(_, m)| m.kind == MoveKind::Feed)
        .collect();
    assert_eq!(feeds.len(), 2, "keep the authored line and arc");
    let [(n, line), (m, arc)] = feeds[..] else {
        panic!("one line and one arc")
    };
    assert!(line.arc.is_none() && arc.arc.is_some());
    near(line.to.y, 17.995);
    near(arc.from.y, 18.0);
    let link = &cut.path.moves[n + 1..m];
    assert!(
        link.iter()
            .any(|mv| mv.kind == MoveKind::Rapid && mv.from.y < mv.to.y && mv.to.z > TOP),
        "{link:?}"
    );
    assert_eq!(link.last().expect("reentry").kind, MoveKind::Plunge);
    posting::post_setup_unwritten(&world).expect("post the separated strokes");
}
