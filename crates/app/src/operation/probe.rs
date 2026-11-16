//! The in-process measurement verdict (M5.3 D): what the program's
//! blades cut TOGETHER, judged against the budget a Part op's author
//! asked the probe to hold - spoken at the post, never at the op.
//!
//! A PROBE word on the stream states the ask (camcraft-post voices
//! it, and the shop machine takes the real measurement); this is the
//! CamCraft-side answer made before any tape runs, the law that
//! decides whether the part gets a re-run. The measurement is the
//! PROGRAM's, not any single operation's, and that is the whole
//! point: a roughing blade that holds steel back for a finish op
//! leaves nothing on the part once the finish answers for it, and
//! leaves exactly what stands when it never does. So the blade legs
//! compose into one cut - the boxcar-min union the M5.2 C scan
//! already makes of one op's passes, now of the program's - and one
//! meridian measurement at the strictest ask says what the probe
//! would find. This is M2.3's rest intent read at the drawing's
//! bar: a program that asked no probe posts on its promise of a
//! later finish op; an authored ask turns that promise into a fact.
//!
//! Jurisdiction: the boxcar-blade family - Part, GrooveRough,
//! GrooveFinish - the kinds whose cut is a full-width band in the
//! meridian. The threading V and the forming crown compose floors
//! the same scan could read (their floors exist beside the blade's),
//! but no probe ask rides them yet: the ask is authored on the
//! Part, so the other lathe laws stay honestly silent rather than
//! half-measured. No machine-file clause: a `.mch` states nothing
//! about probes, and an unstated limit checks nothing - what is
//! judged here is deviation, not hardware.
//!
//! Provenance: in-process gauging as machine-tool handbook practice;
//! the composition law is our own, on our own revolved-blade verdict
//! (M5.2 C). No vendor strategy consulted.

use flecs_ecs::prelude::World;

use crate::cl::Toolpath as ClPath;
use crate::model;
use crate::turning::{self, BladeLeg};
use crate::verify::Warning;

use super::{material_of, OpParams, SrcRef};

/// The blade family the composed scan judges (module header): one
/// full-width band each, the boxcar's own vocabulary.
fn is_blade(params: &OpParams) -> bool {
    matches!(
        params,
        OpParams::Part { .. } | OpParams::GrooveRough { .. } | OpParams::GrooveFinish { .. }
    )
}

/// One probe ask, read off a Part op: the budget, the op that
/// stated it, and the part it measures (a stock-sourced ask also
/// cares WHICH setup's stock - the same word names a different
/// block in another setup).
struct Ask {
    budget: f64,
    name: String,
    src: SrcRef,
    setup: Option<String>,
}

/// Does a cut belong to this part? The model names it; a stock
/// block is pinned through its setup too.
fn shares(src: &SrcRef, setup: Option<&str>, ask: &Ask) -> bool {
    *src == ask.src
        && (!matches!(src, SrcRef::Stock)
            || ask.setup.is_none()
            || setup == ask.setup.as_deref())
}

