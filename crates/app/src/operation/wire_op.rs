//! The Wire Contour operation's seed, command door and generation
//! (#532, part of #290): the op's condition id read against the
//! machine its setup mounts (#531's door - the wire diameter and the
//! spark gap have no other source), the picked profile or the model's
//! through-silhouette as the geometry, and `wire_edm::wire_contour`
//! run over it.
//!
//! Provenance: our own glue over the motion crate's wire generator
//! (whose module head cites the public sources) and the machine file's
//! condition table (#531); the umbrella's decisions D2, D4 and D8.

use flecs_ecs::prelude::World;

use crate::cl::Toolpath;
use crate::geo2d::{Loop, Point2, Region};
use crate::machine::{ConditionField, Kind};
use crate::machining2d::GenError;
use crate::verify::Warning;
use crate::wire_edm::{
    check_passes, clearance_door, wire_burnout, wire_passes, TabParams, WireCorner, WireParams, WirePass, WireTaper,
};

use super::kinds::Carried;
use super::types::{OpParams, WireContourOp, WireSide};
use super::ResolvedHeights;

/// The wire draft: the carried workpiece planes, the die side, and
/// neither a condition nor a start hole - both are the user's to state
/// (a draft that guessed either would be an inference).
pub(super) fn seed_wire_contour(c: &Carried) -> OpParams {
    OpParams::WireContour(WireContourOp {
        top_z: c.top,
        bottom_z: c.bottom,
        condition: String::new(),
        side: WireSide::Die,
        picks: crate::path_picks::PathPicks::default(),
        tabs: None,
        capture: false,
        both_pieces_restrained: false,
        stock_centered_in_xy_travel: false,
        taper_deg: 0.0,
        four_axis: None,
        burnout: None,
        clearance: super::types::DEFAULT_CLEARANCE,
        skims: Vec::new(),
        sharp_corners: false,
        lead_arc_radius: None,
    })
}

/// What the machine's condition states for this cut.
#[derive(Clone, Debug, PartialEq)]
pub struct WireCondition {
    pub id: String,
    /// The wire's diameter, mm.
    pub wire_diameter: f64,
    /// The spark gap per side, mm.
    pub gap: f64,
    /// The skim passes after this (the rough) pass, each resolved from
    /// its own condition (#538); empty is the rough pass alone.
    pub skims: Vec<WirePass>,
    /// The machine's name and its stated maximum taper (#536), for the
    /// taper's door.
    pub machine: String,
    pub max_taper_deg: Option<f64>,
    /// The machine's angle-word taper dialect (#1164), as stated;
    /// `None` = it tapers by U/V moves.
    pub taper_angle: Option<crate::machine::TaperAngle>,
}

/// Read the wire diameter in mm from the mounted [`ToolKind::Wire`] tool.
///
/// # Errors
/// No wire tool mounted, the named entity is not a wire tool, or its wire
/// definition violates [`camcraft_tools::wire::refusal`].
pub fn wire_diameter_of(world: &World, tool_name: &str) -> Result<f64, String> {
    if tool_name.trim().is_empty() {
        return Err("a Wire Contour needs a wire tool mounted from the library".into());
    }
    let Some(entity) = crate::model::find_by_name(world, tool_name) else {
        return Err(format!("a Wire Contour's tool '{tool_name}' is not in the library"));
    };
    let Some(tool) = crate::model::tool_of(world, &entity) else {
        return Err(format!("'{tool_name}' is not a tool"));
    };
    if tool.kind != crate::tools::ToolKind::Wire {
        return Err(format!("tool '{tool_name}' is a {}; a Wire Contour cuts with a wire", tool.kind));
    }
    if let Some(why) = camcraft_tools::wire::refusal(&tool) {
        return Err(format!("tool '{tool_name}': {why}"));
    }
    Ok(tool.diameter)
}

