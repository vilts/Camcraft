//! The world-free half of a generation (M5.3 B): [`collect_job`] reads
//! everything [`super::generate`] ever reads from the world and packs it
//! into a [`GenJob`] of plain owned data; [`super::run_job`] turns the
//! job into the path without ever naming a `World`. The type system is
//! the law here: `&World` is not `Send`, so a job that compiles into a
//! `Send` struct cannot smuggle a live read into a worker thread, and
//! the collect phase is where every read provably happened.
//!
//! Determinism, the whole point of the seam: a constructor's world is
//! frozen (constructors only read - the command writes nothing until
//! `apply`), so running the jobs of a batch in any order - or in
//! parallel - produces exactly the parts the serial loop produced.
//! `rayon`'s `into_par_iter().collect()` preserves input order, so even
//! the `parts`/`skipped` lists come out in tree order, byte-for-byte
//! the old sequential result.

use std::sync::Arc;

use super::*;

/// Everything one generation reads from the world, and nothing else.
/// The world-touching precomputes ride as stored `Result`s, taken at
/// their generation-time call sites, so a batch surfaces exactly the
/// error the serial path surfaces, in the same precedence.
pub struct GenJob {
    /// The operation's own name, for the generation log (#163) - "" for
    /// a preview with no entity.
    pub(super) op_name: String,
    pub(super) tool: crate::tools::Tool,
    /// The tool's modelled body (#368): its envelope, for the fixture
    /// laws' band stack. `tool` already carries the cylinder enclosing
    /// it, for every law that reads one; None is no body.
    pub(super) body: Option<camcraft_tools::holder_profile::HolderProfile>,
    /// #852: the tool's drawn neck (`tool_cutter::neck_of`); empty = none.
    pub(super) neck: Vec<camcraft_tools::holder_profile::HolderBand>,
    /// The tool's lathe body (#369): its clearance bands, judged against
    /// the revolved stock on every move of a turning op; None is none.
    pub(super) lathe_body: Option<std::sync::Arc<camcraft_tools::lathe_holder::LatheClearance>>,
    /// The op's tool NAME as the component stored it - the error
    /// strings of the tilted kinds quote it verbatim.
    pub(super) tool_name: String,
    pub(super) params: OpParams,
    pub(super) src: SrcRef,
    pub(super) material: Material,
    pub(super) frame: Option<crate::cl::TiltedFrame>,
    /// Resolved heights, UNshifted - the frame's `oz` fold is pure
    /// arithmetic and belongs to `run_job`.
    pub(super) hs: ResolvedHeights,
    pub(super) stock_facets: Option<Vec<Triangle>>,
    pub(super) stock_aabb: Option<(Point3, Point3)>,
    // Sampled mesh stock can miss narrow features; only analytic stock credits this advisory.
    pub(super) analytic_stock: bool,
    /// #1091 review: the stock's mesh fidelity is known (`model_fidelity::stock_fidelity_known`).
    pub(super) stock_fidelity_known: bool,
    pub(super) section_z: Option<Result<f64, GenError>>,
    pub(super) picked_region: Option<Result<Region, GenError>>,
    pub(super) contour_sel:
        Option<Result<super::selection::SelectionGeometry, GenError>>,
    pub(super) drive_chains: Option<Result<Vec<Vec<Point3>>, GenError>>,
    pub(super) prior: Option<Result<Arc<crate::rest::PriorState>, GenError>>,
    /// The program ahead of this op in its setup, uncut (issue #96
    /// review): verify's vertical-dive convictions are re-read against
    /// the stock it leaves standing. `None` is nothing ahead cut, a
    /// tilted or turned op (whose verify runs in another frame or the
    /// meridian), or an unreadable stock - all the raw-stock reading.
    pub(super) verify_prior: Option<crate::rest::PriorPlan>,
    /// The Drill ops ahead, for the turned family's rapid law (#103).
    pub(super) drills_ahead: Vec<crate::rest::DrillAhead>,
    /// The turning ops ahead, for the same law (#159).
    pub(super) turned_ahead: Vec<crate::rest::TurnedAhead>,
    pub(super) is_mill: bool,
    /// The document's machine kind (M13.5): the lathe routes the Drill
    /// kind down the spindle axis, and the rotary kinds read their
    /// letter off the machine's rotary inventory.
    pub(super) machine_kind: crate::machine::Kind,
    /// The mounted machine's rotary inventory, `(model, axes)` - None
    /// when no machine is mounted, which checks nothing (the
    /// unstated-limit law).
    pub(super) machine_rotaries: Option<(String, Vec<crate::machine::AxisLimits>)>,
    pub(super) spindle: Option<(f64, f64)>,
    pub(super) fixtures: Vec<crate::fixture::FixtureBox>,
    /// `fixtures`' names, index for index (#339 slice 5 refusals).
    pub(super) fixture_names: Vec<String>,
    /// The highest world Z the tip reaches (WCS origin + stated Z top).
    pub(super) z_ceiling: Option<f64>,
    /// `fixtures` split by whether the steel turns with the part
    /// (M13.8 (2)): `.0` rides the rotary, `.1` stands still (the
    /// declared-fixed models, then the machine table) - the post gate's
    /// own split (`fixture::workholding_for`), read here so generation
    /// and the gate speak one verdict.
    pub(super) workholding: (Vec<crate::fixture::FixtureBox>, Vec<crate::fixture::FixtureBox>),
    /// The tagged models' meshes beside their boxes (#339): the mesh
    /// law acquits what a box flagged and the real steel does not meet.
    pub(super) fixture_meshes: Vec<crate::fixture::MeshedFixture>,
    /// The rotary axis line a rotary kind turns its part about, in the
    /// world frame; None stands the machine-frame judgement down and the
    /// fixed steel keeps the path-frame verdict (the conservative one).
    pub(super) rotary_axis: Option<crate::verify::RotaryAxis>,
    /// Whether `material` is a finished PART (a named model, or picks
    /// made only on models) rather than the setup's stock: the depth-
    /// aware verify judges the 2D kinds against its solid only then -
    /// cutting into stock is the job, cutting into the part never is.
    pub(super) part_solid: bool,
    /// The area controls resolved against the world (M11.5); `None` for
    /// the identity - an untouched operation carries nothing new.
    pub(super) area: Option<Result<AreaCtx, GenError>>,
    /// The work offset's world origin (`posting::wcs_context`), the
    /// spindle a lathe or mill-turn post programs about: world Z
    /// through it. The cross drill measures its standoffs from this
    /// line (issue #67), not from the world origin.
    pub(super) wcs_origin: [f64; 3],
    /// The spindle law's non-blocking note (issue #71 review): the
    /// share of the turned surface the axis fit set aside, which the
    /// op does not turn. Carried onto the path's notes by `run_job`.
    pub(super) spindle_note: Option<String>,
    /// #319: carried material outside the declared stock, said by name on
    /// the path once it exists (`carried_stock::Overhang::note`) - the post
    /// gate refuses when a feed move reaches it.
    pub(super) carried_overhang: Option<super::carried_stock::Overhang>,
}

