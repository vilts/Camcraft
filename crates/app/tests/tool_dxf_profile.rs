//! #743 part 2 acceptance tests:
//! - STEP -> DXF half-profile with centreline (sampling STEP mesh proves every
//!   point lies inside the revolved profile, non-revolved features enveloped)
//! - Named refusal when converting a STEP body without a clear ISO 13399 axis
//! - DXF half-profile command import and named refusal for flute overlap
//! - Round trip STEP -> DXF -> body -> DXF stability within tolerance.
//!
//! Gate map (M4.4): covers app::commands, app::tool_body, app::tool_profile_dxf,
//! tools::spun, geometry::dxf.

use std::f64::consts::TAU;
use std::path::{Path, PathBuf};

use camcraft_app::commands::{AddTool, CommandError, History, SetToolBody};
use camcraft_app::geometry::{self, Triangle};
use camcraft_app::model;
use camcraft_app::project;
use camcraft_app::spun::{SpunPoint, SpunProfile};
use camcraft_app::tool_body;
use camcraft_app::tool_profile_dxf::{
    CENTRELINE_LAYER, PROFILE_LAYER, dxf_to_spun, placed_mesh_to_spun_profile, spun_to_dxf,
    step_to_dxf, step_to_spun_profile, tool_body_to_dxf,
};
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;
use camcraft_geometry::dxf::{Primitive, Unit, Units, parse};

const TOOL: &str = "EM-0400-2FL";

fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/models")
        .join(name)
}

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

fn world_with_tool() -> (flecs_ecs::prelude::World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        AddTool::new(&world, TOOL, catalog_tool(TOOL)).expect("tool"),
    );
    (world, history)
}

/// Evaluate the conservative maximum radius of `spun` at axial height `z`.
fn profile_radius_at(spun: &SpunProfile, z: f64) -> f64 {
    let eps = 1e-6;
    spun.holder_profile()
        .bands()
        .iter()
        .filter(|b| z >= b.above - eps && z <= b.above + b.len + eps)
        .map(|b| b.r)
        .fold(0.0, f64::max)
}

/// Sample vertices, edge midpoints, centroid, and a 5x5 barycentric grid per triangle.
fn sample_triangles(tris: &[Triangle]) -> Vec<[f64; 3]> {
    let mut out = Vec::new();
    for t in tris {
        for i in 0..=5 {
            for j in 0..=(5 - i) {
                let u = i as f64 / 5.0;
                let v = j as f64 / 5.0;
                let w = 1.0 - u - v;
                out.push([
                    u * t.a.x + v * t.b.x + w * t.c.x,
                    u * t.a.y + v * t.b.y + w * t.c.y,
                    u * t.a.z + v * t.b.z + w * t.c.z,
                ]);
            }
        }
    }
    out
}

#[test]
fn step_to_dxf_half_profile_envelops_every_sampled_mesh_point() {
    let path = testdata("tool_csys_box_iso13399.step");
    let dxf = step_to_dxf(&path).expect("ISO 13399 STEP converts to DXF");
    let doc = parse(dxf.as_bytes()).expect("written DXF parses cleanly");
    assert_eq!(doc.units, Units::Known(Unit::Millimeters));

    let centrelines: Vec<_> = doc
        .primitives
        .iter()
        .filter(|p| p.layer.eq_ignore_ascii_case(CENTRELINE_LAYER))
        .collect();
    assert_eq!(centrelines.len(), 1, "DXF carries one centreline entity");
    assert!(matches!(centrelines[0].primitive, Primitive::Line { .. }));
    assert!(
        doc.primitives
            .iter()
            .any(|p| p.layer.eq_ignore_ascii_case(PROFILE_LAYER)),
        "DXF carries profile entities on {PROFILE_LAYER}"
    );

    let spun = dxf_to_spun(dxf.as_bytes()).expect("exported DXF re-imports as SpunProfile");

    // Reconstruct the placed STEP mesh via the exact ISO 13399 CSYS alignment
    // and verify every sampled point lies inside the revolved half-profile.
    let spec = camcraft_app::commands::ToolBodySpec::from_step_auto(&path).expect("step auto");
    let placed = tool_body::place(&spec.facets, spec.axis, spec.offset).expect("places");
    let samples = sample_triangles(&placed);
    assert!(!samples.is_empty());

    let mut saw_corner = false;
    for p in samples {
        let z = p[2];
        let rho = p[0].hypot(p[1]);
        let r_env = profile_radius_at(&spun, z);
        assert!(
            rho <= r_env + 1e-6,
            "mesh sample ({}, {}, {z}) has rho={rho} > envelope r={r_env}",
            p[0],
            p[1]
        );
        if (rho - 10.0 * std::f64::consts::SQRT_2).abs() < 1e-3 {
            saw_corner = true;
        }
    }
    assert!(
        saw_corner,
        "fixture box must exercise corners at r ~ 14.142"
    );
    // Flats at x=+/-10 must be conservatively enveloped to >= 14.142, never cut away to 10.
    assert!(profile_radius_at(&spun, 55.0) >= 10.0 * std::f64::consts::SQRT_2 - 1e-4);
}