/// The condition door (#531, #532, #2011): the op's mounted wire tool and
/// its condition on the setup's wire EDM machine, with the spark gap stated
/// and smaller than the wire's radius, matching `edm.wire_diameters` and any
/// condition-stated `wire_diameter` within [`crate::machine::WIRE_MATCH_MM`].
///
/// # Errors
/// Missing/invalid wire tool, no wire EDM mounted, wire diameter mismatch,
/// missing condition/gap, or gap >= wire radius.
pub fn condition_of(world: &World, setup: Option<&str>, tool_name: &str, op: &WireContourOp) -> Result<WireCondition, String> {
    let wire_diameter = wire_diameter_of(world, tool_name)?;
    let (_, mounted) = crate::machine::of_setup(world, setup);
    let Some(machine) = mounted.filter(|m| crate::machine::kind(Some(m)) == Kind::WireEdm) else {
        return Err(
            "a Wire Contour needs a wire EDM machine mounted on its setup - the machine's EDM conditions state the spark gap".into(),
        );
    };
    if let Some(edm) = machine.edm.as_ref()
        && !edm.wire_diameters.is_empty()
        && !edm.wire_diameters.iter().any(|w| (*w - wire_diameter).abs() <= crate::machine::WIRE_MATCH_MM)
    {
        let list: Vec<String> = edm.wire_diameters.iter().map(|w| format!("{w}")).collect();
        return Err(format!(
            "tool '{tool_name}' wire diameter {wire_diameter} mm is not one of machine {}'s edm.wire_diameters ({} mm)",
            machine.model,
            list.join(", ")
        ));
    }
    let (id, gap) = one_condition(&machine, &op.condition, tool_name, wire_diameter, "a Wire Contour")?;
    let mut skims = Vec::with_capacity(op.skims.len());
    for (i, s) in op.skims.iter().enumerate() {
        let who = format!("skim {} of the Wire Contour", i + 1);
        let (condition, gap) =
            one_condition(&machine, &s.condition, tool_name, wire_diameter, &who).map_err(|e| if e.starts_with(&who) { e } else { format!("{who}: {e}") })?;
        skims.push(WirePass { condition, wire_diameter, gap, reverse: s.reverse });
    }
    let max_taper_deg = machine.edm.as_ref().and_then(|e| e.max_taper_deg);
    let taper_angle = machine.edm.as_ref().and_then(|e| e.taper_angle);
    let c = WireCondition { id, wire_diameter, gap, skims, machine: machine.model.clone(), max_taper_deg, taper_angle };
    // The burnout's clearance against the condition's kerf (#540).
    if let Some(clear) = op.burnout {
        clearance_door(clear, c.wire_diameter * 0.5 + c.gap)?;
    }
    // Skims with a taper, a burnout, tabs or a 4-axis cut (#538 x #536,
    // #540, #539, #537): refused by name at the door, never cut as
    // something else.
    if let Some(why) = skims_refusal(op) {
        return Err(why);
    }
    // The passes' offset laws (#538) are the machine's numbers alone,
    // so they stand at the door too, not first in the generator.
    let rough = WireParams {
        wire_diameter: c.wire_diameter,
        gap: c.gap,
        condition: c.id.clone(),
        side: op.side,
        start: None,
        plane_z: 0.0,
        tabs: None,
        stock: None,
        clearance: op.clearance,
        sharp_corners: false,
    };
    check_passes(&rough, &c.skims).map_err(|e| match e {
        GenError::Invalid(why) => why,
        e => e.to_string(),
    })?;
    Ok(c)
}