/// The tripwire phrase for a stored precompute whose arm never collected
/// it - a call-structure drift, which is a bug, and it names itself
/// rather than cutting a silent shadow (Prime Directive).
pub(super) fn unreached(what: &str) -> GenError {
    GenError::Invalid(format!("internal: the job's {what} was never collected"))
}

/// Read the world once, as `generate` would, and pack the answers.
/// Errors keep their generation-time precedence: everything that can
/// refuse before a single region is built (tool, material, heights,
/// the tilt refusals) refuses HERE, exactly as the old head refused;
/// everything an arm used to refuse at lives on as a stored `Result`
/// for `run_job` to take at that exact statement.
pub fn collect_job(world: &World, op: &Operation, op_name: &str) -> Result<GenJob, GenError> {
    // Tool: the diameter drives the path, the body drives the reach law.
    let Some(tool_entity) = model::find_by_name(world, &op.tool) else {
        return Err(GenError::Invalid(format!("no tool named '{}'", op.tool)));
    };
    let Some(tool) = model::tool_of(world, &tool_entity) else {
        return Err(GenError::Invalid(format!("'{}' is not a tool", op.tool)));
    };
    let neck = crate::tool_cutter::neck_of(world, &tool_entity, &tool);
    let (tool, body) = crate::tool_body::judged(world, &tool_entity, tool)
        .map_err(|e| GenError::Invalid(format!("tool '{}': {e}", op.tool)))?;
    // #369: a lathe body is judged only by the turning family's
    // revolved-stock law; on any other op no law could read it, so the
    // op is refused by name rather than cut with it unjudged.
    let lathe_body = crate::tool_body_lathe::clearance_of(world, &tool_entity);
    if lathe_body.is_some() && op.params.kind().info().family() != super::Family::Turn {
        return Err(GenError::Invalid(format!(
            "tool '{}' carries a lathe-frame body, which only a turning operation judges - clear it or place a spindle body for this op",
            op.tool
        )));
    }
    let setup = op.setup.as_deref();
    // #326: an op this slice has not verified in an aligned setup is
    // refused by name here, at the one door every generation passes -
    // never cut in the unaligned frame.
    if let Some(why) = crate::setup_align::generation_problem(world, op) {
        return Err(GenError::Invalid(why));
    }
    let material = super::material::judged_material(world, setup, &op.src)?;
    let part_solid = part_solid_of(world, setup, &op.src, &material);
    // Setup stock, when it is not the source itself: the swarf column
    // around a smaller model (the head read at `generate` - same call).
    let mut stock_facets = match &op.src {
        SrcRef::Stock => None,
        SrcRef::Entity(_) | SrcRef::Selection(_) => {
            model::find_by_name(world, &crate::setup::stock_entity_name(world, setup))
                .and_then(|e| model::world_facets(world, &e))
        }
    };
    let mut stock_aabb = model::stock_world_aabb_for(world, setup).map(|b| (b.min, b.max));
    let mut analytic_stock = model::find_by_name(world, &crate::setup::stock_entity_name(world, setup))
        .and_then(|e| model::stock_def_of(world, &e))
        .is_some_and(|s| matches!(s, crate::stock::StockShape::Box { .. } | crate::stock::StockShape::Cylinder { .. }));

    // The height seam, refused by name before anything else (as the old
    // head refused it, `resolve_params` being the first fallible read
    // after the material).
    let hs = resolve_params(world, setup, Some(&op.src), &op.params).map_err(GenError::Invalid)?;

    // The 3+2 wrap (M5.1 B), same refusals, same order: non-finite
    // letters first, then the tilted-selection lock.
    let frame = match op.params.tilt() {
        None => None,
        Some(t) => match t.frame() {
            Some(f) => Some(f),
            // Only a non-finite letter could get here (the parameter
            // door refuses it); naming it beats inventing a plane.
            None => {
                return Err(GenError::Invalid(
                    "the 3+2 index commands no direction - its angles are not finite".into(),
                ));
            }
        },
    };
    // The one refusal the wrap owes (M5.1 B): a picked selection is
    // read as world geometry by the selection walk - projecting a pick
    // onto a tilted plane is not implemented, and a shadow of the pick
    // is not what was picked. Refused by name, never silently cut as
    // a projection.
    if frame.is_some() && matches!(op.src, SrcRef::Selection(_)) {
        return Err(GenError::Invalid(
            "a tilted plane cannot yet index a picked selection - pick a whole model instead"
                .into(),
        ));
    }

    // The arm-scoped world reads, precomputed under the same guards
    // that decide which arm runs them; each result surfaces at its
    // arm's own statement in `run_job`.
    let section_z = section_z_of(world, setup, Some(&op.src), &op.params);
    let picked_region = match (&op.params, &op.src) {
        // Contour's Selection pair is resolved as raw geometry below
        // (its arm never asks for a region first); every other kind
        // whose picks author the region takes selection_region's
        // answer.
        (params, SrcRef::Selection(items))
            if selection_drives_region(params)
                && !matches!(params, OpParams::Contour { .. } | OpParams::PlasmaProfile(_)) =>
        {
            Some(selection_region(world, setup, items, params, &op.area))
        }
        // A whole PART under the pocket family clears the stock around
        // it, never the part's own section (issue #12).
        (params, src) => whole_model_pocket(world, setup, src, params, frame.as_ref()),
    };
    let contour_sel = match (&op.params, &op.src) {
        // The plasma cut reads its picks the contour's way (#670):
        // closed rings AND open runs, each cut as picked.
        (OpParams::Contour { .. } | OpParams::PlasmaProfile(_), SrcRef::Selection(items)) => {
            Some(selection_geometry_impl(world, setup, items, &op.params, None))
        }
        _ => None,
    };
    let drive_chains = match &op.params {
        OpParams::MultiAxisContour { .. } => Some(drive_chains_of(world, op)),
        _ => None,
    };
    // #318: an op in a setup whose incoming stock must be carried across
    // a boundary the carry refuses has no known stock to judge against -
    // refused by name, never read as the setup's own fresh block (#319:
    // every op, the posed ones included - their verdicts read it too).
    if let Some(why) = crate::rest::carry_refusal(world, op_name, setup) {
        return Err(GenError::Invalid(format!("{op_name}: {why}")));
    }
    // #319: where the carried body stands outside the declared stock,
    // every stock-reading law judges the box enclosing both.
    let carried = super::carried_stock::judged(world, op_name, setup, tool.diameter * 0.5)?;
    if let Some((facets, aabb)) = carried.stock {
        stock_facets = Some(facets);
        stock_aabb = Some(aabb);
        analytic_stock = true;
    }
    let carried_overhang = carried.overhang;
    let prior = match &op.params {
        OpParams::Pocket3D { .. } | OpParams::MultiAxisRough { .. } => {
            // The mask the rest laws stand on, resolved NOW on the
            // caller's thread; the Arc is so a run never clones a
            // height field (Dexel is big, and borrowed anyway).
            Some(
                crate::rest::prior_state(world, op_name, setup, tool.diameter * 0.5)
                    // #319: declared material past the carried grid is
                    // present - no mask, the declared stock is cut.
                    .map(|p| if carried.distrust_prior { crate::rest::PriorState { grid: None, ..p } } else { p })
                    .map(Arc::new),
            )
        }
        _ => None,
    };
    // Verify's prior reading (#96 review): collected here, cut on the
    // job thread only when a dive was convicted. An unreadable stock
    // grid is logged and read as the raw stock - convictions stand.
    // The turned family reads it too (#103): its axial rapids are judged
    // against the stock the program ahead faced.
    let verify_prior = if frame.is_none() && !carried.distrust_prior {
        match crate::rest::prior_plan(world, op_name, setup, tool.diameter * 0.5) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(op = %op_name, "the program ahead is unreadable for the rapid law, raw stock stands: {e}");
                None
            }
        }
    } else {
        None
    };
    // The axial pilots ahead (#103): a bore pass's approach runs down
    // the hole the drill made, which the facing-only prior cannot read.
    let turned = op.params.kind().info().section == super::kinds::Section::Turning;
    let drills_ahead = if turned { crate::rest::drills_ahead(world, op_name, setup) } else { Vec::new() };
    let turned_ahead = if turned { crate::rest::turned_ahead(world, op_name, setup) } else { Vec::new() };
    let area = area_ctx(world, op, tool.diameter * 0.5, &material, part_solid);
    // #388: the op's own setup's machine, active or not.
    let machine_kind = crate::machine::kind_of_setup(world, setup);
    // The lathe spindle law (issue #62): every kind that turns about the
    // fitted axis - the turning family, and the lathe's axial Drill -
    // must have that axis ON the spindle the post programs (world Z
    // through the work offset's origin, the very origin `wcs_context`
    // hands the post), or it is refused here, before a move exists.
    // (The post reads it again, against the origin it is handed then.)
    // The mill-turn's cross drill joins it (issue #67): its standoffs
    // are measured about that same line.
    let (slot, wcs_origin) = crate::posting::wcs_context(world, setup);
    let spindle_note = if super::spindle_law::turns(world, setup, &op.params) {
        super::spindle_law::check(&material.facets, wcs_origin, slot)?
    } else {
        None
    };
    let is_mill = machine_kind == crate::machine::Kind::Mill;
    let (fixture_names, fixtures) = crate::fixture::named_boxes_for(world, setup).into_iter().unzip();
    let workholding = crate::fixture::workholding_for(world, setup);
    let fixture_meshes = crate::fixture::meshes_for(world, setup);
    let rotary_axis = crate::fixture::rotary_axis_of(world, op);
    let mounted = crate::machine::of_setup(world, setup).1;
    let spindle = mounted
        .as_ref()
        .and_then(|m| m.spindle_d.zip(m.spindle_len))
        .map(|(d, l)| (d * 0.5, l));
    let z_ceiling = mounted
        .as_ref()
        .and_then(|m| m.axis(2))
        .and_then(crate::machine::AxisLimits::program_travel)
        .map(|(lo, hi)| wcs_origin[2] + lo.max(hi));
    let machine_rotaries = mounted.map(|m| {
        let rot: Vec<_> = m.axes.iter().filter(|a| a.rotary).cloned().collect();
        (m.model, rot)
    });

    Ok(GenJob {
        op_name: op_name.to_owned(),
        tool,
        body,
        neck,
        lathe_body,
        tool_name: op.tool.clone(),
        params: op.params.clone(),
        src: op.src.clone(),
        material,
        frame,
        hs,
        stock_facets,
        stock_aabb,
        analytic_stock,
        stock_fidelity_known: crate::model_fidelity::stock_fidelity_known(world, setup),
        section_z,
        picked_region,
        contour_sel,
        drive_chains,
        prior,
        verify_prior,
        drills_ahead,
        turned_ahead,
        is_mill,
        machine_kind,
        machine_rotaries,
        spindle,
        fixtures,
        fixture_names,
        z_ceiling,
        workholding,
        fixture_meshes,
        rotary_axis,
        part_solid,
        area,
        wcs_origin,
        spindle_note,
        carried_overhang,
    })
}

/// The old `region_for_src`'s law told without a world: the picks' answer when
/// the picks drive, otherwise the pure region build with the section
/// height `section_z_of` already resolved. The `take`s mean a stored
/// error surfaces exactly where the live call used to make it.
pub(super) fn region_for_job(
    picked_region: &mut Option<Result<Region, GenError>>,
    section_z: &mut Option<Result<f64, GenError>>,
    src: &SrcRef,
    params: &OpParams,
    material: &Material,
    frame: Option<&crate::cl::TiltedFrame>,
) -> Result<Region, GenError> {
    // A stored answer is THE answer: the picks' region, or the area
    // controls' (M11.5) - `run_job` precomputes the controlled region
    // into this same slot, so every arm takes it unchanged.
    if let Some(r) = picked_region.take() {
        return r;
    }
    if matches!(src, SrcRef::Selection(_)) && selection_drives_region(params) {
        // The Contour arm never comes through here (its picks resolve
        // as geometry, not as a region) - so the guard that decided
        // collection decides the take.
        debug_assert!(!matches!(params, OpParams::Contour { .. }));
        return Err(unreached("picked selection region"));
    }
    region_core(
        material,
        params,
        frame,
        section_z.take().unwrap_or(Ok(0.0)),
    )
}