#[test]
fn non_revolved_step_or_fine_32_plus_band_mesh_is_conservatively_enveloped() {
    // 40 stepped rings with a simulated axial drive slot/flat cutout on one quadrant:
    // forces > MAX_BANDS reduction (32 -> <=31) and verifies flats/slots never cut away material.
    let mut tris = Vec::new();
    let rings = 40;
    let sides = 16;
    let z_base = 25.0;
    let ring_h = 2.0;
    for k in 0..rings {
        let z0 = z_base + k as f64 * ring_h;
        let z1 = z0 + ring_h;
        let r_base = 12.0 + (k as f64 * 0.4);
        for s in 0..sides {
            let a0 = s as f64 * TAU / sides as f64;
            let a1 = (s + 1) as f64 * TAU / sides as f64;
            // Drive slot: pull radius inward on one angular sector; envelope must keep r_base!
            let r0 = if s == 2 { r_base * 0.55 } else { r_base };
            let r1 = if (s + 1) % sides == 2 {
                r_base * 0.55
            } else {
                r_base
            };
            let p00 = geometry::Point3::new(r0 * a0.cos(), r0 * a0.sin(), z0);
            let p10 = geometry::Point3::new(r1 * a1.cos(), r1 * a1.sin(), z0);
            let p11 = geometry::Point3::new(r1 * a1.cos(), r1 * a1.sin(), z1);
            let p01 = geometry::Point3::new(r0 * a0.cos(), r0 * a0.sin(), z1);
            tris.push(Triangle::new(p00, p10, p11));
            tris.push(Triangle::new(p00, p11, p01));
        }
    }
    let spun = placed_mesh_to_spun_profile(&tris)
        .expect("40-band tapered mesh simplifies within MAX_POINTS");
    assert!(spun.points().len() <= camcraft_app::spun::MAX_POINTS);
    let dxf = spun_to_dxf(&spun).expect("exports to DXF");
    let reparsed = dxf_to_spun(dxf.as_bytes()).expect("re-imports");
    for p in sample_triangles(&tris) {
        let rho = p[0].hypot(p[1]);
        let r_env = profile_radius_at(&reparsed, p[2]);
        assert!(
            rho <= r_env + 1e-6,
            "sample z={} rho={rho} > r_env={r_env}",
            p[2]
        );
    }
}