/// One condition `id` on `machine`, with its gap stated and matching the
/// mounted wire diameter (`c.wire_diameter` within `WIRE_MATCH_MM` when
/// stated, and `gap < wire_diameter * 0.5`).
fn one_condition(
    machine: &crate::machine::Machine,
    id: &str,
    tool_name: &str,
    wire_diameter: f64,
    who: &str,
) -> Result<(String, f64), String> {
    let stated = || match machine.edm.as_ref().map(|e| e.conditions.as_slice()) {
        None | Some([]) => "it states none - add edm.conditions to the machine file".to_owned(),
        Some(cs) => format!("it states {}", cs.iter().map(|c| c.id.as_str()).collect::<Vec<_>>().join(", ")),
    };
    if id.trim().is_empty() {
        return Err(format!("{who} names no EDM condition - choose one of machine {}'s conditions ({})", machine.model, stated()));
    }
    let Some(edm) = machine.edm.as_ref() else {
        return Err(format!("machine {} states no EDM condition {id:?} ({})", machine.model, stated()));
    };
    let known = edm.conditions.iter().any(|c| c.id == id);
    let c = edm
        .condition(&machine.model, id, &[ConditionField::Gap])
        .map_err(|e| if known { e } else { format!("{e} ({})", stated()) })?;
    let Some(gap) = c.gap else {
        return Err(format!("machine {}: EDM condition {:?} states no gap", machine.model, c.id));
    };
    if let Some(d) = c.wire_diameter
        && (d - wire_diameter).abs() > crate::machine::WIRE_MATCH_MM
    {
        return Err(format!(
            "machine {}: EDM condition {:?} expects wire diameter {d} mm, but tool '{tool_name}' is {wire_diameter} mm",
            machine.model, c.id
        ));
    }
    if gap >= wire_diameter * 0.5 {
        return Err(format!(
            "machine {}: EDM condition {:?} spark gap {gap} mm must be smaller than tool '{tool_name}' wire radius ({} mm)",
            machine.model,
            c.id,
            wire_diameter * 0.5
        ));
    }
    Ok((c.id.clone(), gap))
}

/// The condition a wire EDM kind's generation cuts with, validated against
/// the mounted library tool (`op.tool`).
///
/// # Errors
/// [`condition_of`]'s refusals, as a generation refusal.
pub(super) fn job_wire(world: &World, op: &crate::schema::Operation) -> Result<Option<(WireCondition, Option<WireCondition>)>, GenError> {
    match &op.params {
        OpParams::WireContour(w) => {
            let setup = op.setup.as_deref();
            let main = condition_of(world, setup, &op.tool, w).map_err(GenError::Invalid)?;
            let cutoff = cutoff_of(world, setup, &op.tool, w).map_err(GenError::Invalid)?;
            Ok(Some((main, cutoff)))
        }
        _ => Ok(None),
    }
}

/// The tab cut-off's condition (#539): `None` when tabs are off or no
/// cut-off condition is named (the generator refuses the latter by
/// name); a named one the machine does not state or incompatible with
/// the wire tool is refused here.
///
/// # Errors
/// The condition lookup's refusal, by name.
pub fn cutoff_of(world: &World, setup: Option<&str>, tool_name: &str, op: &WireContourOp) -> Result<Option<WireCondition>, String> {
    let Some(t) = op.tabs.as_ref().filter(|t| !t.cutoff_condition.trim().is_empty()) else {
        return Ok(None);
    };
    // The probe is the cut-off's condition alone: the skims' laws read
    // the rough pass, not the cut-off.
    let probe = WireContourOp { condition: t.cutoff_condition.clone(), skims: Vec::new(), ..op.clone() };
    condition_of(world, setup, tool_name, &probe)
        .map(Some)
        .map_err(|e| format!("the tab cut-off's condition: {e}"))
}

