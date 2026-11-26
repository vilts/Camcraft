//! The post's live re-judge of the parts beside an operation's source
//! (#1628): what generation read through `keep_law::parts_in_stock` (a
//! stock source) and `keep_law::neighbour_parts` (a part source), read
//! again against the parts as they stand now.
//!
//! Provenance: our own design (`docs/decisions/1628-kept-parts-live-judge.md`)
//! over the solid and mark laws generation runs (`generate::contain_into`,
//! `mark_law::mark_into`). No vendor material consulted.
//!
//! ## The law
//!
//! Those parts are not in the op's stamp (the stamp contracts stand:
//! workholding never stales a path, a stock-based path never reads the
//! model, a fidelity refusal keeps its name). So a part moved into the
//! cut, added, un-tagged as a fixture, or un-marked as raw stock only
//! after a clean generation leaves the op `Ok` with a verdict nobody gave.
//! On an aligned stock the program sim judges every part live; where it
//! cannot judge (a turned stock, a posed path), the post gate
//! (`posting_sim`) calls [`moved_part_problem`] for each program op: the
//! cached path is judged again, with the very laws and numbers generation
//! used, against each kept part as it stands. A cut found is a refusal
//! naming the op and the part; nothing churns the op's status.

use flecs_ecs::prelude::World;

use crate::cl::{Move, Toolpath};
use crate::schema::Operation;
use crate::verify;

use super::generate::{contain_into, stock_to_leave_of, DepthLaw};
use super::{keep_law, mark_law};

/// Whether `op`'s kind can read parts beside its source at all - a cheap
/// door before a job is collected (most ops carry no such law).
fn may_keep(op: &Operation) -> bool {
    keep_law::is_stock_kept_kind(&op.params) || keep_law::is_part_kept_kind(&op.params, &op.src)
}

/// `None` when the cached `path` of program op `name` cuts none of the
/// parts its part laws keep beside its source, as they stand now; else
/// the refusal, naming the op and the first part it enters. An op whose
/// inputs can no longer be read is refused too: no verdict is no pass.
#[must_use]
pub fn moved_part_problem(world: &World, name: &str, op: &Operation, path: &Toolpath) -> Option<String> {
    part_problem(world, name, op, path, false)
}

/// #2313: the part a CONVICTED stock-source Chamfer or Deburr breaks
/// into (its cached findings carry the stock-edge gouge), named by the
/// same live judge - so the post's refusal names the op, the part and
/// the way out, also for a part that was there at generation.
#[must_use]
pub fn stock_edge_problem(world: &World, name: &str, op: &Operation, path: &Toolpath) -> Option<String> {
    part_problem(world, name, op, path, true)
}

