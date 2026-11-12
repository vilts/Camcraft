//! Which fixture a warned move strikes (#339 slice 4): a `FixtureHit`
//! names the op, the move and the band, never the steel - the laws judge
//! every box at once. To tint the struck fixture, each fixture is judged
//! ALONE with the very laws the post gate runs ([`op_hits`]), and the
//! warned moves each one reproduces are its own. A warned move no single
//! fixture reproduces (the steel moved since generation, or two boxes
//! only convict together) is reported unresolved - never guessed.
//!
//! Display only: nothing here writes the world or feeds the post.
//!
//! Provenance: our own composition of the existing workholding laws
//! (`camcraft_sim::verify`, the post gate's own call shape); no vendor
//! material consulted.

use flecs_ecs::prelude::World;

use crate::cl::Toolpath as ClPath;
use crate::fixture::{self, FixtureBox, MeshedFixture, MACHINE_TABLE};
use crate::model;
use crate::operation;
use crate::schema::Operation;
use crate::verify::Warning;

/// The workholding laws (c2)/(d2), mesh-refined (#339), for one op's
/// path against the given steel: exactly the post gate's per-op check.
/// `Err` names a tool body that no longer fits its tool; a missing tool
/// or no steel at all is no hit.
pub fn op_hits(
    world: &World,
    op: &Operation,
    path: &ClPath,
    turning: &[FixtureBox],
    fixed: &[FixtureBox],
    meshes: &[MeshedFixture],
) -> Result<Vec<Warning>, String> {
    let Some((tool_entity, tool)) = model::find_by_name(world, &op.tool)
        .and_then(|t| model::tool_of(world, &t).map(|d| (t, d)))
    else {
        return Ok(Vec::new());
    };
    // #368: the body's enclosing cylinder and band stack; a body that no
    // longer fits its tool is itself the problem, by name.
    // #852: the drawn neck, judged against the stored record.
    let neck = crate::tool_cutter::neck_of(world, &tool_entity, &tool);
    let (tool, body) =
        crate::tool_body::judged(world, &tool_entity, tool).map_err(|e| format!("tool {}: {e}", op.tool))?;
    // `boxes_for` already folds in the machine-derived table, so empty
    // lists mean no steel this op could strike at all.
    if turning.is_empty() && fixed.is_empty() {
        return Ok(Vec::new());
    }
    // #388: the op's own setup's machine, never the active one.
    let spindle = crate::machine::of_setup(world, op.setup.as_deref())
        .1
        .and_then(|m| m.spindle_d.zip(m.spindle_len))
        .map(|(d, l)| (d * 0.5, l));
    let wh = crate::verify::Workholding { turning, fixed, axis: fixture::rotary_axis_of(world, op) };
    let geom = tool.geom();
    let holder = crate::tool_body::stack(&geom, body.as_ref()).with_neck(neck);
    Ok(path_hits(path, (&geom, &holder), &wh, spindle, meshes))
}

/// The per-op workholding laws themselves, world-free: the post gate's
/// verdict ([`op_hits`]) and the generator's - its warnings and the
/// link planner's re-judge (#339 slice 5) - all call this one law over
/// the same inputs, so a routed link is judged exactly as it posts.
#[must_use]
pub fn path_hits(
    path: &ClPath,
    (geom, holder): (&crate::tools::ToolGeom, &camcraft_tools::holder_profile::HolderStack),
    wh: &crate::verify::Workholding<'_>,
    spindle: Option<(f64, f64)>,
    meshes: &[MeshedFixture],
) -> Vec<Warning> {
    // #339: each law's hits on a meshed fixture re-read against its
    // closed mesh - the mesh only ever acquits.
    let stack = crate::verify::steel_stack(geom, holder, spindle);
    let steel = crate::verify::MeshSteel { fixtures: meshes, stack: &stack, chord_tol: operation::GOUGE_TOL };
    let mut hits = crate::verify::mesh_refined(path, wh, &steel, false, |w| {
        crate::verify::bands_vs_workholding_held(path, geom, holder, w, spindle, operation::GOUGE_TOL)
    });
    // The spindle housing rides posed rapids too (M13.8 (1)).
    hits.extend(crate::verify::mesh_refined(path, wh, &steel, true, |w| {
        crate::verify::rapids_vs_workholding_held(path, geom, holder, w, spindle)
    }));
    hits
}

/// The moves an op's cached warnings say strike a fixture, ascending.
#[must_use]
pub fn warned_moves(warnings: &[Warning]) -> Vec<usize> {
    let mut at: Vec<usize> = warnings
        .iter()
        .filter_map(|w| match w {
            Warning::FixtureHit { at, .. } => Some(*at),
            _ => None,
        })
        .collect();
    at.sort_unstable();
    at.dedup();
    at
}

/// Who an op's fixture warnings point at.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Struck {
    /// The fixtures (tagged model names, and [`MACHINE_TABLE`] for the
    /// derived table) that reproduce at least one warned move alone,
    /// name order, table last.
    pub fixtures: Vec<String>,
    /// Warned moves no single fixture reproduces today.
    pub unresolved: Vec<usize>,
    /// Every warned move, ascending.
    pub warned: Vec<usize>,
}

impl Struck {
    /// Whether `name` is one of the struck fixtures.
    #[must_use]
    pub fn names(&self, name: &str) -> bool {
        self.fixtures.iter().any(|f| f == name)
    }
}

/// Resolve the op `op_name`'s cached `FixtureHit` warnings to the
/// fixtures that cause them. `None`: no such op, no cached path, or no
/// fixture warning to resolve.
#[must_use]
pub fn struck_by(world: &World, op_name: &str) -> Option<Struck> {
    let e = model::find_by_name(world, op_name)?;
    let op = model::operation_of(world, &e)?;
    let cache = model::toolpath_of(world, &e)?;
    let warned = warned_moves(&cache.warnings);
    if warned.is_empty() {
        return None;
    }
    let ctx = op.setup.as_deref();
    let mut fixtures = Vec::new();
    let mut covered: Vec<usize> = Vec::new();
    let mut judge = |name: String, turning: Vec<FixtureBox>, fixed: Vec<FixtureBox>, meshes: Vec<MeshedFixture>| {
        // A tool body that no longer fits is the post gate's refusal, not
        // a fixture's: nothing is resolved through it.
        let Ok(hits) = op_hits(world, &op, &cache.path, &turning, &fixed, &meshes) else {
            return;
        };
        let mine: Vec<usize> = warned_moves(&hits).into_iter().filter(|a| warned.binary_search(a).is_ok()).collect();
        if !mine.is_empty() {
            fixtures.push(name);
            covered.extend(mine);
        }
    };
    for (name, entity) in fixture::tagged(world, ctx) {
        let Some(b) = model::entity_world_aabb(world, &entity) else { continue };
        let b = FixtureBox { min: b.min, max: b.max };
        let meshes = model::world_facets(world, &entity)
            .map(|tris| vec![MeshedFixture::new(name.clone(), b, tris.into())])
            .unwrap_or_default();
        if fixture::def_of(world, &entity).is_some_and(|d| d.fixed) {
            judge(name, Vec::new(), vec![b], meshes);
        } else {
            judge(name, vec![b], Vec::new(), meshes);
        }
    }
    if let Some(t) = fixture::table_box(world, ctx) {
        judge(MACHINE_TABLE.to_string(), Vec::new(), vec![t], Vec::new());
    }
    let unresolved = warned.iter().copied().filter(|a| !covered.contains(a)).collect();
    Some(Struck { fixtures, unresolved, warned })
}