/// The condition's share of the operation's stamp (#532, #2011): its id,
/// wire diameter and gap as the machine and mounted wire tool state them
/// now - or one byte for "not stated", which no generated path's stamp carries.
pub(super) fn stamp_condition(world: &World, setup: Option<&str>, tool_name: &str, op: &WireContourOp, h: &mut std::hash::DefaultHasher) {
    use std::hash::Hasher;
    h.write(b"wire-condition");
    match condition_of(world, setup, tool_name, op) {
        Ok(c) => {
            h.write_u8(1);
            h.write(c.id.as_bytes());
            h.write_u8(0xff);
            h.write(&c.wire_diameter.to_bits().to_le_bytes());
            h.write(&c.gap.to_bits().to_le_bytes());
            // #538: each skim's condition as the machine states it now.
            // Nothing is written for no skims, so a rough-only cut's
            // stamp is the one it always was.
            for s in &c.skims {
                h.write(b"skim");
                h.write(s.condition.as_bytes());
                h.write_u8(0xff);
                h.write(&s.wire_diameter.to_bits().to_le_bytes());
                h.write(&s.gap.to_bits().to_le_bytes());
                h.write_u8(u8::from(s.reverse));
            }
            // #536: a taper also reads the machine's maximum; a vertical
            // cut's stamp stays what it was.
            if op.taper_deg != 0.0 || op.four_axis.is_some() {
                h.write(&c.max_taper_deg.map_or(u64::MAX, f64::to_bits).to_le_bytes());
                // #1164: an angle-word machine's rules shape the path;
                // a U/V machine's stamp stays what it was.
                if let Some(a) = c.taper_angle {
                    h.write(b"taper-angle");
                    h.write(format!("{:?}{:?}", a.positive, a.corner).as_bytes());
                }
            }
        }
        Err(_) => h.write_u8(0),
    }
    // #539: the cut-off's condition, only when tabs are on.
    if op.tabs.is_some() {
        h.write(b"wire-cutoff");
        match cutoff_of(world, setup, tool_name, op) {
            Ok(Some(c)) => {
                h.write_u8(1);
                h.write(&c.wire_diameter.to_bits().to_le_bytes());
                h.write(&c.gap.to_bits().to_le_bytes());
            }
            Ok(None) | Err(_) => h.write_u8(0),
        }
    }
}

/// The condition ids the setup's wire EDM states, in the file's order -
/// what a page offers to choose from. Empty when no wire EDM is mounted
/// or its table is empty; whether a condition is complete is
/// [`condition_of`]'s question, asked when one is chosen.
#[must_use]
pub fn conditions_of(world: &World, setup: Option<&str>) -> Vec<String> {
    crate::machine::of_setup(world, setup)
        .1
        .filter(|m| crate::machine::kind(Some(m)) == Kind::WireEdm)
        .and_then(|m| m.edm)
        .map(|e| e.conditions.into_iter().map(|c| c.id).collect())
        .unwrap_or_default()
}

/// The generator's numbers from the op's, the condition's and the
/// world's. `stock` is the setup's stock outline in XY - the edge an
/// open run is entered from (#1109).
///
/// # Errors
/// A workpiece top or bottom that does not resolve, or a bottom that
/// does not stand below the top.
pub fn wire_params(
    op: &WireContourOp,
    cond: &WireCondition,
    cutoff: Option<&WireCondition>,
    hs: &ResolvedHeights,
    stock: Option<Region>,
) -> Result<WireParams, GenError> {
    let (Some(top), Some(bottom)) = (hs.top, hs.bottom) else {
        return Err(GenError::Invalid("wire contour: the workpiece's top and bottom must both resolve".into()));
    };
    if !(top.is_finite() && bottom.is_finite()) || bottom >= top {
        return Err(GenError::Invalid(format!(
            "wire contour: the workpiece's bottom ({bottom:.3}) does not stand below its top ({top:.3})"
        )));
    }
    Ok(WireParams {
        wire_diameter: cond.wire_diameter,
        gap: cond.gap,
        condition: cond.id.clone(),
        side: op.side,
        start: op.picks.first(crate::path_picks::PickRole::Start).map(|p| p.xy),
        plane_z: bottom,
        tabs: op.tabs.as_ref().map(|t| TabParams {
            count: t.count,
            width: t.width,
            at: t.at.iter().map(|[x, y]| Point2::new(*x, *y)).collect(),
            cutoff: cutoff.map(|c| (c.id.clone(), c.wire_diameter * 0.5 + c.gap)),
        }),
        stock,
        clearance: op.clearance,
        sharp_corners: op.sharp_corners,
    })
}