fn part_problem(world: &World, name: &str, op: &Operation, path: &Toolpath, known: bool) -> Option<String> {
    if !may_keep(op) {
        return None;
    }
    let job = match super::collect_job(world, op, name) {
        Ok(job) => job,
        Err(e) => return Some(format!("{name}: the parts beside its source cannot be read again ({e}); regenerate it")),
    };
    if matches!(op.params, super::OpParams::MultiAxisDeburr { .. }) {
        // Posed moves have no column-stock verdict. Re-read the stock
        // before accepting a cache, including stock changed after generation.
        if let Err(e) = super::multiaxis_deburr_op::audit_stock(job.stock_facets.as_deref(), &job.material, job.frame.as_ref()) {
            return Some(format!("{name}: {e}"));
        }
    }
    if job.keep_parts.is_empty() && job.neighbour_parts.is_empty() {
        return None;
    }
    let parts = keep_law::kept_parts_named(world, op.setup.as_deref(), &op.params, &op.src, job.part_solid);
    // A cached posed deburr has no verdict for other parts imported or
    // moved after generation. The upright solid judge cannot supply it.
    if matches!(op.params, super::OpParams::MultiAxisDeburr { .. }) {
        let part = parts.first().map_or("other kept parts", |(name, _)| name.as_str());
        return Some(format!("{name}: Multi-Axis Deburr cannot judge the posed cutter against '{part}'; use a document with only the source part"));
    }
    let geom = job.tool.geom();
    let holder = crate::tool_body::stack(&geom, job.body.as_ref()).with_neck(job.neck.clone());
    let cutter = verify::Cutter::of(&job.tool).with_body(&geom, &holder);
    let cutter = super::thread_op::root_cutter(cutter, &job.tool, &job.params);
    let stl = stock_to_leave_of(&job.params);
    let excess = job.material.excess;
    // Generation's split (`generate::run_job_arms`): on a stock source an
    // Engrave or Trace marks the parts (budgeted), a Drill, Bore or Thread
    // takes the containment law with its declared holes' walls excused
    // (#1631), and a cross drill answers to its radial law (#1635); every
    // other kept part - and every neighbour of a part source - takes the
    // plain solid law. #1691: the post judges by the same split, so an
    // unchanged document gets the generation's verdict.
    let law = if job.part_solid {
        Law::Keep
    } else if mark_law::is_mark_kind(&job.params) {
        Law::Mark
    } else if keep_law::is_hole_kind(&job.params) {
        Law::Hole
    } else if keep_law::is_cross_drill(&job.params) {
        Law::Cross
    } else {
        Law::Keep
    };
    let budget = job.hs.top.zip(job.hs.bottom).map_or(0.0, |(t, b)| mark_law::budget(t, b, mark_law::cutter_depth_limit(&job.params, &job.tool)));
    // The thread grant generation gave (#1071 x #1205): only over round walls.
    let thread_bores = if law == Law::Hole { super::thread_op::bores(&job.tool, &job.params, thread_round(&job)) } else { Vec::new() };
    // The laws run in the frame generation judged in: a 3+2 op's local one.
    let local = job.frame.as_ref().map(|f| local_path(f, path));
    let judged = local.as_ref().unwrap_or(path);
    for (part, facets) in parts {
        let facets = job.frame.as_ref().map_or(facets.clone(), |f| f.local_facets(&facets));
        let found = match law {
            Law::Cross => cross_problem(judged, &facets, &job, &cutter).map(|w| if local.is_none() { w } else { String::new() }),
            _ => {
                let judge = |c: &verify::Cutter, bores: &[verify::ThreadBore]| {
                    let mut w = Vec::new();
                    match law {
                        Law::Mark => {
                            mark_law::mark_into(&mut w, judged, &facets, c, budget, excess, job.material.file_tolerance);
                        }
                        Law::Hole => contain_into(&mut w, judged, &facets, c, DepthLaw::Contain, stl, excess, job.material.file_tolerance, &job.params, bores),
                        _ => {
                            contain_into(&mut w, judged, &facets, c, DepthLaw::Keep, stl, excess, job.material.file_tolerance, &job.params, &[]);
                            super::keep_law::note_stock_chamfer_gouges(&job.params, job.part_solid, &mut w);
                        }
                    }
                    w
                };
                let mut warns = judge(&cutter, &thread_bores);
                // #266: a tap's fluted edge is read at its thread's core,
                // as at generation (the core takes no thread grant there).
                super::tap_core::acquit(&mut warns, &job.tool, &job.params, |core| {
                    let g = core.geom();
                    judge(&verify::Cutter::of(core).with_body(&g, &crate::tool_body::stack(&g, job.body.as_ref()).with_neck(holder.neck.clone())), &[])
                });
                warns.iter().find(|w| w.blocks_post()).map(|w| {
                    if local.is_none() {
                        w.to_string()
                    } else if matches!(w, verify::Warning::StockChamferGouge { .. }) {
                        "stock-source chamfer breaks into a part; chamfer with the part as source".to_owned()
                    } else if matches!(w, verify::Warning::StockDeburrGouge { .. }) {
                        "stock-source deburr breaks into a part; use the part as source".to_owned()
                    } else {
                        String::new()
                    }
                }).or_else(|| neighbour_cross_problem(judged, &facets, &job, &cutter).map(|w| if local.is_none() { w } else { String::new() }))
            }
        };
        if let Some(w) = found {
            // A local path's move numbers are not the posted path's: said only upright.
            let what = if w.is_empty() { String::new() } else { format!(" ({w})") };
            return Some(if known {
                format!("{name} breaks into part '{part}'{what}")
            } else {
                format!("{name} now cuts part '{part}'{what} - a part moved, added or no longer workholding since it was generated; regenerate it")
            });
        }
    }
    None
}

