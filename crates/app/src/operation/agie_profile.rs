//! Collect one nominal punch, die opening or straight open cutoff for the native wire EDM job,
//! before the wire generator offsets it.
//! Provenance: CAMCraft's Wire Contour geometry and named controller recipe.

use super::{FeatureSel, OpParams, SrcRef, WireSide, job};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;
use camcraft_geometry::wire_round_profile::WireProfile;
use crate::{model, posting::agie, schema::Operation, setup};
use flecs_ecs::prelude::World;

/// The checked operation: its side, collected job, stock bounds and work origin.
type Checked = (agie::AgieSide, job::GenJob, Point3, Point3, [f64; 3]);

/// The nominal job of `op`, which must cut the modeled workpiece `source`.
/// The caller chooses the source and checks that no other model is present.
/// A die plate must have exactly `openings` through-openings: one for a single
/// die, two for a die group (#2977).
pub(crate) fn agie_job(
    world: &World,
    op: &Operation,
    name: &str,
    source: &str,
    openings: usize,
) -> Result<agie::AgieJob, String> {
    let (side, job, lo, hi, [ox, oy, oz]) = checked(world, op, name, Some(source))?;
    let OpParams::WireContour(wire) = &op.params else {
        return Err("The native wire EDM job requires a Wire Contour".into());
    };
    if !job.material.bodies.is_empty() {
        return Err("The native wire EDM job requires one closed vertical prism spanning the stock height; cavities, steps and multiple bodies are unsupported".into());
    }
    // Both caps of the complete source model have the exact planar outline.
    // A union of sampled slices can add rounded vertices along a sloping face.
    let region = crate::geometry::closed_vertical_prism(&job.material.facets, lo.z, hi.z)
        .map_err(|why| format!("The native wire EDM job requires one closed vertical prism spanning the stock height: {why}"))?;
    let points = match side {
        agie::AgieSide::Punch => {
            let [ring] = region.loops() else {
                return Err("The native wire EDM job requires exactly one closed punch boundary, without openings or additional contours".into());
            };
            ring.points()
        }
        agie::AgieSide::Die => {
            super::agie_die::die_opening(&region, &job, [lo.x, lo.y], [hi.x, hi.y], openings)?
                .points()
        }
        agie::AgieSide::Left | agie::AgieSide::Right => {
            cutoff_edge(&region, &job, side, [lo.x, lo.y], [hi.x, hi.y])?
        }
    };
    Ok(agie::AgieJob {
        side,
        contour: points.iter().map(|p| [p.x - ox, p.y - oy]).collect(),
        start: wire
            .picks
            .first(crate::path_picks::PickRole::Start)
            .map(|p| [p.xy.x - ox, p.xy.y - oy]),
        stock_min: [lo.x - ox, lo.y - oy, lo.z - oz],
        stock_max: [hi.x - ox, hi.y - oy, hi.z - oz],
    })
}

/// The nominal punch of `op` on one selected DXF circle, racetrack (#2022) or
/// convex tangent loop (#2983), and that shape in setup XY. The selected-chain
/// door has refused models, fixtures and other drawings in the setup.
pub(crate) fn agie_sketch_job(
    world: &World,
    op: &Operation,
    name: &str,
) -> Result<(agie::AgieJob, WireProfile), String> {
    if !matches!(&op.params, OpParams::WireContour(w) if w.side == WireSide::Punch) {
        return Err("The native wire EDM job reads a selected DXF chain only for a captured punch".into());
    }
    let (side, job, lo, hi, [ox, oy, oz]) = checked(world, op, name, None)?;
    let (OpParams::WireContour(wire), Some(chain)) = (&op.params, job.wire_sketch.as_ref()) else {
        return Err("Select one closed DXF chain for the native wire EDM punch".into());
    };
    let start = wire
        .picks
        .first(crate::path_picks::PickRole::Start)
        .ok_or("The DXF punch requires a predrilled 2 mm threading hole")?;
    let start = [start.xy.x, start.xy.y];
    let mut shape = chain.wire_profile()?;
    // The rest of the drawing must clear the whole native work: the arc
    // extrema with the rough pass and wire (as the post bounds them), and the
    // complete threading hole. The box of the two holds the straight lead.
    let rough = camcraft_post::agie_setup::qn2_offsets_um(20_000)
        .ok_or("The QN2 recipe has no measured offset for 20 mm stock")?[0];
    let reach = f64::from((u32::from(rough) + 125).next_multiple_of(100)) / 1000.0;
    let [a, b] = shape.bounds();
    let low = Point2::new((a.x - reach).min(start[0] - 1.0), (a.y - reach).min(start[1] - 1.0));
    let high = Point2::new((b.x + reach).max(start[0] + 1.0), (b.y + reach).max(start[1] + 1.0));
    chain.check_envelope(low, high)?;
    shape.place(|p| Point2::new(p.x - ox, p.y - oy));
    let job = agie::AgieJob {
        side,
        contour: Vec::new(),
        start: Some([start[0] - ox, start[1] - oy]),
        stock_min: [lo.x - ox, lo.y - oy, lo.z - oz],
        stock_max: [hi.x - ox, hi.y - oy, hi.z - oz],
    };
    Ok((job, shape))
}