/// Tabs with a taper (#539 x #536), refused by name at the command
/// door and at generation: the tapered builder reads the top-plane path
/// as one closed ring, and a tabbed path is not one. `None` when the
/// op states no tabs or no taper.
#[must_use]
pub fn tabs_taper_refusal(op: &WireContourOp) -> Option<String> {
    (op.tabs.is_some() && op.taper_deg != 0.0).then(|| {
        format!(
            "wire contour: tabs can't be combined with a taper yet (a {:.3} degree taper is asked) - remove the tabs or set the taper to 0",
            op.taper_deg
        )
    })
}

/// Skims with what they are not cut with yet (#538): the skims walk
/// the vertical, untabbed contour of one profile, so a taper (#536), a
/// no-core burnout (#540), tabs (#539) or a 4-axis cut (#537) beside
/// them is refused by name, at the command door and at generation.
/// `None` when the op states no skims or none of those.
#[must_use]
pub fn skims_refusal(op: &WireContourOp) -> Option<String> {
    if op.skims.is_empty() {
        return None;
    }
    let with = if op.four_axis.is_some() {
        "a 4-axis cut".to_owned()
    } else if op.burnout.is_some() {
        "a no-core burnout".to_owned()
    } else if op.tabs.is_some() {
        "tabs".to_owned()
    } else if op.taper_deg != 0.0 {
        format!("a {:.3} degree taper", op.taper_deg)
    } else {
        return None;
    };
    Some(format!(
        "wire contour: {} skim pass(es) are stated with {with} - skims are cut on a vertical, untabbed one-profile contour only; remove the skims or that setting",
        op.skims.len()
    ))
}