/// Which of generation's part laws a kept part answers to (#1691).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Law {
    /// The plain solid law, nothing excused.
    Keep,
    /// The mark law: the surface lowered by the budget and no more.
    Mark,
    /// The containment law: the declared holes' modelled walls excused.
    Hole,
    /// The cross drill's radial law (`cross_part`): the declared cross
    /// holes' walls excused about their own axes.
    Cross,
}

/// A stock-source cross drill's chain judged against one kept part by the
/// radial law generation ran (`cross_spindle::cross_drill` ->
/// `cross_part::part_clear`): the refusal's words, or `None` when clear.
fn cross_problem(path: &Toolpath, facets: &[crate::geometry::Triangle], job: &super::GenJob, cutter: &verify::Cutter) -> Option<String> {
    let crate::operation::OpParams::Drill { cross, .. } = &job.params else {
        return None;
    };
    super::cross_part::part_clear(path, facets, &job.tool, cutter, cross, job.material.excess).err().map(|e| e.to_string())
}

/// A part-source cross drill's chain against one of the setup's other
/// parts: generation ran `cross_part::neighbours_clear` beside the plain
/// solid law (#1641), so the post runs both too. `None` for any other op.
fn neighbour_cross_problem(path: &Toolpath, facets: &[crate::geometry::Triangle], job: &super::GenJob, cutter: &verify::Cutter) -> Option<String> {
    let crate::operation::OpParams::Drill { cross, .. } = &job.params else {
        return None;
    };
    if !job.part_solid || cross.is_empty() {
        return None;
    }
    super::cross_part::neighbours_clear(path, facets, &job.tool, cutter, cross, job.material.excess).err().map(|e| e.to_string())
}

/// Whether a Thread's holes stand in round walls, read as generation read
/// them (`thread_op::walls_round` on its source material, in the op's
/// frame). Any failure to read it is `false`: no thread grant, the teeth
/// judged at full radius - the stricter law.
fn thread_round(job: &super::GenJob) -> bool {
    let crate::operation::OpParams::Thread { external: false, .. } = &job.params else {
        return false;
    };
    let oz = job.frame.as_ref().map_or(0.0, |f| f.origin.z);
    let hl = super::heights::ResolvedHeights {
        clearance: job.hs.clearance - oz,
        top: job.hs.top.map(|v| v - oz),
        bottom: job.hs.bottom.map(|v| v - oz),
        face_top: job.hs.face_top.map(|v| v - oz),
        retract: job.hs.retract.map(|v| v - oz),
    };
    let Ok(plan) = super::thread_op::kernel_params(&job.tool, &job.params, &hl) else {
        return false;
    };
    let facets = job.frame.as_ref().map_or_else(|| job.material.facets.clone(), |f| f.local_facets(&job.material.facets));
    super::thread_op::walls_round(&facets, &plan)
}

/// A posted (world) path back in a 3+2 frame's local coordinates: every
/// move as its 3-D chords at the arc-fit tolerance (a tilted path is
/// already chords; an upright one's arcs give way to chords within the
/// same tolerance the verifier measures arcs by).
fn local_path(f: &crate::cl::TiltedFrame, path: &Toolpath) -> Toolpath {
    let moves = path
        .moves
        .iter()
        .flat_map(|m| {
            m.segments3(crate::machining2d::ARC_FIT_TOL)
                .into_iter()
                .map(|(a, b)| Move { from: f.to_local(a), to: f.to_local(b), arc: None, axis: None, ..*m })
        })
        .collect();
    Toolpath { moves, ..path.clone() }
}