/// The work quality that the condition of `op` requests (#2966). The job
/// check refuses any other condition first.
pub(crate) fn agie_quality(world: &World, op: &Operation) -> Result<agie::Quality, String> {
    let OpParams::WireContour(wire) = &op.params else {
        return Err("The native wire EDM job requires a Wire Contour".into());
    };
    let condition = super::wire_op::condition_of(world, op.setup.as_deref(), &op.tool, wire)?;
    agie::quality_of(&condition.id).ok_or_else(|| {
        format!(
            "The machine condition {} requests no native work quality",
            condition.id
        )
    })
}

/// Check the operation, its machine condition, setup and stock for a native
/// job. `None` for `source` is the selected DXF chain of a punch (#2022).
fn checked(world: &World, op: &Operation, name: &str, source: Option<&str>) -> Result<Checked, String> {
    let OpParams::WireContour(wire) = &op.params else {
        return Err("The native wire EDM job requires a Wire Contour".into());
    };
    if wire.taper_deg != 0.0
        || wire.tabs.is_some()
        || wire.burnout.is_some()
        || !wire.skims.is_empty()
        || wire.four_axis.is_some()
    {
        return Err("The native wire EDM job supports one captured vertical punch, die opening or clamped open cutoff. Disable taper, four-axis, tabs, burnout and requested skims; the controller selects trim passes".into());
    }
    let side = match (&wire.side, &op.src) {
        (WireSide::Punch, SrcRef::Entity(_)) => agie::AgieSide::Punch,
        (WireSide::Punch, SrcRef::Selection(_)) if source.is_none() => agie::AgieSide::Punch,
        (WireSide::Die, SrcRef::Selection(_)) => agie::AgieSide::Die,
        (WireSide::Left, SrcRef::Selection(_)) => agie::AgieSide::Left,
        (WireSide::Right, SrcRef::Selection(_)) => agie::AgieSide::Right,
        _ => return Err("The native wire EDM job requires the complete punch model, the selected rim of a through-opening in the retained die plate, or one selected cutoff edge".into()),
    };
    // An open cutoff severs both pieces before the trims (#2962). A closed
    // cycle keeps its captured slug and its drilled start hole.
    if side.is_open() {
        if !wire.both_pieces_restrained || !wire.stock_centered_in_xy_travel {
            return Err("The open cutoff requires both pieces clamped through all passes and the stock centered in the installed XY travel".into());
        }
        if wire.picks.first(crate::path_picks::PickRole::Start).is_some() || !(0.0..=3.6).contains(&wire.clearance) {
            return Err("The open cutoff requires air threading and at most 3.6 mm run-out clearance for its 4 mm tails".into());
        }
    } else if !wire.capture || wire.picks.first(crate::path_picks::PickRole::Start).is_none() {
        return Err("The native wire EDM job requires a captured closed contour and a predrilled 2 mm start hole on the waste side".into());
    }
    let on_source = match (&op.src, source) {
        (_, None) => super::wire_sketch::selected(&op.src, &op.params),
        (SrcRef::Entity(name), Some(source)) => source == name,
        (SrcRef::Selection(picks), Some(source)) => {
            !picks.is_empty()
                && picks.iter().all(|pick| {
                    pick.entity == source && matches!(pick.feature, FeatureSel::Chain(_))
                })
        }
        (SrcRef::Stock, Some(_)) => false,
    };
    if !on_source {
        return Err("The native wire EDM job requires the complete punch, or the edge chains of one through-opening in the retained die plate".into());
    }
    let setup = op.setup.as_deref();
    let condition = super::wire_op::condition_of(world, setup, &op.tool, wire)?;
    if agie::quality_of(&condition.id).is_none()
        || (condition.wire_diameter - 0.25).abs() > 1e-9
        || (condition.gap - agie::PREVIEW_GAP).abs() > 1e-9
    {
        return Err("Select AGIE_QN2 (Fine Ra 0.30 µm) or AGIE_BERCO_RA035 (Fine+ Ra 0.35 µm, one 20 mm punch) with its unmodified preview gap for cold die steel 0001 and Berco Cut 0.25 mm".into());
    }
    if crate::stock_material::in_force(world, setup).is_some() {
        return Err("The stated stock material conflicts with AGIE_QN2 cold die steel 0001".into());
    }
    let edm = crate::machine::of_setup(world, setup)
        .1
        .and_then(|m| m.edm)
        .ok_or("The native wire EDM job requires a mounted wire EDM profile")?;
    let row = edm.condition(&condition.machine, &condition.id, &[])?;
    if row.role != "rough" || !row.wire_diameter.is_some_and(|d| (d - 0.25).abs() <= 1e-9) {
        return Err(format!(
            "The native wire EDM job requires the {} machine condition to state role rough and wire diameter 0.25 mm",
            condition.id
        ));
    }
    if edm.flushing.is_some() || edm.skim_flushing.is_some() || edm.tank.is_some() {
        return Err("The AGIE_QN2 condition controls the dielectric settings; explicit tank or flushing modes are unsupported".into());
    }
    if op.after != super::AfterOperation::None
        || op.coolant != super::CoolantMode::default()
        || crate::program_entry::sequence(world).iter().any(|(_, e)| {
            crate::program_entry::entry_of(world, e).is_some_and(|entry| !entry.suppressed)
        })
    {
        return Err(
            "Native wire EDM jobs do not support additional stops, homing, or coolant overrides".into(),
        );
    }
    if setup::wcs_slot(world, setup) != 0
        || crate::setup_wcs_flip::of_setup(world, setup) != [false; 3]
        || setup::wcs_entity(world, setup)
            .and_then(|e| model::transform_of(world, &e))
            .is_some_and(|p| !crate::setup_align::is_identity_rot(p.rot))
        || crate::setup_align::alignment(world, setup).is_some()
        || setup::effective(world, setup).is_some_and(|s| {
            crate::setup_relative::relative_to(world, &s).is_some()
                || crate::setup_stock::effective(world, &s)
                    == Some(crate::setup_stock::StockSource::FromPrevious)
        })
    {
        return Err(
            "The native wire EDM job requires one unrotated, unflipped setup with its own stock and first work offset"
                .into(),
        );
    }
    let stock = setup::stock_entity(world, setup).ok_or("The native wire EDM job requires rectangular box stock")?;
    if !matches!(
        model::stock_def_of(world, &stock),
        Some(crate::stock::StockShape::Box { .. })
    ) || model::transform_of(world, &stock)
        .is_some_and(|p| !crate::setup_align::is_identity_rot(p.rot))
    {
        return Err("The native wire EDM job requires axis-aligned rectangular box stock".into());
    }
    let job = job::collect_job(world, op, name).map_err(|e| e.to_string())?;
    if !job.fixtures.is_empty() || !job.unread_steel.is_empty() {
        return Err("The native wire EDM post cannot verify controller-generated leads against fixtures".into());
    }
    let (lo, hi) = job.stock_aabb.ok_or("The native wire EDM job requires finite stock bounds")?;
    let [ox, oy, oz] = job.wcs_origin;
    if job.hs.top != Some(hi.z) || job.hs.bottom != Some(lo.z) || oz != hi.z {
        return Err("The native wire EDM job requires a through cut from stock top to bottom and the setup Z origin at stock top".into());
    }
    // Guide heights and posted stock Z are in the setup frame (stock top Z0).
    if edm.lower_guide_z.is_some_and(|z| z > lo.z - oz)
        || edm.upper_guide_z.is_some_and(|z| z < 0.0)
    {
        return Err("The wire work extends outside the stated wire guide heights".into());
    }
    Ok((side, job, lo, hi, [ox, oy, oz]))
}