#[test]
fn step_conversion_refuses_missing_or_ambiguous_axis_by_name() {
    let err_none = step_to_dxf(&testdata("box.step"))
        .expect_err("STEP without ISO 13399 CSYS must be refused, never guessed as +Z");
    assert!(
        err_none.contains("coordinate system")
            || err_none.contains("CIP")
            || err_none.contains("ISO 13399"),
        "{err_none}"
    );

    // Derive a valid closed-shell STEP whose ISO 13399 CIP placement is duplicated & conflicting
    // (removing PCS/MCS/CSW1_2 so ambiguous CIP is not rescued by another valid role).
    let src = std::fs::read_to_string(testdata("tool_csys_box_iso13399.step")).expect("fixture");
    let ambig = src
        .replace("AXIS2_PLACEMENT_3D('PCS',", "AXIS2_PLACEMENT_3D('NONE',")
        .replace("AXIS2_PLACEMENT_3D('MCS',", "AXIS2_PLACEMENT_3D('NONE',")
        .replace("AXIS2_PLACEMENT_3D('CSW1_2',", "AXIS2_PLACEMENT_3D('NONE',")
        .replace(
            "#220 = AXIS2_PLACEMENT_3D('CIP', #221, #202, #203);",
            "#220 = AXIS2_PLACEMENT_3D('CIP', #221, #202, #203);\n#222 = AXIS2_PLACEMENT_3D('CIP', #201, #202, #203);",
        );
    let tmp = std::env::temp_dir().join(format!("camcraft_ambig_csys_{}.step", std::process::id()));
    std::fs::write(&tmp, ambig).expect("write ambig step");
    let err_ambig = step_to_spun_profile(&tmp);
    let _ = std::fs::remove_file(&tmp);
    let err_ambig = err_ambig.expect_err("ambiguous CIP STEP must be refused by name");
    assert!(
        err_ambig.contains("refused")
            || err_ambig.contains("coordinate system")
            || err_ambig.contains("2 different placements"),
        "{err_ambig}"
    );

    let err_stl = step_to_dxf(&testdata("cube.stl"))
        .expect_err("non-STEP STL without ISO 13399 CSYS must be refused by name");
    assert!(!err_stl.is_empty());
}

#[test]
fn dxf_half_profile_import_with_lines_and_arcs_lands_as_tool_body() {
    let tool = catalog_tool(TOOL);
    let z0 = tool.flute_len + tool.shank_len;
    // Convex and concave arcs in one half-profile.
    let pts = vec![
        SpunPoint::line(z0, 0.0),
        SpunPoint {
            z: z0,
            r: 10.0,
            arc: 0.0,
        },
        SpunPoint {
            z: z0 + 20.0,
            r: 18.0,
            arc: 30.0,
        }, // convex
        SpunPoint {
            z: z0 + 40.0,
            r: 24.0,
            arc: -40.0,
        }, // concave
        SpunPoint {
            z: z0 + 55.0,
            r: 24.0,
            arc: 0.0,
        },
        SpunPoint::line(z0 + 55.0, 0.0),
    ];
    let profile = SpunProfile::new(pts.clone()).expect("valid spun profile");
    let dxf = spun_to_dxf(&profile).expect("exports DXF");

    let (world, mut history) = world_with_tool();
    let cmd = SetToolBody::from_dxf_bytes(&world, TOOL, "custom_holder.dxf", dxf.as_bytes(), None)
        .expect("DXF imports as SetToolBody");
    history.execute(&world, cmd);

    let entity = model::find_by_name(&world, TOOL).expect("tool entity");
    let body = tool_body::body_of(&world, &entity).expect("body landed");
    let landed = body.spun.as_ref().expect("spun profile stored");
    assert_eq!(landed.points().len(), profile.points().len());
    for (g, w) in landed.points().iter().zip(profile.points().iter()) {
        assert!((g.z - w.z).abs() < 1e-9);
        assert!((g.r - w.r).abs() < 1e-9);
        assert!((g.arc - w.arc).abs() < 1e-9);
    }

    // Undo / redo and project save/load preserve it.
    assert!(history.undo(&world));
    assert!(tool_body::body_of(&world, &entity).is_none());
    assert!(history.redo(&world));
    assert!(tool_body::body_of(&world, &entity).is_some());

    let tmp =
        std::env::temp_dir().join(format!("camcraft_dxf_body_{}.camproj", std::process::id()));
    project::save(&world, &tmp).expect("save");
    let world2 = new_world();
    project::load(&world2, &tmp).expect("load");
    let _ = std::fs::remove_file(&tmp);
    let loaded = tool_body::body_of(&world2, &model::find_by_name(&world2, TOOL).expect("tool"))
        .expect("body after reload");
    assert!(loaded.spun.is_some());
}