/// One gate line per ask the program cannot meet - the contract
/// `verification_gate` aggregates (the shared-gate law: exactly the
/// program `program_ops` just blessed is measured here). No ask, no
/// scan, no cost: the number is the switch, and silence is free.
/// The PROBE word itself voices untouched either way - the stream
/// tells the shop what to measure; this tells the author whether
/// the program deserves a re-run before posting it.
#[must_use]
pub(crate) fn probe_problems(world: &World) -> Vec<String> {
    let Ok(ops) = crate::posting::program_ops(world) else {
        return Vec::new(); // the post refuses this program by better names already
    };
    let mut asks: Vec<Ask> = Vec::new();
    for (name, e) in &ops {
        let Some(op) = model::operation_of(world, e) else {
            continue; // in the validated list; lost a beat later
        };
        if let OpParams::Part { inspect_tol, .. } = &op.params
            && *inspect_tol > 0.0
        {
            asks.push(Ask {
                budget: *inspect_tol,
                name: name.clone(),
                src: op.src.clone(),
                setup: op.setup.clone(),
            });
        }
    }
    if asks.is_empty() {
        return Vec::new();
    }
    // One part per program, said once: two asks measuring different
    // solids is not one number, and composing across two frames
    // would make it a wrong one.
    for a in asks.iter().skip(1) {
        if !shares(&a.src, a.setup.as_deref(), &asks[0]) {
            return vec![format!(
                "probe asks on {} and {} measure different parts - one final, one verdict: post the programs apart (M5.3 D)",
                asks[0].name, a.name
            )];
        }
    }
    let head = &asks[0];
    let budget = asks.iter().fold(f64::INFINITY, |b, a| b.min(a.budget));
    let asked: Vec<&str> = asks.iter().map(|a| a.name.as_str()).collect();
    let material = match material_of(world, head.setup.as_deref(), &head.src) {
        Ok(m) => m,
        // The part the probe touches is unresolvable, and it
        // blocks: a judge that cannot measure never reports
        // silence (law (b), everywhere).
        Err(err) => {
            return vec![format!("probe ask on {}: the part is unresolvable: {err}", head.name)];
        }
    };
    // The legs, in program order: every blade cut of this part. The
    // composed `at` index is the program's own move space - blame
    // reads the guilty leg off it, so the line names who left the
    // steel, not merely that steel stands.
    let mut legs: Vec<(String, usize, ClPath, f64)> = Vec::new();
    for (name, e) in &ops {
        let Some(op) = model::operation_of(world, e) else {
            continue;
        };
        if !is_blade(&op.params) || !shares(&op.src, op.setup.as_deref(), head) {
            continue;
        }
        let Some(cache) = model::toolpath_of(world, e) else {
            return vec![format!(
                "{name}: a blade of this part lost its toolpath - the composed part is unmeasurable"
            )];
        };
        let Some(tool) = model::find_by_name(world, &op.tool)
            .and_then(|t| model::tool_of(world, &t))
        else {
            return vec![format!(
                "{name}: the blade's width is the tool's - '{}' is not in the library, and the composed part is unmeasurable",
                op.tool
            )];
        };
        legs.push((name.clone(), cache.path.moves.len(), cache.path, tool.width));
    }
    if legs.is_empty() {
        // Asked for a measurement of a part no blade cuts - the
        // asking Part op itself is always a leg, so this is the
        // lost-operation case; name it rather than judge nothing.
        return vec![format!(
            "probe ask on {}: the program cuts this part with no blade - nothing to measure",
            head.name
        )];
    }
    let refs: Vec<BladeLeg> = legs
        .iter()
        .map(|(_, _, path, width)| BladeLeg { path, width: *width })
        .collect();
    let mut out = Vec::new();
    // #904: on a model imported coarser than promised the composed
    // scan's gouge clause gives the excess back; an ask the excess uses
    // up is refused by name at the fidelity clause (`floor_of`).
    for w in turning::blade_program_warnings_excess(&material.facets, &refs, budget, material.excess) {
        match w {
            // The part's deviation, whichever clause was worse - a
            // probe reads the surface off the model either way.
            Warning::Envelope {
                at,
                gouge_mm,
                rest_mm,
            } => {
                let (blamed, mv) = blame(&legs, at);
                let (word, mm) = if gouge_mm >= rest_mm {
                    ("cut below the model", gouge_mm)
                } else {
                    ("left standing on the part", rest_mm)
                };
                out.push(format!(
                    "probe ask {budget:.4} mm ({}): {mm:.4} mm {word} by {blamed} at move {mv} - the part as machined cannot measure to it; run the pass that takes the steel off, or re-run the named op to hold the ask, then post again",
                    asked.join(", ")
                ));
            }
            // The scan's own refusal: no frame, no measurement, no
            // post. The per-op verdicts speak any first-cut gouge
            // this composes; this line is the ask's, and the ask
            // deserves its own verdict.
            Warning::Gouge { at } => out.push(format!(
                "probe ask {:.4} mm ({}): the composed part is unmeasurable (no frame at move {at}) - a judge that cannot measure never posts",
                budget,
                asked.join(", ")
            )),
            _ => {}
        }
    }
    out
}

/// Whom to blame for a composed move index: the leg whose program
/// range it stands in, and the move number within that op.
fn blame(legs: &[(String, usize, ClPath, f64)], at: usize) -> (String, usize) {
    let mut start = 0usize;
    for (name, len, _, _) in legs {
        if at < start + len {
            return (name.clone(), at - start);
        }
        start += len;
    }
    legs.last()
        .map_or((String::new(), at), |(n, _, _, _)| (n.clone(), at))
}