/// The one selected straight wall of the retained prism for an open cutoff.
/// The whole retained cap must lie on the side away from the kerf, and the
/// model must lie inside the stock.
fn cutoff_edge<'j>(
    region: &Region,
    job: &'j job::GenJob,
    side: agie::AgieSide,
    lo: [f64; 2],
    hi: [f64; 2],
) -> Result<&'j [crate::geo2d::Point2], String> {
    let (rings, open, _) = job
        .contour_sel
        .as_ref()
        .ok_or("Select one retained cutoff edge")?
        .as_ref()
        .map_err(ToString::to_string)?;
    let [chain] = open.as_slice() else {
        return Err("The open cutoff requires exactly one open cutoff edge".into());
    };
    let [a, b] = chain.as_slice() else {
        return Err("The open cutoff requires a single straight edge".into());
    };
    let [body] = region.loops() else {
        return Err("The open cutoff requires one through prism without holes".into());
    };
    let vertices = body.points();
    let has_edge = vertices
        .iter()
        .zip(vertices.iter().cycle().skip(1))
        .any(|(p, q)| (p == a && q == b) || (p == b && q == a));
    let waste = if side == agie::AgieSide::Left { 1. } else { -1. };
    if !rings.is_empty()
        || !has_edge
        || vertices.iter().any(|p| {
            p.x < lo[0]
                || p.x > hi[0]
                || p.y < lo[1]
                || p.y > hi[1]
                || waste * ((b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)) > 0.
        })
    {
        return Err("The selected cutoff must span a full retained wall with the kerf on the waste side and the model within stock".into());
    }
    Ok(chain.as_slice())
}