fn dxf_wrap(units: i32, entities: &str) -> String {
    format!(
        "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n{units}\n0\nENDSEC\n\
         0\nSECTION\n2\nENTITIES\n{entities}0\nENDSEC\n0\nEOF\n"
    )
}

#[test]
fn dxf_half_profile_reaching_flutes_is_refused_by_command() {
    // Reaches into flutes refused at SetToolBody
    let low_foot = dxf_wrap(
        4,
        "0\nLINE\n8\nCENTRELINE\n10\n0\n20\n0\n11\n0\n21\n60\n\
         0\nLINE\n8\nPROFILE\n10\n0\n20\n1\n11\n10\n21\n1\n\
         0\nLINE\n8\nPROFILE\n10\n10\n20\n1\n11\n10\n21\n30\n\
         0\nLINE\n8\nPROFILE\n10\n10\n20\n30\n11\n0\n21\n30\n",
    );
    let (world, _) = world_with_tool();
    match SetToolBody::from_dxf_bytes(&world, TOOL, "low.dxf", low_foot.as_bytes(), None) {
        Err(CommandError::Invalid(m)) => assert!(m.contains("flutes"), "{m}"),
        other => panic!("expected Invalid flutes refusal, got {other:?}"),
    }
}

#[test]
fn round_trip_step_to_dxf_to_body_to_dxf_is_stable_within_tolerance() {
    let step = testdata("tool_csys_box_iso13399.step");
    // Ensure a tool whose flute+shank is <= 35 mm so the box holder fits.
    let mut tool = catalog_tool(TOOL);
    tool.flute_len = 10.0;
    tool.shank_len = 20.0;
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));

    let dxf_1 = step_to_dxf(&step).expect("step -> dxf_1");
    let spun_1 = dxf_to_spun(dxf_1.as_bytes()).expect("dxf_1 -> spun_1");

    history.execute(
        &world,
        SetToolBody::from_dxf_bytes(&world, TOOL, "box.dxf", dxf_1.as_bytes(), None)
            .expect("dxf_1 -> body"),
    );
    let entity = model::find_by_name(&world, TOOL).expect("tool");
    let body_1 = tool_body::body_of(&world, &entity).expect("body landed");

    let dxf_2 = tool_body_to_dxf(&body_1).expect("body_1 -> dxf_2");
    let spun_2 = dxf_to_spun(dxf_2.as_bytes()).expect("dxf_2 -> spun_2");

    assert_eq!(spun_1.points().len(), spun_2.points().len());
    for (p1, p2) in spun_1.points().iter().zip(spun_2.points().iter()) {
        assert!((p1.z - p2.z).abs() < 1e-9, "z drift {} vs {}", p1.z, p2.z);
        assert!((p1.r - p2.r).abs() < 1e-9, "r drift {} vs {}", p1.r, p2.r);
        assert!(
            (p1.arc - p2.arc).abs() < 1e-9,
            "arc drift {} vs {}",
            p1.arc,
            p2.arc
        );
    }

    history.execute(
        &world,
        SetToolBody::from_dxf_bytes(&world, TOOL, "rt.dxf", dxf_2.as_bytes(), None)
            .expect("dxf_2 -> body_2"),
    );
    let body_2 = tool_body::body_of(&world, &entity).expect("body_2");
    let prof_1 = body_1.profile().expect("spindle profile 1");
    let prof_2 = body_2.profile().expect("spindle profile 2");
    assert_eq!(prof_1.bands().len(), prof_2.bands().len());
    for (b1, b2) in prof_1.bands().iter().zip(prof_2.bands().iter()) {
        assert!((b1.above - b2.above).abs() < 1e-9);
        assert!((b1.len - b2.len).abs() < 1e-9);
        assert!((b1.r - b2.r).abs() < 1e-9);
    }
    assert_eq!(dxf_1, dxf_2, "STEP -> DXF -> body -> DXF stays stable");
}
