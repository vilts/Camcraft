//! Transfer one placed DXF wire opening for controller inspection, from CamCraft's setup and selection contracts.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use camcraft_geometry::dxf::Primitive;
use camcraft_geometry::sketch_chains::{ChainClearance, ChainError};
use camcraft_post::agie_setup::{INSPECTION_RECEIPT, InspectionProfile, InspectionSegment};
use flecs_ecs::prelude::World;
use serde::Serialize;

use super::{OpParams, WireSide, resolve_params, wire_sketch};
use crate::geo2d::Point2;
use crate::geometry::{Point3, quat_conjugate, quat_rotate_point, transform_point};
use crate::{model, setup};

#[derive(Serialize)]
pub struct AgieInspectionReceipt {
    pub operation: String,
    pub setup: String,
    pub chain: String,
    pub nominal_blocks: usize,
    pub files: Vec<PathBuf>,
    pub review: &'static str,
}

fn exact_repeat(a: &Primitive, b: &Primitive) -> bool {
    if a == b {
        return true;
    }
    match (a, b) {
        (Primitive::Line { start: a, end: b }, Primitive::Line { start: c, end: d }) => {
            a == d && b == c
        }
        _ => false,
    }
}

/// Cheap eligibility for a selected-geometry export action, independent of cached machining results.
pub fn has_agie_inspection_source(op: &crate::schema::Operation) -> bool {
    !op.suppressed
        && matches!(&op.params, OpParams::WireContour(w) if w.side == WireSide::Die)
        && matches!(&op.src, super::SrcRef::Selection(items) if matches!(items.as_slice(), [g] if matches!(g.feature, super::FeatureSel::SketchChain(_))))
}

pub fn export_agie_inspection(
    world: &World,
    name: &str,
    dest: &Path,
) -> Result<AgieInspectionReceipt, String> {
    let op = model::find_by_name(world, name)
        .and_then(|e| model::operation_of(world, &e))
        .ok_or_else(|| format!("no operation named '{name}'"))?;
    let (opening, setup_name, chain) = prepare_inspection(world, &op)?;
    let nominal_blocks = opening.len();
    let files = opening.export(dest)?;
    Ok(AgieInspectionReceipt {
        operation: name.into(),
        setup: setup_name,
        chain,
        nominal_blocks,
        files,
        review: INSPECTION_RECEIPT,
    })
}

