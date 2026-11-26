//! The verification gate's plasma clause (#672): the through-cut laws
//! of `sim::verify::plasma` run over every Plasma Profile op's cached
//! path, made binding - a refusal blocks the post the way a located
//! gouge does - and the not-judged line that says the 3D stock sim did
//! not run for the torch and what judged it instead.
//! Provenance: our own design (the Prime Directive's "correct, or
//! visibly refused" read onto the M2.3 D gate, as `posting_sim` does
//! for the stock sim); the laws cite UM10720 and the LinuxCNC Plasma
//! Primer in their own module.
//!
//! ## The law here
//!
//! The material sim (`stock_sim`) carves nothing for a torch: the dexel
//! column has no atom for a kerf cut through the plate, so it skips
//! the op and its verdict says nothing about it. That silence must not
//! read as a pass. Every Plasma Profile in the program is judged here
//! from its own CL records (`PlasmaProgram::from_toolpath` - the torch
//! spans, never re-parsed text): cut order, cut crossing and a malformed
//! record refuse by name; kerf crossing and tip-up warn, named on the
//! line. An op with no path, or a path with no record, is a problem in
//! its own right - two missing judges never add up to a pass.

use crate::model;
use crate::operation::OpParams;
use crate::verify::plasma::{judge, PlasmaFinding, PlasmaProgram};
use flecs_ecs::prelude::*;

/// The clause's own words on the not-judged line, said once.
pub const PLASMA_VERDICT: &str = "the 3D stock sim does not run for plasma";

/// The plasma clause's outputs: the blocking problems (a refusal per
/// law, an op the laws cannot read), the visible, non-blocking line
/// naming the ops judged and their warnings, and (#3456) each of those
/// warnings as data, so `verify` lists it as a finding and on its op.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct PlasmaClause {
    pub problems: Vec<String>,
    pub line: Option<String>,
    pub warnings: Vec<GateWarning>,
}

/// #3456: one non-blocking finding of a program law, for one operation:
/// `op` is the operation's key, `kind` the finding's stable name (the
/// MCP finding kind), `message` the text without the op prefix.
#[derive(Debug, Clone, PartialEq)]
pub struct GateWarning {
    pub op: String,
    pub kind: &'static str,
    pub message: String,
}

/// The stable name of a plasma law's warning; `None` for a refusal
/// (a refusal is a gate problem, never a warning).
fn warning_kind(f: &PlasmaFinding) -> Option<&'static str> {
    match f {
        PlasmaFinding::KerfCrossing { .. } => Some("plasma_kerf_crossing"),
        PlasmaFinding::TipUp { .. } => Some("plasma_tip_up"),
        PlasmaFinding::CutOrder { .. } | PlasmaFinding::CutCrossing { .. } | PlasmaFinding::Malformed(_) => None,
    }
}

/// Judge every Plasma Profile of the program (document order, suppressed
/// ops left out - the same walk `program_ops` makes).
#[must_use]
pub fn plasma_clause(world: &World) -> PlasmaClause {
    let mut out = PlasmaClause::default();
    let mut judged: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    for (name, e) in crate::setup::operation_order(world) {
        let Some(op) = model::operation_of(world, &e) else { continue };
        if op.suppressed || !matches!(op.params, OpParams::PlasmaProfile(_)) {
            continue;
        }
        judged.push(name.clone());
        let Some(cache) = model::toolpath_of(world, &e) else {
            out.problems.push(format!("{name}: the plasma laws have no toolpath to judge - generate it first"));
            continue;
        };
        let program = match PlasmaProgram::from_toolpath(&cache.path) {
            Ok(p) => p,
            Err(why) => {
                out.problems.push(format!("{name}: the plasma laws cannot judge the path - {why}"));
                continue;
            }
        };
        for f in judge(&program) {
            let message = format!("plasma {}", describe(world, &f));
            let line = format!("{name}: {message}");
            match warning_kind(&f) {
                Some(kind) if !f.refuses() => {
                    warnings.push(line);
                    out.warnings.push(GateWarning { op: name.clone(), kind, message });
                }
                _ => out.problems.push(line),
            }
        }
    }
    if !judged.is_empty() {
        let verdict = if warnings.is_empty() {
            "clean".to_owned()
        } else {
            format!("{} warning(s) - {}", warnings.len(), warnings.join("; "))
        };
        out.line = Some(format!(
            "{PLASMA_VERDICT} - {} judged by the 2D plasma laws (cut order, cut crossing, kerf crossing, tip-up): {verdict}",
            judged.join(", ")
        ));
    }
    out
}

/// The finding in the document's units.
fn describe(world: &World, f: &PlasmaFinding) -> String {
    let u = crate::units::of(world);
    match f.at() {
        Some(q) => format!(
            "{} near ({}, {}) {}",
            f.describe(),
            crate::units::len(q.x, u, 2),
            crate::units::len(q.y, u, 2),
            u.suffix()
        ),
        None => f.describe(),
    }
}