/// Generate: `picks` is the picked profile (closed rings, open runs)
/// when the source is a selection; otherwise `base` - the model's
/// through-silhouette - is the profile. `stock` is read only for an
/// open run (#1109). `arcs`: a meshed model's chord, its arcs read back (#3215).
#[allow(clippy::too_many_arguments)]
pub(super) fn generate_wire(
    op: &WireContourOp,
    arcs: Option<f64>,
    cond: Result<(WireCondition, Option<WireCondition>), GenError>,
    hs: &ResolvedHeights,
    picks: Option<(Vec<Loop>, Vec<Vec<Point2>>)>,
    ruled: Option<Result<super::wire_ruled::RuledPicks, GenError>>,
    base: impl FnOnce() -> Result<Region, GenError>,
    stock: impl FnOnce() -> Option<Region>,
) -> Result<(Toolpath, Vec<Warning>), GenError> {
    let (cond, cutoff) = cond?;
    if let Some(why) = skims_refusal(op) {
        return Err(GenError::Invalid(why));
    }
    // #3332: the exit pick is read on a vertical, single-pass, untabbed
    // cut only; beside any other cut it is refused by name here too.
    if let Some(why) = super::pick_kinds::wire_exit_refusal(op) {
        return Err(GenError::Invalid(why));
    }
    // #3991: the tangent arc lead is cut on a selected DXF profile
    // (`wire_sketch_arcs`), which never reaches here; any other profile
    // is refused by name, never cut with the straight lead silently.
    if let Some(why) = super::wire_sketch_arcs::lead_arc_refusal(op) {
        return Err(GenError::Invalid(why));
    }
    if op.lead_arc_radius.is_some() {
        return Err(GenError::Invalid(
            "Wire Contour: a tangent arc lead is stated on a profile that is not a selected DXF chain - the arc lead is cut on a drawing's lines and arcs so far; set the straight lead or select the profile in a DXF drawing".into(),
        ));
    }
    let corner = corner_rule(op, &cond)?;
    // #537: two profiles paired into rulings - its own generator, read
    // from its own picks. A burnout (#540), tabs (#539) and an open run
    // (#1109) are not cut along rulings: each refused by name, never
    // cut as something else; on an angle-word machine `corner_rule`
    // has refused it already (#1164).
    if let Some(four) = &op.four_axis {
        if op.burnout.is_some() {
            return Err(GenError::Invalid(
                "wire contour: a 4-axis cut is asked with a no-core burnout - the burnout is cut vertical only; turn the burnout off or cut the two profiles without it".into(),
            ));
        }
        if op.tabs.is_some() {
            return Err(GenError::Invalid(
                "wire contour: a 4-axis cut is asked with tabs - tabs are cut on a one-profile contour only; remove the tabs or cut the two profiles without them".into(),
            ));
        }
        let p = wire_params(op, &cond, cutoff.as_ref(), hs, None)?;
        let picks = ruled.ok_or_else(|| super::job::unreached("wire profiles"))??;
        return super::wire_ruled::generate(op, four, &cond, &p, hs, &picks).map(|path| (path, Vec::new()));
    }

    let (loops, chains) = match picks {
        Some(geom) => geom,
        None => (base()?.loops().to_vec(), Vec::new()),
    };
    let stock = if chains.is_empty() { None } else { stock() };
    let p = wire_params(op, &cond, cutoff.as_ref(), hs, stock)?;
    // #538: skims walk the vertical, untabbed, one-profile contour
    // (`wire_passes`); every other cut is main's path exactly.
    if !cond.skims.is_empty() {
        let path = wire_passes(&loops, &chains, &p, &cond.skims)?;
        return Ok((path, Vec::new()));
    }
    let taper = WireTaper {
        angle_deg: op.taper_deg,
        top_z: hs.top.unwrap_or(p.plane_z),
        max_deg: cond.max_taper_deg,
        machine: cond.machine.clone(),
        corner,
    };
    let path = match op.burnout {
        // #536 x #540: the burnout's rings are cut vertical; a taper
        // with them is refused by name, never cut upright silently.
        Some(_) if op.taper_deg != 0.0 => {
            return Err(GenError::Invalid(format!(
                "wire contour: a {:.3} degree taper is asked with a no-core burnout - the burnout is cut vertical only; set the taper to 0 or turn the burnout off",
                op.taper_deg
            )))
        }
        Some(clearance) => wire_burnout(&loops, &chains, &p, clearance)?,
        // #539 x #536: the tapered builder reads one closed ring; a
        // tabbed path is not one, so the pair is refused by name.
        None => {
            if let Some(why) = tabs_taper_refusal(op) {
                return Err(GenError::Invalid(why));
            }
            // #3332: an exit pick ends an open run at its nearest end
            // and leaves a closed profile there; no pick is main's path.
            match op.picks.first(crate::path_picks::PickRole::Exit) {
                Some(exit) => crate::wire_edm::wire_contour_exit(&loops, &chains, &p, exit.xy)?,
                None => crate::wire_edm::wire_contour_facets(&loops, &chains, &p, &taper, arcs)?,
            }
        }
    };
    Ok((path, Vec::new()))
}

/// The corner rule a leaning cut is generated under (#1164): on a U/V
/// machine CamCraft's own mitre (#536); on an angle-word machine the
/// control's stated rule - unstated is refused by name, and so is a
/// 4-axis cut, which one angle cannot state.
///
/// # Errors
/// A 4-axis cut on an angle-word machine, or its rules incomplete.
pub(super) fn corner_rule(op: &WireContourOp, cond: &WireCondition) -> Result<WireCorner, GenError> {
    let Some(angle) = cond.taper_angle.filter(|_| op.taper_deg != 0.0 || op.four_axis.is_some()) else {
        return Ok(WireCorner::Mitre);
    };
    if op.four_axis.is_some() {
        return Err(GenError::Invalid(format!(
            "wire contour: a 4-axis cut, and machine {} tapers by angle word (edm.taper_angle) - one angle cannot state different upper and lower profiles; a 4-axis cut needs a U/V machine",
            cond.machine
        )));
    }
    let (_, corner) = angle.complete(&cond.machine).map_err(|e| GenError::Invalid(format!("wire contour: {e}")))?;
    Ok(match corner {
        crate::machine::TaperCorner::Mitre => WireCorner::Mitre,
        crate::machine::TaperCorner::Cone => WireCorner::Cone,
    })
}