pub(crate) fn prepare_inspection(
    world: &World,
    op: &crate::schema::Operation,
) -> Result<(InspectionProfile, String, String), String> {
    let OpParams::WireContour(wire) = &op.params else {
        return Err("select a Wire Contour for Agie inspection".into());
    };
    if op.suppressed
        || wire.side != WireSide::Die
        || wire.taper_deg != 0.0
        || wire.four_axis.is_some()
        || wire.tabs.is_some()
        || wire.burnout.is_some()
        || !wire.skims.is_empty()
    {
        return Err("Agie inspection requires one active, vertical die opening without tabs, burnout or skims".into());
    }
    let declared = op.setup.as_deref();
    let setup_name = setup::effective(world, declared);
    let machine = crate::setup_mount::machine_of(world, declared)
        .1
        .filter(|m| {
            m.model == "AGIECUT CLASSIC 2S"
                && crate::machine::kind(Some(m)) == crate::machine::Kind::WireEdm
        })
        .ok_or(
            "mount the AGIECUT CLASSIC 2S template on the operation’s setup for nominal inspection",
        )?;
    if declared.is_some_and(|n| !setup::exists(world, n))
        || setup::wcs_slot(world, declared) != 0
        || crate::setup_wcs_flip::of_setup(world, declared) != [false; 3]
        || crate::setup_align::alignment(world, declared).is_some()
        || setup_name.as_deref().is_some_and(|s| {
            crate::setup_relative::relative_to(world, s).is_some()
                || crate::setup_stock::effective(world, s)
                    == Some(crate::setup_stock::StockSource::FromPrevious)
        })
    {
        return Err(
            "Agie inspection needs an unflipped, independent first work offset with its own stock"
                .into(),
        );
    }
    let diameter = super::wire_op::wire_diameter_of(world, &op.tool)?;
    if (diameter - 0.25).abs() > 1e-9 {
        return Err("Agie inspection uses a 0.25 mm wire tool".into());
    }
    if crate::stock_material::in_force(world, declared).is_some() {
        return Err("the stated project material cannot be matched to Agie steel 0001".into());
    }
    let heights = resolve_params(world, declared, Some(&op.src), &op.params)?;
    let selected = wire_sketch::resolve_geometry(world, op, &heights, None)?;
    let (_, [ox, oy, oz]) = crate::posting::wcs_context(world, declared);
    if oz != selected.stock.max.z
        || (selected.stock.max.z - selected.stock.min.z - 10.0).abs() > 1e-9
    {
        return Err(
            "Agie inspection needs 10 mm through stock with work Z0 on its top face".into(),
        );
    }
    let hole_pick = wire
        .picks
        .first(crate::path_picks::PickRole::Start)
        .ok_or("state the centre of the 3 mm threading hole")?;
    let hole = Point2::new(hole_pick.xy.x - ox, hole_pick.xy.y - oy);
    let pose = selected.pose;
    let place = |p: Point2| {
        let q = transform_point(Point3::new(p.x, p.y, 0.0), pose.pos, pose.rot);
        Point2::new(q.x - ox, q.y - oy)
    };
    let yaw = 2.0 * pose.rot[2].atan2(pose.rot[3]);
    let placed_curve = |p: &Primitive| match *p {
        Primitive::Line { start, end } => Primitive::Line {
            start: place(start),
            end: place(end),
        },
        Primitive::Arc {
            center,
            radius,
            start_angle,
            sweep_angle,
        } => Primitive::Arc {
            center: place(center),
            radius,
            start_angle: start_angle + yaw,
            sweep_angle,
        },
        Primitive::Circle { center, radius } => Primitive::Circle {
            center: place(center),
            radius,
        },
    };
    let ring = &selected.chain;
    ring.nesting.as_ref().map_err(|e| e.to_string())?;
    if ring.max_gap > 1e-8 {
        return Err("the selected opening has an unresolved joint gap".into());
    }
    let local_hole = quat_rotate_point(
        quat_conjugate(pose.rot),
        Point3::new(hole_pick.xy.x - pose.pos.x, hole_pick.xy.y - pose.pos.y, 0.0),
    );
    let mut source_clearance = ChainClearance::new(ring).map_err(|e| e.to_string())?;
    if !source_clearance.contains_disk(Point2::new(local_hole.x, local_hole.y), 1.5 + 1e-8) {
        return Err(
            "the full 3 mm threading hole must lie inside the selected nominal profile".into(),
        );
    }
    let mut spans = Vec::new();
    for seg in &ring.segments {
        let (a, b) = (place(seg.start), place(seg.end));
        let arc = match seg.curve {
            Primitive::Line { .. } => None,
            Primitive::Arc {
                center,
                sweep_angle,
                ..
            } => Some((place(center), sweep_angle)),
            Primitive::Circle { center, radius } => {
                let mid = place(Point2::new(center.x - radius, center.y));
                let half = Some((place(center), std::f64::consts::PI));
                spans.push(InspectionSegment {
                    start: a,
                    end: mid,
                    arc: half,
                });
                spans.push(InspectionSegment {
                    start: mid,
                    end: a,
                    arc: half,
                });
                continue;
            }
        };
        spans.push(InspectionSegment {
            start: a,
            end: b,
            arc,
        });
    }
    let mut profile = InspectionProfile::new(&spans, hole)?;
    profile.check_machine(&machine)?;
    let data = selected.data.primitives();
    let owned: HashSet<_> = ring.segments.iter().map(|s| s.primitive).collect();
    let mut used = owned.clone();
    for problem in &selected.data.chains().rejected {
        if let ChainError::DuplicatePrimitive { primitive, of } = problem.error
            && owned.contains(&of)
            && let (Some(a), Some(b)) = (data.get(primitive), data.get(of))
            && a.layer == b.layer
            && exact_repeat(&a.primitive, &b.primitive)
        {
            used.insert(primitive);
        }
    }
    for (i, item) in data.iter().enumerate().filter(|(i, _)| !used.contains(i)) {
        if source_clearance
            .obstructs(i, &item.primitive, 1e-8)
            .map_err(|e| e.to_string())?
        {
            return Err(format!(
                "unselected DXF primitive {} touches or enters the source opening",
                i + 1
            ));
        }
        profile.check_other(i, &placed_curve(&item.primitive))?;
    }
    Ok((
        profile,
        setup_name.unwrap_or_else(|| "Document".into()),
        ring.id.to_string(),
    ))
}
