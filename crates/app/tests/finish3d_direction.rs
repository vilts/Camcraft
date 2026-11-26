//! #2377: Parallel directions through the app's stock, area, and verifier.
//! Gate map (#2377): covers app::operation (generate, finish_clip, wcs_yaw),
//! camcraft_motion::finishing3d, camcraft_sim::verify.
//! Provenance: analytic plane and ball tangency over an original fixture.

use camcraft_app::cl::{Move, MoveKind, Toolpath};
use camcraft_app::commands::{
    AddTool, CreateOperation, History, ImportModel, SetStock, SetWcsOrigin,
};
use camcraft_app::finishing3d::FinishPattern;
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube, quat_from_yaw};
use camcraft_app::operation::{self, AreaControls, Containment, Height, OpParams, OpType, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::verify::Warning;
use camcraft_app::{model, tools, world};
use flecs_ecs::prelude::World;

const TOOL: &str = "BN-0600";
const RADIUS: f64 = 3.0;
const FINISH_TOL: f64 = 0.05;
const PLATE_TOP: f64 = 6.0;
const X_MIN: f64 = 13.0;
const X_MAX: f64 = 37.0;
const Y_MIN: f64 = -12.0;
const Y_MAX: f64 = 2.0;
const SLOPE_X: f64 = 0.125;
const SLOPE_Y: f64 = 0.0625;

fn roof(x: f64, y: f64) -> f64 {
    4.0 + SLOPE_X * (x - X_MIN) + SLOPE_Y * (y - Y_MIN)
}

/// An off-centre 24 x 14 mm solid with a shallow roof that rises in both axes.
fn wedge() -> Mesh {
    let corners = [
        (X_MIN, Y_MIN),
        (X_MAX, Y_MIN),
        (X_MAX, Y_MAX),
        (X_MIN, Y_MAX),
    ];
    let bottom = corners.map(|(x, y)| Point3::new(x, y, 0.0));
    let top = corners.map(|(x, y)| Point3::new(x, y, roof(x, y)));
    let [a, b, c, d] = bottom;
    let [e, f, g, h] = top;
    Mesh::from_facets(
        [
            [a, d, c, b],
            [e, f, g, h],
            [a, b, f, e],
            [b, c, g, f],
            [c, d, h, g],
            [d, a, e, h],
        ]
        .into_iter()
        .flat_map(|[p, q, r, s]| [Triangle::new(p, q, r), Triangle::new(p, r, s)])
        .collect(),
    )
}

fn document(part: Mesh, yaw: f64) -> (World, History) {
    let world = world::new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", part));
    let pos = Point3::new(25.0, -5.0, 0.0);
    let stock = StockShape::Box {
        dx: 28.0,
        dy: 18.0,
        dz: 8.0,
    };
    h.execute(
        &world,
        SetStock::new(
            &world,
            stock,
            Location {
                pos,
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let tool = tools::catalog()
        .into_iter()
        .find(|(name, _)| *name == TOOL)
        .unwrap()
        .1;
    h.execute(&world, AddTool::new(&world, TOOL, tool).unwrap());
    if yaw != 0.0 {
        let loc = Location {
            pos: Point3::new(11.0, -14.0, 8.0),
            rot: quat_from_yaw(yaw.to_radians()),
        };
        h.execute(&world, SetWcsOrigin::new(&world, loc).unwrap());
    }
    (world, h)
}

fn params(pattern: FinishPattern, angle: f64) -> OpParams {
    let mut p = operation::seed_params(OpType::Finish3D.ordinal());
    let OpParams::Finish3D {
        pattern: pat,
        direction_angle,
        tolerance,
        stepover,
        clearance_z,
        top_z,
        bottom_z,
        ..
    } = &mut p
    else {
        panic!("expected the 3D Finish seed");
    };
    *pat = pattern;
    *direction_angle = angle;
    *tolerance = FINISH_TOL;
    *stepover = Some(1.0);
    *clearance_z = Height::fixed(18.0);
    *top_z = Height::fixed(8.0);
    *bottom_z = Height::fixed(0.0);
    p
}

fn operation_with(world: &World, h: &mut History, pattern: FinishPattern) -> Operation {
    h.execute(
        world,
        CreateOperation::new(
            world,
            "Finish",
            TOOL,
            SrcRef::Entity("part".into()),
            params(pattern, 0.0),
        )
        .unwrap(),
    );
    let entity = model::find_by_name(world, "Finish").unwrap();
    model::operation_of(world, &entity).unwrap()
}

fn inside_part(p: Point3, inset: f64) -> bool {
    (X_MIN + inset..=X_MAX - inset).contains(&p.x) && (Y_MIN + inset..=Y_MAX - inset).contains(&p.y)
}

fn assert_raster(
    path: &Toolpath,
    angle: f64,
    is_raster: impl Fn(&Move) -> bool,
    tip_z_at: impl Fn(f64, f64) -> f64,
) {
    let (uy, ux) = angle.to_radians().sin_cos();
    let cross = |p: Point3| p.y * ux - p.x * uy;
    let mut rows = Vec::new();
    let mut swept = 0.0;
    for m in path.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
        for p in [m.from, m.to] {
            assert!(
                inside_part(p, RADIUS - 1e-3),
                "{angle}°: the cutter extends beyond the physical boundary at {p:?}"
            );
        }
        let dr = m.to - m.from;
        let length = dr.x.hypot(dr.y);
        if length <= 1e-8 || !is_raster(m) {
            continue;
        }
        assert!(m.arc.is_none(), "a clipped raster row is straight: {m:?}");
        let off_axis = (dr.y * ux - dr.x * uy).abs();
        assert!(
            off_axis <= 1e-6 * length + 1e-8,
            "{angle}°: segment {m:?} goes off its raster row"
        );
        for p in [m.from, m.to] {
            assert!(
                (p.z - tip_z_at(p.x, p.y)).abs() < 0.015,
                "{angle}°: the ball does not follow the target surface at {p:?}"
            );
        }
        rows.push(cross(m.from));
        swept += length;
    }
    assert!(
        swept > 35.0,
        "{angle}°: expected several cutting rows, got {swept} mm"
    );
    // Cross-feed advances along V=(-sin(angle), cos(angle)). Its order
    // distinguishes 90 from 270 and 0 from 180 even with alternating rows.
    let span = rows.last().unwrap() - rows.first().unwrap();
    assert!(
        span > 4.0,
        "{angle}°: rows did not advance along +V, span {span}"
    );
    rows.sort_by(f64::total_cmp);
    rows.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    assert!(
        rows.len() >= 4,
        "{angle}°: expected several separated rows: {rows:?}"
    );
}

fn clipped_parallel(part: Mesh, angle: f64, yaw: f64) -> Toolpath {
    let (world, mut h) = document(part, yaw);
    let mut op = operation_with(&world, &mut h, FinishPattern::Parallel);
    op.params = params(FinishPattern::Parallel, angle);
    op.area = AreaControls {
        containment: Some(Containment::Inside),
        ..Default::default()
    };
    let (path, warnings) = operation::generate(&world, &op, "Finish").unwrap();
    assert!(
        !warnings.iter().any(|w| match w {
            Warning::Gouge { .. } => true,
            Warning::Envelope { gouge_mm, .. } => *gouge_mm > FINISH_TOL,
            _ => false,
        }),
        "{angle}° with WCS yaw {yaw}°: {warnings:?}"
    );
    path
}

#[test]
fn zero_and_ninety_degree_rasters_cut_the_flat_top() {
    let lo = Point3::new(X_MIN, Y_MIN, 0.0);
    let hi = Point3::new(X_MAX, Y_MAX, PLATE_TOP);
    for angle in [0.0, 90.0] {
        let path = clipped_parallel(cube(lo, hi), angle, 0.0);
        // The plateau feeds cross its interior at the top plane. A
        // level waterline on the wall falls outside those cuts.
        assert_raster(
            &path,
            angle,
            |m| {
                let mid = Point3::new(
                    m.from.x.midpoint(m.to.x),
                    m.from.y.midpoint(m.to.y),
                    PLATE_TOP,
                );
                (m.from.z - PLATE_TOP).abs() < 1e-6
                    && (m.to.z - PLATE_TOP).abs() < 1e-6
                    && inside_part(mid, RADIUS + 0.25)
            },
            |_, _| PLATE_TOP,
        );
    }
}

#[test]
fn parallel_angles_cut_the_clipped_roof_and_pass_verification() {
    let offset = RADIUS * (1.0f64.hypot(SLOPE_X.hypot(SLOPE_Y)) - 1.0);
    for (angle, yaw) in [(180.0, 0.0), (270.0, 0.0), (45.0, 0.0), (45.0, 30.0)] {
        let path = clipped_parallel(wedge(), angle, yaw);
        // The waterline feeds stay at one Z. These directions cross the
        // slope, so their raster feeds change Z.
        assert_raster(
            &path,
            angle + yaw,
            |m| (m.to.z - m.from.z).abs() > 1e-8,
            |x, y| roof(x, y) + offset,
        );
    }
}

#[test]
fn nonparallel_patterns_keep_their_paths_and_stamp_with_a_saved_angle() {
    let (world, mut h) = document(wedge(), 0.0);
    let mut op = operation_with(&world, &mut h, FinishPattern::Scallop);
    for pattern in [
        FinishPattern::ParallelMulti,
        FinishPattern::Scallop,
        FinishPattern::SteepShallow,
        FinishPattern::Ramp,
        FinishPattern::Waterline,
    ] {
        op.params = params(pattern, 0.0);
        let old_stamp = operation::current_stamp(
            &world,
            TOOL,
            &op.src,
            &op.params,
            "Finish",
            op.setup.as_deref(),
        )
        .unwrap();
        let (old_path, _) = operation::generate(&world, &op, "Finish").unwrap();
        op.params = params(pattern, 45.0);
        let new_stamp = operation::current_stamp(
            &world,
            TOOL,
            &op.src,
            &op.params,
            "Finish",
            op.setup.as_deref(),
        )
        .unwrap();
        let (new_path, _) = operation::generate(&world, &op, "Finish").unwrap();
        assert_eq!(old_stamp, new_stamp, "{pattern:?}");
        assert_eq!(old_path.moves, new_path.moves, "{pattern:?}");
    }
}

#[test]
fn finish_creation_refuses_invalid_typed_angles() {
    let (world, _) = document(wedge(), 0.0);
    let src = SrcRef::Entity("part".into());
    for angle in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -0.001,
        360.0,
        720.0,
    ] {
        let result = CreateOperation::new(
            &world,
            "invalid",
            TOOL,
            src.clone(),
            params(FinishPattern::Parallel, angle),
        );
        assert!(
            matches!(result, Err(e) if e.to_string().contains("direction angle")),
            "accepted or misdiagnosed {angle}"
        );
    }
}
